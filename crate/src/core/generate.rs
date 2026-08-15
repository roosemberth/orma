//! Populating an identity volume with the fields a schema declares.

use crate::core::field_type::{Invalid, Recipe};
use crate::core::schema::{Field, FieldPath, Schema};

/// What generate does about the values a volume already holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Populate a volume that holds nothing.
    Populate,
    /// Only populate missing fields.
    Upgrade,
}

/// Each step carries a request to perform an action in the world.
/// Upon completing it, the driver should answer the request.
#[must_use = "generate makes no progress until the step is carried out"]
pub enum Step<'r, 's> {
    CheckValue(CheckValue<'r, 's>),
    DrawEntropy(DrawEntropy<'r, 's>),
    HashPassphrase(GetHashedPassphrase<'r, 's>),
    WriteValue(WriteValue<'r, 's>),
    Done(Result<(), GenerateError>),
}

/// Report what the volume already holds for this field, if anything.
#[must_use = "the check has to be answered for generate to go on"]
pub struct CheckValue<'r, 's> {
    generate: &'r mut Generate<'s>,
    field: &'s Field,
    current_field_idx: usize,
}

impl<'s> CheckValue<'_, 's> {
    /// Where inside the volume the value would be stored.
    pub fn path(&self) -> &'s FieldPath {
        self.field.path()
    }

    /// A value is already stored there.
    pub fn present(self, value: &[u8]) {
        if let Some(held) = self.generate.held.get_mut(self.current_field_idx) {
            *held = true;
        }
        self.generate.present.push(Held {
            path: self.field.path().clone(),
            invalid: self.field.kind().validate(value).err(),
        });
        self.advance();
    }

    /// Nothing is stored there.
    pub fn absent(self) {
        self.advance();
    }

    pub fn failed(self, why: String) {
        self.generate.fail(self.field, why);
    }

    fn advance(self) {
        self.generate.phase = GeneratePhase::CheckField(self.current_field_idx + 1);
    }
}

/// Draw randomness from the kernel for a field that requires it.
#[must_use = "the draw has to be answered for generate to go on"]
pub struct DrawEntropy<'r, 's> {
    generate: &'r mut Generate<'s>,
    field: &'s Field,
    current_field_idx: usize,
    bytes: usize,
    build: fn(&[u8]) -> Vec<u8>,
}

impl<'s> DrawEntropy<'_, 's> {
    /// How many bytes the field's type asks for.
    pub fn wanted(&self) -> usize {
        self.bytes
    }

    /// Submit the drawn randomness.
    pub fn filled(self, entropy: &[u8]) {
        let (field, at) = (self.field, self.current_field_idx);
        let value = (self.build)(entropy);
        self.generate.accept(field, at, value);
    }

    pub fn failed(self, why: String) {
        self.generate.fail(self.field, why);
    }
}

/// Request a passphrase from the operator and hash it.
/// Both steps are performed by the driver, so we never have the raw passphrase.
#[must_use = "the hashed password has to be answered for generate to go on"]
pub struct GetHashedPassphrase<'r, 's> {
    generate: &'r mut Generate<'s>,
    field: &'s Field,
    current_field_idx: usize,
}

impl<'s> GetHashedPassphrase<'_, 's> {
    /// Which field the operator is being asked about.
    pub fn path(&self) -> &'s FieldPath {
        self.field.path()
    }

    pub fn description(&self) -> Option<&'s str> {
        self.field.description()
    }

    /// Record the hashed passphrase.
    pub fn hashed(self, record: &[u8]) {
        let field = self.field;
        let at = self.current_field_idx;
        self.generate.accept(field, at, record.to_vec());
    }

    pub fn failed(self, why: String) {
        self.generate.fail(self.field, why);
    }
}

/// Write a produced value into the volume.
#[must_use = "the write has to be answered for generate to go on"]
pub struct WriteValue<'r, 's> {
    generate: &'r mut Generate<'s>,
    field: &'s Field,
    current_field_idx: usize,
}

impl<'s> WriteValue<'_, 's> {
    /// Where inside the volume the value belongs.
    pub fn path(&self) -> &'s FieldPath {
        self.field.path()
    }

    /// The mode the value is to be stored under.
    pub fn permissions(&self) -> u32 {
        self.field.kind().permissions()
    }

    pub fn value(&self) -> &[u8] {
        self.generate
            .values
            .get(self.current_field_idx)
            .and_then(Option::as_deref)
            .unwrap_or_default()
    }

    pub fn written(self) {
        self.generate.phase = GeneratePhase::WriteField(self.current_field_idx + 1);
    }

    pub fn failed(self, why: String) {
        self.generate.fail(self.field, why);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    #[error("the volume already holds:\n{}", .0.iter().map(Held::to_string).collect::<Vec<_>>().join("\n"))]
    AlreadyHeld(Vec<Held>),
    #[error("the volume holds invalid values:\n{}", .0.iter().map(Held::to_string).collect::<Vec<_>>().join("\n"))]
    InvalidValues(Vec<Held>),
    #[error("{path}: {why}")]
    Failed { path: FieldPath, why: String },
    #[error("{path}: produced a value its own type refuses: {reason}")]
    Unusable { path: FieldPath, reason: Invalid },
    #[error("{path}: producing a '{type_name}' is not implemented")]
    Unable {
        path: FieldPath,
        type_name: &'static str,
    },
}

/// A value the volume was already holding, and what its field type made of it.
#[derive(Debug)]
pub struct Held {
    pub path: FieldPath,
    pub invalid: Option<Invalid>,
}

impl std::fmt::Display for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.invalid {
            Some(invalid) => write!(f, "{}: {invalid}", self.path),
            None => write!(f, "{}", self.path),
        }
    }
}

#[derive(Debug)]
enum GeneratePhase {
    CheckField(usize),
    MakeField(usize),
    WriteField(usize),
    Done,
}

/// The generate operation.
///
/// Produces a value for every field the schema declares and stores it in the
/// identity volume. The volume is surveyed in full before any change is made.
#[derive(Debug)]
pub struct Generate<'s> {
    schema: &'s Schema,
    mode: Mode,
    phase: GeneratePhase,
    present: Vec<Held>,
    /// Which fields the volume was already holding, by declaration order.
    held: Vec<bool>,
    values: Vec<Option<Vec<u8>>>,
    failure: Option<GenerateError>,
}

impl<'s> Generate<'s> {
    pub fn new(schema: &'s Schema, mode: Mode) -> Generate<'s> {
        Generate {
            schema,
            mode,
            phase: GeneratePhase::CheckField(0),
            present: Vec::new(),
            held: vec![false; schema.fields().len()],
            values: Vec::new(),
            failure: None,
        }
    }

    pub fn step(&mut self) -> Step<'_, 's> {
        if let Some(failure) = self.failure.take() {
            return Step::Done(Err(failure));
        }
        match self.phase {
            GeneratePhase::CheckField(at) => match self.schema.fields().get(at) {
                Some(field) => Step::CheckValue(CheckValue {
                    generate: self,
                    field,
                    current_field_idx: at,
                }),
                None => self.survey(),
            },
            GeneratePhase::MakeField(at) => match self.schema.fields().get(at) {
                // Don't generate existing or optional values
                Some(field)
                    if field.is_optional() || self.held.get(at).copied().unwrap_or(false) =>
                {
                    self.values.push(None);
                    self.phase = GeneratePhase::MakeField(at + 1);
                    self.step()
                }
                Some(field) => match field.kind().recipe() {
                    Some(Recipe::FromEntropy { bytes, build }) => Step::DrawEntropy(DrawEntropy {
                        generate: self,
                        field,
                        current_field_idx: at,
                        bytes,
                        build,
                    }),
                    Some(Recipe::FromPassphrasePrompt) => {
                        Step::HashPassphrase(GetHashedPassphrase {
                            generate: self,
                            field,
                            current_field_idx: at,
                        })
                    }
                    None => {
                        self.failure = Some(GenerateError::Unable {
                            path: field.path().clone(),
                            type_name: field.kind().name(),
                        });
                        self.phase = GeneratePhase::Done;
                        self.step()
                    }
                },
                None => {
                    self.phase = GeneratePhase::WriteField(0);
                    self.step()
                }
            },
            GeneratePhase::WriteField(at) => match self.schema.fields().get(at) {
                // An optional field is not generated.
                Some(_) if self.values.get(at).is_none_or(Option::is_none) => {
                    self.phase = GeneratePhase::WriteField(at + 1);
                    self.step()
                }
                Some(field) => Step::WriteValue(WriteValue {
                    generate: self,
                    field,
                    current_field_idx: at,
                }),
                None => {
                    self.phase = GeneratePhase::Done;
                    Step::Done(Ok(()))
                }
            },
            GeneratePhase::Done => Step::Done(Ok(())),
        }
    }

    /// Survey the existing fields in the volume.
    fn survey(&mut self) -> Step<'_, 's> {
        let present = std::mem::take(&mut self.present);
        let refusal = match self.mode {
            Mode::Populate if !present.is_empty() => Some(GenerateError::AlreadyHeld(present)),
            Mode::Upgrade => {
                let invalid: Vec<Held> = present
                    .into_iter()
                    .filter(|held| held.invalid.is_some())
                    .collect();
                match invalid.is_empty() {
                    true => None,
                    false => Some(GenerateError::InvalidValues(invalid)),
                }
            }
            Mode::Populate => None,
        };
        if let Some(refusal) = refusal {
            self.phase = GeneratePhase::Done;
            return Step::Done(Err(refusal));
        }
        self.phase = GeneratePhase::MakeField(0);
        self.step()
    }

    /// Check a produced value using the field validator and accept it.
    fn accept(&mut self, field: &Field, field_idx: usize, value: Vec<u8>) {
        match field.kind().validate(&value) {
            Ok(()) => {
                self.values.push(Some(value));
                self.phase = GeneratePhase::MakeField(field_idx + 1);
            }
            Err(reason) => {
                self.failure = Some(GenerateError::Unusable {
                    path: field.path().clone(),
                    reason,
                });
                self.phase = GeneratePhase::Done;
            }
        }
    }

    fn fail(&mut self, field: &Field, why: String) {
        self.failure = Some(GenerateError::Failed {
            path: field.path().clone(),
            why,
        });
        self.phase = GeneratePhase::Done;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::schema::file;
    use crate::core::schema::file::fixtures;

    enum FieldValueHeld {
        Value(&'static [u8]),
        Nothing,
    }

    fn schema(fields: Vec<file::Field>) -> Schema {
        Schema::new(fixtures::schema(fields)).unwrap()
    }

    /// The path and bytes a run produced.
    type Produced = Vec<(String, Vec<u8>)>;

    /// Drive a generate to its end, answering each check from `held` and every
    /// draw with `entropy`. Returns the outcome alongside what was written.
    fn drive(
        schema: &Schema,
        mode: Mode,
        held: Vec<FieldValueHeld>,
        entropy: &[u8],
    ) -> (Result<(), GenerateError>, Produced) {
        let mut held = held.into_iter();
        let mut written = Vec::new();
        let mut generate = Generate::new(schema, mode);
        loop {
            match generate.step() {
                Step::CheckValue(check) => match held.next().unwrap() {
                    FieldValueHeld::Value(value) => check.present(value),
                    FieldValueHeld::Nothing => check.absent(),
                },
                Step::DrawEntropy(draw) => {
                    assert_eq!(draw.wanted(), entropy.len());
                    draw.filled(entropy);
                }
                Step::HashPassphrase(hash) => hash.hashed(CRYPT_RECORD),
                Step::WriteValue(write) => {
                    written.push((write.path().as_str().to_owned(), write.value().to_vec()));
                    write.written();
                }
                Step::Done(outcome) => return (outcome, written),
            }
        }
    }

    const CRYPT_RECORD: &[u8] = b"$y$j9T$saltSaltSalt$hashHashHash";

    const MACHINE_ID: &[u8] = b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1";

    const ENTROPY: &[u8] = &[
        0xd2, 0xc8, 0xe7, 0xe9, 0xa4, 0xb3, 0x4d, 0x62, //
        0xb8, 0xf8, 0xa0, 0xc5, 0xe9, 0xd7, 0xf3, 0xb1,
    ];

    #[test]
    fn a_schema_declaring_nothing_produces_nothing() {
        let schema = schema(vec![]);
        let (outcome, written) = drive(&schema, Mode::Populate, vec![], ENTROPY);
        assert!(outcome.is_ok());
        assert!(written.is_empty());
    }

    #[test]
    fn a_machine_id_is_made_out_of_the_randomness_drawn() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let (outcome, written) = drive(
            &schema,
            Mode::Populate,
            vec![FieldValueHeld::Nothing],
            ENTROPY,
        );

        assert!(outcome.is_ok());
        assert_eq!(
            written,
            vec![(
                "/machine-id".to_owned(),
                b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1\n".to_vec()
            )]
        );
    }

    #[test]
    fn a_volume_already_holding_a_value_is_refused() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/other-id", "machine-id"),
        ]);
        let (outcome, written) = drive(
            &schema,
            Mode::Populate,
            vec![FieldValueHeld::Nothing, FieldValueHeld::Value(MACHINE_ID)],
            ENTROPY,
        );

        assert_eq!(
            outcome.unwrap_err().to_string(),
            "the volume already holds:\n/other-id"
        );
        assert!(written.is_empty());
    }

    #[test]
    fn a_hashed_password_is_asked_for_and_accepted_as_produced() {
        let schema = schema(vec![fixtures::field("/user.passwd", "hashed-password")]);
        let (outcome, written) = drive(
            &schema,
            Mode::Populate,
            vec![FieldValueHeld::Nothing],
            ENTROPY,
        );

        assert!(outcome.is_ok());
        assert_eq!(
            written,
            vec![("/user.passwd".to_owned(), CRYPT_RECORD.to_vec())]
        );
    }

    #[test]
    fn a_hashing_that_produces_nonsense_is_refused() {
        let schema = schema(vec![fixtures::field("/user.passwd", "hashed-password")]);
        let mut generate = Generate::new(&schema, Mode::Populate);
        let outcome = loop {
            match generate.step() {
                Step::CheckValue(check) => check.absent(),
                Step::HashPassphrase(hash) => hash.hashed(b"hunter2"),
                Step::WriteValue(write) => write.written(),
                Step::DrawEntropy(_) => panic!("unexpected step for this test"),
                Step::Done(outcome) => break outcome,
            }
        };
        assert!(
            outcome
                .unwrap_err()
                .to_string()
                .contains("produced a value its own type refuses")
        );
    }

    #[test]
    fn an_optional_field_is_not_produced() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::optional_field("/sudo.passwd", "hashed-password"),
        ]);
        let (outcome, written) = drive(
            &schema,
            Mode::Populate,
            vec![FieldValueHeld::Nothing, FieldValueHeld::Nothing],
            ENTROPY,
        );

        assert!(outcome.is_ok());
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0, "/machine-id");
    }

    #[test]
    fn an_optional_value_already_held_stops_the_run() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::optional_field("/sudo.passwd", "hashed-password"),
        ]);
        let (outcome, written) = drive(
            &schema,
            Mode::Populate,
            vec![FieldValueHeld::Nothing, FieldValueHeld::Value(CRYPT_RECORD)],
            ENTROPY,
        );
        assert_eq!(
            outcome.unwrap_err().to_string(),
            "the volume already holds:\n/sudo.passwd"
        );
        assert!(written.is_empty());
    }

    #[test]
    fn a_held_value_its_type_refuses_is_named_with_its_fault() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let (outcome, written) = drive(
            &schema,
            Mode::Populate,
            vec![FieldValueHeld::Value(b"nonsense")],
            ENTROPY,
        );

        assert_eq!(
            outcome.unwrap_err().to_string(),
            "the volume already holds:\n\
             /machine-id: expected 32 characters, found 8"
        );
        assert!(written.is_empty());
    }

    #[test]
    fn upgrading_produces_only_what_the_volume_lacks() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/user.passwd", "hashed-password"),
        ]);
        let (outcome, written) = drive(
            &schema,
            Mode::Upgrade,
            vec![FieldValueHeld::Value(MACHINE_ID), FieldValueHeld::Nothing],
            ENTROPY,
        );

        assert!(outcome.is_ok());
        assert_eq!(
            written,
            vec![("/user.passwd".to_owned(), CRYPT_RECORD.to_vec())]
        );
    }

    #[test]
    fn upgrading_a_volume_that_lacks_nothing_produces_nothing() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let (outcome, written) = drive(
            &schema,
            Mode::Upgrade,
            vec![FieldValueHeld::Value(MACHINE_ID)],
            ENTROPY,
        );

        assert!(outcome.is_ok());
        assert!(written.is_empty());
    }

    #[test]
    fn upgrading_refuses_a_volume_holding_an_invalid_value() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/user.passwd", "hashed-password"),
        ]);
        let (outcome, written) = drive(
            &schema,
            Mode::Upgrade,
            vec![FieldValueHeld::Value(b"nonsense"), FieldValueHeld::Nothing],
            ENTROPY,
        );

        assert_eq!(
            outcome.unwrap_err().to_string(),
            "the volume holds invalid values:\n\
             /machine-id: expected 32 characters, found 8"
        );
        assert!(written.is_empty());
    }

    #[test]
    fn the_whole_volume_is_surveyed_before_anything_is_produced() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/other-id", "machine-id"),
        ]);
        let mut generate = Generate::new(&schema, Mode::Populate);
        let mut checked = Vec::new();

        while let Step::CheckValue(check) = generate.step() {
            checked.push(check.path().as_str().to_owned());
            check.absent();
        }
        assert_eq!(checked, vec!["/machine-id", "/other-id"]);
    }

    #[test]
    fn a_write_that_fails_ends_the_run() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let mut generate = Generate::new(&schema, Mode::Populate);

        let outcome = loop {
            match generate.step() {
                Step::CheckValue(check) => check.absent(),
                Step::DrawEntropy(draw) => draw.filled(ENTROPY),
                Step::WriteValue(write) => write.failed("disk full".to_owned()),
                Step::HashPassphrase(_) => panic!("unexpected step for this test"),
                Step::Done(outcome) => break outcome,
            }
        };

        assert_eq!(outcome.unwrap_err().to_string(), "/machine-id: disk full");
    }

    #[test]
    fn randomness_that_falls_short_is_refused() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let mut generate = Generate::new(&schema, Mode::Populate);

        let outcome = loop {
            match generate.step() {
                Step::CheckValue(check) => check.absent(),
                Step::DrawEntropy(draw) => draw.filled(&[0x01, 0x02]),
                Step::WriteValue(write) => write.written(),
                Step::HashPassphrase(_) => panic!("unexpected step for this test"),
                Step::Done(outcome) => break outcome,
            }
        };

        assert!(
            outcome
                .unwrap_err()
                .to_string()
                .starts_with("/machine-id: produced a value its own type refuses")
        );
    }
}
