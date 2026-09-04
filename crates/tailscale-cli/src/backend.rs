//! What the rest of the server sees of the local node.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::exec::ExecError;

/// A boxed future, spelled out rather than pulled from `futures`, because this
/// crate needs exactly one of them.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Whether a call may overlap with others.
///
/// The local node serialises its own mutations anyway, but two `tailscale set`
/// calls racing produce a result neither caller asked for, and the failure is
/// invisible. Mutating calls therefore queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Concurrency {
    /// Runs alongside any number of other reads.
    Shared,
    /// Runs alone.
    Exclusive,
}

/// One invocation of the CLI.
///
/// The argument list is a list. It is never joined into a string and never
/// handed to a shell, which is what makes an argument containing a space, a
/// quote or a semicolon uninteresting.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub args: Vec<String>,
    pub concurrency: Concurrency,
    pub timeout: Duration,
    /// Bytes to write to the child's standard input, for the few commands that
    /// read a document rather than a flag.
    pub stdin: Option<Vec<u8>>,
}

impl Invocation {
    /// A read: overlaps freely, default timeout.
    pub fn read<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            args: args.into_iter().map(Into::into).collect(),
            concurrency: Concurrency::Shared,
            timeout: crate::exec::DEFAULT_TIMEOUT,
            stdin: None,
        }
    }

    /// A mutation: queues behind other mutations.
    pub fn mutate<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            concurrency: Concurrency::Exclusive,
            ..Self::read(args)
        }
    }

    /// A mutation that cannot race the node's configuration: it changes the
    /// filesystem, or a peer, but never what `tailscale set` and `tailscale up`
    /// contend over.
    ///
    /// It runs in the shared lane because the lock exists to keep two
    /// configuration writes apart, and these are not that. Queueing them would
    /// buy no safety and cost a great deal: a ten-minute file transfer holding
    /// the exclusive lock stalls every concurrent read for its whole duration.
    pub fn mutate_shared<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::read(args)
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_stdin(mut self, stdin: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(stdin.into());
        self
    }

    /// The command as it would be written, for logs and error messages. Never
    /// used to run anything.
    pub fn display(&self) -> String {
        let mut out = String::from("tailscale");
        for arg in &self.args {
            out.push(' ');
            out.push_str(arg);
        }
        out
    }
}

/// What came back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// `None` when the child was terminated by a signal.
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl Output {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    pub fn stdout_str(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.stdout)
    }
}

/// The local node, as the tools see it.
///
/// One method, so that a fake is a dozen lines. `dyn`-compatible by hand-rolling
/// the boxed future rather than taking an `async fn` in the trait.
pub trait LocalBackend: Send + Sync + std::fmt::Debug {
    fn run<'a>(&'a self, invocation: Invocation) -> BoxFuture<'a, Result<Output, ExecError>>;
}

impl<T: LocalBackend + ?Sized> LocalBackend for std::sync::Arc<T> {
    fn run<'a>(&'a self, invocation: Invocation) -> BoxFuture<'a, Result<Output, ExecError>> {
        (**self).run(invocation)
    }
}

/// A backend that has no binary to run.
///
/// Used when the local surface is switched off, or when discovery found
/// nothing. Every call fails the same way, which keeps the "surface absent"
/// path identical whether the operator disabled it or the machine lacks the
/// binary — and means no caller has to special-case an absent backend.
#[derive(Debug, Clone)]
pub struct Unavailable {
    reason: String,
}

impl Unavailable {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

impl Default for Unavailable {
    fn default() -> Self {
        Self::new("the local surface is not available")
    }
}

impl LocalBackend for Unavailable {
    fn run<'a>(&'a self, _invocation: Invocation) -> BoxFuture<'a, Result<Output, ExecError>> {
        Box::pin(async move {
            Err(ExecError::BinaryNotFound {
                searched: vec![self.reason.clone()],
            })
        })
    }
}
