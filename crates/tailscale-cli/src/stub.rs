//! A scriptable backend, for tests.
//!
//! Behind the `testing` feature so it does not reach a release build. It exists
//! because nearly every interesting behaviour of this server is "what does it
//! do when the CLI says *that*", and answering that against a real binary would
//! mean a real tailnet.

use std::sync::Mutex;
use std::time::Duration;

use crate::backend::{BoxFuture, Invocation, LocalBackend, Output};
use crate::exec::ExecError;

/// What the stub does when a call matches.
#[derive(Debug, Clone)]
pub enum Reply {
    /// The command ran and produced this.
    Ran(Output),
    /// There was no binary to run.
    Unavailable,
    /// The command did not finish in time, having printed this much first.
    TimedOut { printed: String },
}

impl Reply {
    /// A clean exit with this on standard output.
    pub fn ok(stdout: impl Into<String>) -> Self {
        Self::Ran(Output {
            exit_code: Some(0),
            stdout: stdout.into().into_bytes(),
            stderr: String::new(),
        })
    }

    /// A command that hung after saying nothing.
    pub fn timed_out() -> Self {
        Self::TimedOut {
            printed: String::new(),
        }
    }

    /// A command that said this and then hung.
    pub fn hung_after(printed: impl Into<String>) -> Self {
        Self::TimedOut {
            printed: printed.into(),
        }
    }

    /// A non-zero exit with this on standard error.
    pub fn failed(exit_code: i32, stderr: impl Into<String>) -> Self {
        Self::Ran(Output {
            exit_code: Some(exit_code),
            stdout: Vec::new(),
            stderr: stderr.into(),
        })
    }
}

/// A backend that answers from a script instead of a process.
#[derive(Debug)]
pub struct StubBackend {
    rules: Vec<(Vec<String>, Reply)>,
    fallback: Reply,
    calls: Mutex<Vec<Invocation>>,
}

impl StubBackend {
    /// Answer every call the same way.
    pub fn always(reply: Reply) -> Self {
        Self {
            rules: Vec::new(),
            fallback: reply,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Every call succeeds with this on standard output.
    pub fn ok(stdout: impl Into<String>) -> Self {
        Self::always(Reply::ok(stdout))
    }

    /// Every call fails this way.
    pub fn failure(exit_code: i32, stderr: impl Into<String>) -> Self {
        Self::always(Reply::failed(exit_code, stderr))
    }

    /// There is no binary at all.
    pub fn missing() -> Self {
        Self::always(Reply::Unavailable)
    }

    /// Answer calls whose arguments start with `prefix` this way.
    ///
    /// Rules are tried in the order they were added, so a more specific rule
    /// must be added before the general one it refines.
    #[must_use]
    pub fn on<I, S>(mut self, prefix: I, reply: Reply) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rules
            .push((prefix.into_iter().map(Into::into).collect(), reply));
        self
    }

    /// Everything that has been run, in order.
    pub fn calls(&self) -> Vec<Invocation> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// The arguments of everything that has been run, in order.
    pub fn argv(&self) -> Vec<Vec<String>> {
        self.calls().into_iter().map(|c| c.args).collect()
    }

    fn reply_for(&self, args: &[String]) -> Reply {
        self.rules
            .iter()
            .find(|(prefix, _)| args.starts_with(prefix))
            .map_or_else(|| self.fallback.clone(), |(_, reply)| reply.clone())
    }
}

impl LocalBackend for StubBackend {
    fn run<'a>(&'a self, invocation: Invocation) -> BoxFuture<'a, Result<Output, ExecError>> {
        let reply = self.reply_for(&invocation.args);
        let display = invocation.display();
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(invocation);
        }
        Box::pin(async move {
            match reply {
                Reply::Ran(output) => Ok(output),
                Reply::Unavailable => Err(ExecError::BinaryNotFound {
                    searched: vec!["a stub backend with no binary".to_owned()],
                }),
                Reply::TimedOut { printed } => Err(ExecError::Timeout {
                    command: display,
                    timeout: Duration::from_secs(30),
                    printed,
                }),
            }
        })
    }
}
