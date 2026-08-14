//! Tools the platform shells out to.

use std::fmt;
use std::process::Command;

/// A tool orma may invoke.
#[derive(Debug, Clone, Copy)]
pub struct Tool(&'static str);

impl Tool {
    pub const MKPASSWD: Tool = Tool("mkpasswd");

    pub fn command(self) -> Command {
        Command::new(self.0)
    }

    pub fn failed(self, why: impl fmt::Display) -> Error {
        Error {
            tool: self,
            why: why.to_string(),
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{tool}: {why}")]
pub struct Error {
    pub tool: Tool,
    pub why: String,
}
