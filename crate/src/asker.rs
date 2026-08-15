use std::collections::VecDeque;
use std::io::{BufRead, IsTerminal, Write, stderr, stdin};

use crate::tool::Tool;

/// How to ask questions to the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AskVia {
    /// Straight to this process' terminal.
    #[default]
    Tty,
    /// Read from the process' stdin.
    Stdin,
    /// Through systemd, which routes the question to whichever agent can put
    /// it in front of the operator.
    SystemdAskPassword,
}

impl AskVia {
    fn is_subject_to_typos(self) -> bool {
        !matches!(self, AskVia::Stdin)
    }
}

impl std::str::FromStr for AskVia {
    type Err = String;

    fn from_str(name: &str) -> Result<AskVia, String> {
        match name {
            "tty" => Ok(AskVia::Tty),
            "stdin" => Ok(AskVia::Stdin),
            "systemd-ask-password" => Ok(AskVia::SystemdAskPassword),
            other => Err(format!(
                "unknown way to ask '{other}', expected 'tty', 'stdin' or \
                 'systemd-ask-password'"
            )),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not be read: {0}")]
    Unheard(std::io::Error),
    #[error("the answers do not match")]
    Mismatch,
    #[error("stdin ran out of answers")]
    StdinRanOut,
    #[error("programming error: the rehearsal did not see this question")]
    Unrehearsed,
    #[error("stdin is a terminal")]
    StdinIsATerminal,
    #[error(transparent)]
    Tool(#[from] crate::tool::Error),
}

// Prompts questions to the operator.
//
// During rehersal, if asking via stdin, the stdin responses are read and
// evaluated.
pub struct Asker {
    via: AskVia,
    /// What the rehearsal read off stdin, kept for the run that follows it.
    read_from_stdin: VecDeque<String>,
}

impl Asker {
    pub fn new(via: AskVia) -> Asker {
        Asker {
            via,
            read_from_stdin: VecDeque::new(),
        }
    }

    /// The answer the next question would get, without putting it to anyone.
    ///
    /// Piped answers are read here so we can detect if the piped data ran out.
    pub fn rehearse(&mut self) -> Result<String, Error> {
        reachable(self.via)?;
        match self.via {
            AskVia::Stdin => {
                let answer = read_answer_stdin()?;
                self.read_from_stdin.push_back(answer.clone());
                Ok(answer)
            }
            AskVia::Tty | AskVia::SystemdAskPassword => {
                /// Placeholder value in the operator's stead.
                const REHEARSAL: &str = "orma rehearsal";
                Ok(REHEARSAL.to_owned())
            }
        }
    }

    /// Prompt `prompt` to the operator, guarding the response against typos.
    pub fn ask_guard_against_typos(&mut self, prompt: &str) -> Result<String, Error> {
        let answer = self.prompt(prompt)?;
        if self.via.is_subject_to_typos() && answer != self.prompt("Confirm: ")? {
            return Err(Error::Mismatch);
        }
        Ok(answer)
    }

    /// Tell the operator something, without expecting an answer.
    pub fn say(&self, something: &str) {
        let mut err = stderr();
        writeln!(err, "{}", something.trim_end()).ok();
        err.flush().ok();
    }

    fn prompt(&mut self, prompt: &str) -> Result<String, Error> {
        match self.via {
            AskVia::Tty => ask_tty(prompt),
            AskVia::Stdin => {
                announce_question(prompt);
                // Reading another line here would answer a question the
                // rehearsal never saw. This happens if the two diverge.
                self.read_from_stdin.pop_front().ok_or(Error::Unrehearsed)
            }
            AskVia::SystemdAskPassword => ask_through_systemd(prompt),
        }
    }
}

/// Take the next answer off stdin. Each question consumes one line, so
/// several questions are answered by feeding several lines.
fn read_answer_stdin() -> Result<String, Error> {
    let mut line = String::new();
    stdin()
        .lock()
        .read_line(&mut line)
        .map_err(Error::Unheard)?;
    let answer = line.trim_end_matches(['\n', '\r']);
    match answer.is_empty() {
        true => Err(Error::StdinRanOut),
        false => Ok(answer.to_owned()),
    }
}

fn announce_question(prompt: &str) {
    let mut err = stderr();
    writeln!(err, "{}", prompt.trim_end()).ok();
    err.flush().ok();
}

/// Whether the specified via is likely to be usable.
fn reachable(via: AskVia) -> Result<(), Error> {
    match via {
        // The prompt goes to this process's own stdio.
        AskVia::Tty => Ok(()),
        // Prompting via stdin requires no confirmation, prevent the user from
        // unintentionally incorrectly feeding the program. This a safeguard
        // and can be easily defeated with `cat | orma ...`
        AskVia::Stdin => match stdin().is_terminal() {
            true => Err(Error::StdinIsATerminal),
            false => Ok(()),
        },
        AskVia::SystemdAskPassword => {
            let agent = Tool::SYSTEMD_ASK_PASSWORD;
            let answered = agent
                .command()
                .arg("--help")
                .output()
                .map_err(|err| agent.failed(format!("could not be started: {err}")))?;
            match answered.status.success() {
                true => Ok(()),
                false => Err(agent.failed("is present but does not run").into()),
            }
        }
    }
}

/// Print the prompt to stderr to keep it out of anything redirecting stdout.
fn ask_tty(prompt: &str) -> Result<String, Error> {
    let mut err = stderr();
    write!(err, "{prompt}").ok();
    err.flush().ok();
    if stdin().is_terminal() {
        return rpassword::read_password().map_err(Error::Unheard);
    }
    let mut line = String::new();
    stdin()
        .lock()
        .read_line(&mut line)
        .map_err(Error::Unheard)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_owned())
}

/// Hand the question to systemd and await the response.
fn ask_through_systemd(prompt: &str) -> Result<String, Error> {
    let agent = Tool::SYSTEMD_ASK_PASSWORD;
    let asked = agent
        .command()
        .arg(prompt)
        .output()
        .map_err(|err| agent.failed(format!("could not be started: {err}")))?;
    if !asked.status.success() {
        let complaint = String::from_utf8_lossy(&asked.stderr);
        return Err(agent.failed(complaint.trim()).into());
    }
    let answer = String::from_utf8(asked.stdout)
        .map_err(|_| agent.failed("answered with something that is not text"))?;
    Ok(answer.trim_end_matches(['\n', '\r']).to_owned())
}
