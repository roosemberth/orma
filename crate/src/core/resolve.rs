//! Checking a volume against the schema a system expects of it.

use crate::core::field_type::Invalid;
use crate::core::schema::{Field, FieldPath, Schema};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Reach a verdict and write nothing.
    EvaluateOnly,
    /// Reach a verdict, then lay the values at an output path.
    Write,
}

/// Each step carries a request to perform an action in the world.
/// Upon completing it, the driver should answer the request.
#[must_use = "resolve makes no progress until the step is carried out"]
pub enum Step<'r, 's> {
    ReadValue(ReadValue<'r, 's>),
    Done(Result<(), ResolveError>),
}

/// Read the value of a file within the volume.
#[must_use = "the read has to be answered for resolve to go on"]
pub struct ReadValue<'r, 's> {
    resolve: &'r mut Resolve<'s>,
    field: &'s Field,
}

impl<'s> ReadValue<'_, 's> {
    /// Where inside the volume the value is stored.
    pub fn path(&self) -> &'s FieldPath {
        self.field.path()
    }

    /// The bytes stored there.
    pub fn found(self, value: &[u8]) {
        let refused = self.field.kind().validate(value).err().map(Reason::Invalid);
        self.settle(refused);
    }

    /// Nothing is stored there.
    pub fn absent(self) {
        self.settle(Some(Reason::Missing));
    }

    /// Something is stored there but it could not be read.
    pub fn unreadable(self, why: String) {
        self.settle(Some(Reason::Unreadable(why)));
    }

    fn settle(self, refused: Option<Reason>) {
        if let Some(reason) = refused {
            self.resolve.rejections.push(Rejection {
                path: self.field.path().clone(),
                reason,
            });
        }
        self.resolve.current_field_index += 1;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("{}", .0.iter().map(Rejection::to_string).collect::<Vec<_>>().join("\n"))]
    Unsatisfied(Vec<Rejection>),
    #[error("laying the values at an output path is not implemented")]
    UnimplementedWrite,
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

/// The resolve operation.
/// Every field is evaluated, so a single run reports everything wrong with
/// the volume rather than only the first fault.
#[derive(Debug)]
pub struct Resolve<'s> {
    schema: &'s Schema,
    mode: Mode,
    current_field_index: usize,
    rejections: Vec<Rejection>,
}

impl<'s> Resolve<'s> {
    pub fn new(schema: &'s Schema, mode: Mode) -> Resolve<'s> {
        Resolve {
            schema,
            mode,
            current_field_index: 0,
            rejections: Vec::new(),
        }
    }

    pub fn step(&mut self) -> Step<'_, 's> {
        if self.mode == Mode::Write {
            return Step::Done(Err(ResolveError::UnimplementedWrite));
        }
        match self.schema.fields().get(self.current_field_index) {
            Some(field) => Step::ReadValue(ReadValue {
                resolve: self,
                field,
            }),
            None => Step::Done(self.verdict()),
        }
    }

    fn verdict(&mut self) -> Result<(), ResolveError> {
        let rejections = std::mem::take(&mut self.rejections);
        if rejections.is_empty() {
            Ok(())
        } else {
            Err(ResolveError::Unsatisfied(rejections))
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

    fn drive(schema: &Schema, answers: Vec<Answer>) -> Result<(), ResolveError> {
        let mut answers = answers.into_iter();
        let mut resolve = Resolve::new(schema, Mode::EvaluateOnly);
        loop {
            match resolve.step() {
                Step::ReadValue(read) => match answers.next().unwrap() {
                    Answer::Value(value) => read.found(value),
                    Answer::Absent => read.absent(),
                    Answer::Unreadable(why) => read.unreadable(why.to_owned()),
                },
                Step::Done(verdict) => return verdict,
            }
        }
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
        assert!(drive(&schema, vec![Answer::Value(MACHINE_ID)]).is_ok());
    }

    #[test]
    fn a_missing_value_is_rejected() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let err = drive(&schema, vec![Answer::Absent]).unwrap_err();
        assert_eq!(err.to_string(), "/machine-id: required but missing");
    }

    #[test]
    fn a_value_that_cannot_be_read_is_rejected() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let err = drive(&schema, vec![Answer::Unreadable("denied")]).unwrap_err();
        assert_eq!(err.to_string(), "/machine-id: could not be read: denied");
    }

    #[test]
    fn a_value_its_type_refuses_is_rejected() {
        let schema = schema(vec![fixtures::field("/machine-id", "machine-id")]);
        let err = drive(&schema, vec![Answer::Value(b"nope")]).unwrap_err();
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
        let err = drive(&schema, vec![Answer::Absent, Answer::Value(b"nope")]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "/machine-id: required but missing\n\
             /other-id: expected 32 characters, found 4"
        );
    }

    #[test]
    fn writing_the_values_is_not_implemented() {
        let schema = schema(vec![]);
        let mut resolve = Resolve::new(&schema, Mode::Write);
        assert!(matches!(
            resolve.step(),
            Step::Done(Err(ResolveError::UnimplementedWrite))
        ));
    }
}
