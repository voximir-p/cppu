use std::fmt;
use std::io::{self, IsTerminal, Write};

#[derive(Clone, Copy)]
pub(super) enum Status {
    Amalgamating,
    Compiling,
    Running,
    Removed,
    Finished,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Self::Amalgamating => "Amalgamating",
            Self::Compiling => "Compiling",
            Self::Running => "Running",
            Self::Removed => "Removed",
            Self::Finished => "Finished",
        }
    }
}

/// Cargo-style output: aligned green statuses and unindented diagnostics.
pub(super) struct Ui {
    quiet: bool,
    color: bool,
}

impl Ui {
    pub(super) fn is_quiet(&self) -> bool {
        self.quiet
    }

    pub(super) fn new(quiet: bool) -> Self {
        let no_color = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
        let dumb_terminal = std::env::var_os("TERM").is_some_and(|term| term == "dumb");
        Self {
            quiet,
            color: io::stderr().is_terminal() && !no_color && !dumb_terminal,
        }
    }

    pub(super) fn status(&self, status: Status, message: impl fmt::Display) {
        if !self.quiet {
            self.line(&format!("{:>12}", status.label()), 92, " ", message);
        }
    }

    pub(super) fn warning(&self, message: impl fmt::Display) {
        self.line("warning", 93, ": ", message);
    }

    pub(super) fn error(&self, message: impl fmt::Display) {
        self.line("error", 91, ": ", message);
    }

    fn line(&self, label: &str, color: u8, separator: &str, message: impl fmt::Display) {
        let mut stderr = io::stderr().lock();
        if self.color {
            let _ = writeln!(stderr, "\x1b[1;{color}m{label}\x1b[0m{separator}{message}");
        } else {
            let _ = writeln!(stderr, "{label}{separator}{message}");
        }
    }

    pub(super) fn compiler_output(&self, diagnostics: &[u8]) {
        if !diagnostics.is_empty() {
            let mut stderr = io::stderr().lock();
            let _ = stderr.write_all(diagnostics);
            if diagnostics.last() != Some(&b'\n') {
                let _ = writeln!(stderr);
            }
        }
    }
}
