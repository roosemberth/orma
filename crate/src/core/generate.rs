//! Populating an identity volume with the fields a schema declares.

use crate::core::field_type::{Invalid, Recipe};
use crate::core::schema::{Field, FieldPath, Schema};

/// Each step carries a request to perform an action in the world.
/// Upon completing it, the driver should answer the request.
#[must_use = "generate makes no progress until the step is carried out"]
pub enum Step<'r, 's> {
    CheckValue(CheckValue<'r, 's>),
    DrawEntropy(DrawEntropy<'r, 's>),
    WriteValue(WriteValue<'r, 's>),
    Done(Result<(), GenerateError>),
}

/// Report whether the volume already holds a value for this field.
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
    pub fn present(self) {
        self.generate.present.push(self.field.path().clone());
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
        let value = (self.build)(entropy);
        // What generate produces has to be what resolve would accept, so the
        // value is held to its own type before it goes anywhere.
        match self.field.kind().validate(&value) {
            Ok(()) => {
                self.generate.values.push(value);
                self.generate.phase = GeneratePhase::MakeField(self.current_field_idx + 1);
            }
            Err(reason) => {
                self.generate.failure = Some(GenerateError::Unusable {
                    path: self.field.path().clone(),
                    reason,
                });
                self.generate.phase = GeneratePhase::Done;
            }
        }
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

    pub fn value(&self) -> &[u8] {
        self.generate
            .values
            .get(self.current_field_idx)
            .map_or(&[], Vec::as_slice)
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
    #[error("would overwrite:\n{}", .0.iter().map(FieldPath::to_string).collect::<Vec<_>>().join("\n"))]
    WouldOverwrite(Vec<FieldPath>),
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
    phase: GeneratePhase,
    present: Vec<FieldPath>,
    values: Vec<Vec<u8>>,
    failure: Option<GenerateError>,
}

impl<'s> Generate<'s> {
    pub fn new(schema: &'s Schema) -> Generate<'s> {
        Generate {
            schema,
            phase: GeneratePhase::CheckField(0),
            present: Vec::new(),
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
                Some(field) => match field.kind().recipe() {
                    Some(Recipe::FromEntropy { bytes, build }) => Step::DrawEntropy(DrawEntropy {
                        generate: self,
                        field,
                        current_field_idx: at,
                        bytes,
                        build,
                    }),
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
        if !present.is_empty() {
            self.phase = GeneratePhase::Done;
            return Step::Done(Err(GenerateError::WouldOverwrite(present)));
        }
        self.phase = GeneratePhase::MakeField(0);
        self.step()
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
        Value,
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
        held: Vec<FieldValueHeld>,
        entropy: &[u8],
    ) -> (Result<(), GenerateError>, Produced) {
        let mut held = held.into_iter();
        let mut written = Vec::new();
        let mut generate = Generate::new(schema);
        loop {
            match generate.step() {
                Step::CheckValue(check) => match held.next().unwrap() {
                    FieldValueHeld::Value => check.present(),
                    FieldValueHeld::Nothing => check.absent(),
                },
                Step::DrawEntropy(draw) => {
                    assert_eq!(draw.wanted(), entropy.len());
                    draw.filled(entropy);
                }
                Step::WriteValue(write) => {
                    written.push((write.path().as_str().to_owned(), write.value().to_vec()));
                    write.written();
                }
                Step::Done(outcome) => return (outcome, written),
            }
        }
    }

    const ENTROPY: &[u8] = &[
        0xd2, 0xc8, 0xe7, 0xe9, 0xa4, 0xb3, 0x4d, 0x62, //
        0xb8, 0xf8, 0xa0, 0xc5, 0xe9, 0xd7, 0xf3, 0xb1,
    ];

    #[test]
    fn a_schema_declaring_nothing_produces_nothing() {
        let schema = schema(vec![]);
        let (outcome, written) = drive(&schema, vec![], ENTROPY);
        assert!(outcome.is_ok());
        assert!(written.is_empty());
    }

    #[test]
    fn a_machine_id_is_made_out_of_the_randomness_drawn() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let (outcome, written) = drive(&schema, vec![FieldValueHeld::Nothing], ENTROPY);

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
    fn an_existing_value_is_not_overwritten() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/other-id", "machine-id"),
        ]);
        let (outcome, written) = drive(
            &schema,
            vec![FieldValueHeld::Nothing, FieldValueHeld::Value],
            ENTROPY,
        );

        assert_eq!(
            outcome.unwrap_err().to_string(),
            "would overwrite:\n/other-id"
        );
        assert!(written.is_empty());
    }

    #[test]
    fn the_whole_volume_is_surveyed_before_anything_is_produced() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/other-id", "machine-id"),
        ]);
        let mut generate = Generate::new(&schema);
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
        let mut generate = Generate::new(&schema);

        let outcome = loop {
            match generate.step() {
                Step::CheckValue(check) => check.absent(),
                Step::DrawEntropy(draw) => draw.filled(ENTROPY),
                Step::WriteValue(write) => write.failed("disk full".to_owned()),
                Step::Done(outcome) => break outcome,
            }
        };

        assert_eq!(outcome.unwrap_err().to_string(), "/machine-id: disk full");
    }

    #[test]
    fn randomness_that_falls_short_is_refused() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let mut generate = Generate::new(&schema);

        let outcome = loop {
            match generate.step() {
                Step::CheckValue(check) => check.absent(),
                Step::DrawEntropy(draw) => draw.filled(&[0x01, 0x02]),
                Step::WriteValue(write) => write.written(),
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
