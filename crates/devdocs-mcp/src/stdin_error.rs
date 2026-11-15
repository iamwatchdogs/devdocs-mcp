//! The guarded stdin transport: it tells a broken stdin pipe from a
//! clean client close. rmcp folds EOF and io errors into one
//! `ConnectionClosed` — `AsyncRwTransport::receive` returns `None` for
//! both (`LineRead::Eof` and `LineRead::Io(e)`, which only logs) — so
//! the error travels out-of-band through the slot for the `serve` caller
//! (docs/implementation/08 §MCP adapter).

use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use tokio::io::ReadBuf;

/// Where a stdin read error waits for the `serve` caller. `None` means
/// the transport closed without an error: a clean client close.
pub(crate) type StdinErrorSlot = Arc<Mutex<Option<io::Error>>>;

/// The stdin side of the stdio transport: it delegates every read to
/// the wrapped reader and captures the first read error in the slot.
/// The generic reader keeps the error path testable with stand-in
/// readers; the serve path wraps the [`rmcp::transport::StdioReader`].
pub(crate) struct GuardedStdin<R> {
    inner: R,
    slot: StdinErrorSlot,
}

impl<R> GuardedStdin<R> {
    pub(crate) fn new(inner: R, slot: StdinErrorSlot) -> Self {
        Self { inner, slot }
    }
}

impl<R: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for GuardedStdin<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Err(e)) => {
                // First error wins: the slot holds a copy of the kind
                // and the message (`io::Error` carries no `Clone`, so
                // the source chain drops), and the poll returns the
                // original.
                if let Ok(mut guard) = this.slot.lock()
                    && guard.is_none()
                {
                    *guard = Some(io::Error::new(e.kind(), e.to_string()));
                }
                Poll::Ready(Err(e))
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::task::{Context, Poll, Waker};

    // The trait must be in scope for the manual `poll_read` calls; the
    // workspace tokio has no io-util, so no ext trait applies here.
    use tokio::io::AsyncRead as _;

    use super::*;

    /// A reader whose poll always fails: the broken stdin pipe stand-in.
    struct FailingReader;

    impl tokio::io::AsyncRead for FailingReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::other("test read failure")))
        }
    }

    /// A reader whose poll returns a clean end of stream: the closed
    /// stdin stand-in.
    struct EofReader;

    impl tokio::io::AsyncRead for EofReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    /// A reader whose poll fails with a poll-counted message: the
    /// stand-in that tells the first from the second error.
    struct CountingFailures(u32);

    impl tokio::io::AsyncRead for CountingFailures {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let this = self.get_mut();
            this.0 += 1;
            Poll::Ready(Err(io::Error::other(format!("failure {}", this.0))))
        }
    }

    #[test]
    fn a_read_error_lands_in_the_slot() {
        let slot: StdinErrorSlot = Arc::new(Mutex::new(None));
        let mut guarded = GuardedStdin {
            inner: FailingReader,
            slot: Arc::clone(&slot),
        };
        // Manual polling: `Waker::noop` is stable since 1.85, inside the
        // 1.90 MSRV.
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut buf = [0_u8; 8];
        let mut read_buf = ReadBuf::new(&mut buf);
        let outcome = Pin::new(&mut guarded).poll_read(&mut cx, &mut read_buf);
        assert!(
            matches!(outcome, Poll::Ready(Err(_))),
            "the poll passes the error through: {outcome:?}"
        );
        let captured = slot.lock().ok().and_then(|mut guard| guard.take());
        assert!(captured.is_some(), "the slot holds the read error");
    }

    #[test]
    fn a_clean_eof_leaves_the_slot_empty() {
        let slot: StdinErrorSlot = Arc::new(Mutex::new(None));
        let mut guarded = GuardedStdin {
            inner: EofReader,
            slot: Arc::clone(&slot),
        };
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut buf = [0_u8; 8];
        let mut read_buf = ReadBuf::new(&mut buf);
        let outcome = Pin::new(&mut guarded).poll_read(&mut cx, &mut read_buf);
        assert!(
            matches!(outcome, Poll::Ready(Ok(()))),
            "the poll passes EOF through: {outcome:?}"
        );
        let captured = slot.lock().ok().and_then(|mut guard| guard.take());
        assert!(captured.is_none(), "EOF is not an error");
    }

    #[test]
    fn a_second_read_error_does_not_replace_the_first() {
        let slot: StdinErrorSlot = Arc::new(Mutex::new(None));
        let mut guarded = GuardedStdin {
            inner: CountingFailures(0),
            slot: Arc::clone(&slot),
        };
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut buf = [0_u8; 8];
        let mut read_buf = ReadBuf::new(&mut buf);
        for _ in 0..2 {
            let outcome = Pin::new(&mut guarded).poll_read(&mut cx, &mut read_buf);
            assert!(
                matches!(outcome, Poll::Ready(Err(_))),
                "the poll passes the error through: {outcome:?}"
            );
        }
        let captured = slot.lock().ok().and_then(|mut guard| guard.take());
        let message = captured.map(|error| error.to_string()).unwrap_or_default();
        assert_eq!(
            message, "failure 1",
            "the slot holds the first error: the second does not replace it"
        );
    }
}
