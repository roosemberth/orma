//! Checking a volume against the schema a system expects of it.

use crate::core::field_type::Invalid;
use crate::core::schema::{Field, FieldPath, Schema};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Reach a verdict and write nothing.
    EvaluateOnly,
    /// Reach a verdict, then provision the values at an output path.
    Write,
}

/// Each step carries a request to perform an action in the world.
/// Upon completing it, the driver should answer the request.
#[must_use = "resolve makes no progress until the step is carried out"]
pub enum Step<'r, 's> {
    ReadValue(ReadValue<'r, 's>),
    WriteValue(WriteValue<'r, 's>),
    Done(Result<(), ResolveError>),
}

/// Read the value of a file within the volume.
#[must_use = "the read has to be answered for resolve to go on"]
pub struct ReadValue<'r, 's> {
    resolve: &'r mut Resolve<'s>,
    field: &'s Field,
    current_field_idx: usize,
}

impl<'s> ReadValue<'_, 's> {
    /// Where inside the volume the value is stored.
    pub fn path(&self) -> &'s FieldPath {
        self.field.path()
    }

    /// The file contents were available.
    pub fn found(self, value: &[u8]) {
        match self.field.kind().validate(value) {
            Ok(()) => self.conclude(Some(value.to_vec()), None),
            Err(invalid) => self.conclude(None, Some(Reason::Invalid(invalid))),
        }
    }

    /// Nothing is stored there.
    pub fn absent(self) {
        match self.field.is_optional() {
            true => self.conclude(None, None),
            false => self.conclude(None, Some(Reason::Missing)),
        }
    }

    /// Something is stored there but it could not be read.
    pub fn unreadable(self, why: String) {
        self.conclude(None, Some(Reason::Unreadable(why)));
    }

    fn conclude(self, value: Option<Vec<u8>>, refused: Option<Reason>) {
        if let Some(reason) = refused {
            self.resolve.rejections.push(Rejection {
                path: self.field.path().clone(),
                reason,
            });
        }
        self.resolve.values.push(value);
        self.resolve.phase = ResolvePhase::ReadField(self.current_field_idx + 1);
    }
}

/// Write the value of a field at the output path.
#[must_use = "the write has to be answered for resolve to go on"]
pub struct WriteValue<'r, 's> {
    resolve: &'r mut Resolve<'s>,
    field: &'s Field,
    current_field_idx: usize,
}

impl<'s> WriteValue<'_, 's> {
    /// Where the value should be written, relative to the output path.
    pub fn path(&self) -> &'s FieldPath {
        self.field.path()
    }

    pub fn value(&self) -> &[u8] {
        self.resolve
            .values
            .get(self.current_field_idx)
            .and_then(Option::as_deref)
            .unwrap_or_default()
    }

    pub fn written(self) {
        self.resolve.phase = ResolvePhase::WriteField(self.current_field_idx + 1);
    }

    pub fn failed(self, why: String) {
        self.resolve.failure = Some(ResolveError::WriteFailed {
            path: self.field.path().clone(),
            why,
        });
        self.resolve.phase = ResolvePhase::Done;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("{}", .0.iter().map(Rejection::to_string).collect::<Vec<_>>().join("\n"))]
    Unsatisfied(Vec<Rejection>),
    #[error("{path}: could not be written: {why}")]
    WriteFailed { path: FieldPath, why: String },
}

#[derive(Debug)]
pub struct Rejection {
    pub path: FieldPath,
    pub reason: Reason,
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.reason)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Reason {
    #[error("required but missing")]
    Missing,
    #[error("could not be read: {0}")]
    Unreadable(String),
    #[error("{0}")]
    Invalid(#[from] Invalid),
}

#[derive(Debug)]
enum ResolvePhase {
    ReadField(usize),
    WriteField(usize),
    Done,
}

/// The resolve operation.
///
/// Verifies fields in the identity volume and writes them at the output path.
/// Every field is evaluated, so a single run reports everything wrong with
/// the volume rather than only the first fault.
#[derive(Debug)]
pub struct Resolve<'s> {
    schema: &'s Schema,
    mode: Mode,
    phase: ResolvePhase,
    values: Vec<Option<Vec<u8>>>,
    rejections: Vec<Rejection>,
    failure: Option<ResolveError>,
}

impl<'s> Resolve<'s> {
    pub fn new(schema: &'s Schema, mode: Mode) -> Resolve<'s> {
        Resolve {
            schema,
            mode,
            phase: ResolvePhase::ReadField(0),
            values: Vec::new(),
            rejections: Vec::new(),
            failure: None,
        }
    }

    pub fn step(&mut self) -> Step<'_, 's> {
        if let Some(failure) = self.failure.take() {
            return Step::Done(Err(failure));
        }
        match self.phase {
            ResolvePhase::ReadField(at) => match self.schema.fields().get(at) {
                Some(field) => Step::ReadValue(ReadValue {
                    resolve: self,
                    field,
                    current_field_idx: at,
                }),
                None => self.verdict(),
            },
            ResolvePhase::WriteField(at) => match self.schema.fields().get(at) {
                // A missing but not required field has nothing to provision.
                Some(_) if self.values.get(at).is_none_or(Option::is_none) => {
                    self.phase = ResolvePhase::WriteField(at + 1);
                    self.step()
                }
                Some(field) => Step::WriteValue(WriteValue {
                    resolve: self,
                    field,
                    current_field_idx: at,
                }),
                None => Step::Done(Ok(())),
            },
            ResolvePhase::Done => Step::Done(Ok(())),
        }
    }

    fn verdict(&mut self) -> Step<'_, 's> {
        let rejections = std::mem::take(&mut self.rejections);
        if !rejections.is_empty() {
            self.phase = ResolvePhase::Done;
            return Step::Done(Err(ResolveError::Unsatisfied(rejections)));
        }
        match self.mode {
            Mode::EvaluateOnly => {
                self.phase = ResolvePhase::Done;
                Step::Done(Ok(()))
            }
            Mode::Write => {
                self.phase = ResolvePhase::WriteField(0);
                self.step()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::schema::file;
    use crate::core::schema::file::fixtures;

    const MACHINE_ID: &[u8] = b"d2c8e7e9a4b34d62b8f8a0c5e9d7f3b1";

    enum Answer {
        Value(&'static [u8]),
        Absent,
        Unreadable(&'static str),
    }

    fn schema(fields: Vec<file::Field>) -> Schema {
        Schema::new(fixtures::schema(fields)).unwrap()
    }

    /// The path and bytes a run provisioned at the output.
    type Provisioned = Vec<(String, Vec<u8>)>;

    fn drive(
        schema: &Schema,
        mode: Mode,
        answers: Vec<Answer>,
    ) -> (Result<(), ResolveError>, Provisioned) {
        let mut answers = answers.into_iter();
        let mut written = Vec::new();
        let mut resolve = Resolve::new(schema, mode);
        loop {
            match resolve.step() {
                Step::ReadValue(read) => match answers.next().unwrap() {
                    Answer::Value(value) => read.found(value),
                    Answer::Absent => read.absent(),
                    Answer::Unreadable(why) => read.unreadable(why.to_owned()),
                },
                Step::WriteValue(write) => {
                    written.push((write.path().as_str().to_owned(), write.value().to_vec()));
                    write.written();
                }
                Step::Done(verdict) => return (verdict, written),
            }
        }
    }

    fn evaluate(schema: &Schema, answers: Vec<Answer>) -> Result<(), ResolveError> {
        drive(schema, Mode::EvaluateOnly, answers).0
    }

    #[test]
    fn a_schema_declaring_nothing_asks_for_nothing() {
        let schema = schema(vec![]);
        let mut resolve = Resolve::new(&schema, Mode::EvaluateOnly);
        assert!(matches!(resolve.step(), Step::Done(Ok(()))));
    }

    #[test]
    fn it_asks_for_each_declared_field_in_turn() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/other-id", "machine-id"),
        ]);
        let mut resolve = Resolve::new(&schema, Mode::EvaluateOnly);

        let mut asked = Vec::new();
        while let Step::ReadValue(read) = resolve.step() {
            asked.push(read.path().as_str().to_owned());
            read.found(MACHINE_ID);
        }
        assert_eq!(asked, vec!["/machine-id", "/other-id"]);
    }

    #[test]
    fn a_volume_holding_every_value_satisfies_the_schema() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        assert!(evaluate(&schema, vec![Answer::Value(MACHINE_ID)]).is_ok());
    }

    #[test]
    fn a_missing_value_is_rejected() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let err = evaluate(&schema, vec![Answer::Absent]).unwrap_err();
        assert_eq!(err.to_string(), "/machine-id: required but missing");
    }

    #[test]
    fn an_optional_value_may_be_absent() {
        let schema = schema(vec![fixtures::optional_field(
            "/sudo.passwd",
            "hashed-password",
        )]);
        let (verdict, provisioned) = drive(&schema, Mode::Write, vec![Answer::Absent]);

        assert!(verdict.is_ok());
        assert!(provisioned.is_empty());
    }

    #[test]
    fn an_optional_value_that_is_present_is_still_judged() {
        let schema = schema(vec![fixtures::optional_field(
            "/sudo.passwd",
            "hashed-password",
        )]);
        let err = evaluate(&schema, vec![Answer::Value(b"hunter2")]).unwrap_err();

        assert_eq!(
            err.to_string(),
            "/sudo.passwd: not a crypt record: missing leading '$'"
        );
    }

    #[test]
    fn a_value_that_cannot_be_read_is_rejected() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let err = evaluate(&schema, vec![Answer::Unreadable("denied")]).unwrap_err();
        assert_eq!(err.to_string(), "/machine-id: could not be read: denied");
    }

    #[test]
    fn a_value_its_type_refuses_is_rejected() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let err = evaluate(&schema, vec![Answer::Value(b"nope")]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "/machine-id: expected 32 characters, found 4"
        );
    }

    #[test]
    fn every_field_is_judged_before_the_verdict() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/other-id", "machine-id"),
        ]);
        let err = evaluate(&schema, vec![Answer::Absent, Answer::Value(b"nope")]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "/machine-id: required but missing\n\
             /other-id: expected 32 characters, found 4"
        );
    }

    #[test]
    fn evaluating_only_writes_nothing() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let (verdict, written) =
            drive(&schema, Mode::EvaluateOnly, vec![Answer::Value(MACHINE_ID)]);
        assert!(verdict.is_ok());
        assert!(written.is_empty());
    }

    #[test]
    fn accepted_values_are_provisioned_at_the_output() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/other-id", "machine-id"),
        ]);
        let (verdict, written) = drive(
            &schema,
            Mode::Write,
            vec![Answer::Value(MACHINE_ID), Answer::Value(MACHINE_ID)],
        );

        assert!(verdict.is_ok());
        assert_eq!(
            written,
            vec![
                ("/machine-id".to_owned(), MACHINE_ID.to_vec()),
                ("/other-id".to_owned(), MACHINE_ID.to_vec()),
            ]
        );
    }

    /// One bad field costs the whole volume: the output is never touched.
    #[test]
    fn a_volume_that_fails_its_schema_provisions_nothing() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/other-id", "machine-id"),
        ]);
        let (verdict, written) = drive(
            &schema,
            Mode::Write,
            vec![Answer::Value(MACHINE_ID), Answer::Absent],
        );

        assert!(verdict.is_err());
        assert!(written.is_empty());
    }

    #[test]
    fn a_write_that_fails_ends_the_run() {
        let schema = schema(vec![
            fixtures::field("/machine-id", "machine-id"),
            fixtures::field("/other-id", "machine-id"),
        ]);
        let mut resolve = Resolve::new(&schema, Mode::Write);
        let mut written = Vec::new();

        let verdict = loop {
            match resolve.step() {
                Step::ReadValue(read) => read.found(MACHINE_ID),
                Step::WriteValue(write) => {
                    written.push(write.path().as_str().to_owned());
                    write.failed("disk full".to_owned());
                }
                Step::Done(verdict) => break verdict,
            }
        };

        assert_eq!(written, vec!["/machine-id"]);
        assert_eq!(
            verdict.unwrap_err().to_string(),
            "/machine-id: could not be written: disk full"
        );
    }
}
