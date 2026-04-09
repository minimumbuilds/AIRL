use std::process::{Child, Command, Stdio};

use crate::transport::{Transport, TransportError, write_frame, read_frame};

/// Transport that communicates with a child process via stdin/stdout
/// using the length-prefixed framing protocol.
pub struct StdioTransport {
    child: Child,
    closed: bool,
}

impl StdioTransport {
    /// Spawn a child process and attach to its stdin/stdout.
    pub fn spawn(command: &str, args: &[&str]) -> Result<Self, TransportError> {
        let child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(TransportError::Io)?;
        Ok(Self { child, closed: false })
    }
}

impl Transport for StdioTransport {
    fn send_message(&mut self, payload: &str) -> Result<(), TransportError> {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or(TransportError::Disconnected)?;
        write_frame(stdin, payload).map_err(TransportError::Io)
    }

    fn recv_message(&mut self) -> Result<String, TransportError> {
        let stdout = self
            .child
            .stdout
            .as_mut()
            .ok_or(TransportError::Disconnected)?;
        read_frame(stdout).map_err(TransportError::Io)
    }

    fn close(&mut self) -> Result<(), TransportError> {
        // Drop stdin to signal EOF, then wait for the child to exit.
        drop(self.child.stdin.take());
        self.child.wait().map_err(TransportError::Io)?;
        self.closed = true;
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_cat_round_trip() {
        // `cat` echoes stdin to stdout byte-for-byte, which makes it
        // a perfect test target for framing round-trips.
        let mut transport = StdioTransport::spawn("cat", &[]).unwrap();

        transport.send_message("hello from stdio").unwrap();
        let reply = transport.recv_message().unwrap();
        assert_eq!(reply, "hello from stdio");

        transport.close().unwrap();
    }

    #[test]
    fn stdio_multiple_messages() {
        let mut transport = StdioTransport::spawn("cat", &[]).unwrap();

        for i in 0..5 {
            let msg = format!("message #{}", i);
            transport.send_message(&msg).unwrap();
            let reply = transport.recv_message().unwrap();
            assert_eq!(reply, msg);
        }

        transport.close().unwrap();
    }
}
