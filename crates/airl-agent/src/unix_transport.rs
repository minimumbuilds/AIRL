use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::transport::{Transport, TransportError, write_frame, read_frame};

/// Transport over a Unix domain socket using length-prefixed framing.
pub struct UnixTransport {
    stream: UnixStream,
}

impl UnixTransport {
    /// Connect to a Unix socket at the given path.
    pub fn connect<P: AsRef<Path>>(path: P) -> Result<Self, TransportError> {
        let stream = UnixStream::connect(path).map_err(TransportError::Io)?;
        Ok(Self { stream })
    }

    /// Wrap an already-established Unix stream (e.g. from a listener).
    pub fn from_stream(stream: UnixStream) -> Self {
        Self { stream }
    }
}

impl Transport for UnixTransport {
    fn send_message(&mut self, payload: &str) -> Result<(), TransportError> {
        write_frame(&mut self.stream, payload).map_err(TransportError::Io)
    }

    fn recv_message(&mut self) -> Result<String, TransportError> {
        read_frame(&mut self.stream).map_err(TransportError::Io)
    }

    fn close(&mut self) -> Result<(), TransportError> {
        self.stream
            .shutdown(std::net::Shutdown::Both)
            .map_err(TransportError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn unix_round_trip() {
        let dir = std::env::temp_dir().join(format!("airl-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock_path = dir.join("agent.sock");

        // Clean up any leftover socket
        let _ = std::fs::remove_file(&sock_path);

        let listener = UnixListener::bind(&sock_path).unwrap();
        let path_clone = sock_path.clone();

        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = UnixTransport::from_stream(stream);
            let msg = server.recv_message().unwrap();
            server.send_message(&format!("echo: {}", msg)).unwrap();
        });

        let mut client = UnixTransport::connect(&sock_path).unwrap();
        client.send_message("hello unix").unwrap();
        let reply = client.recv_message().unwrap();
        assert_eq!(reply, "echo: hello unix");

        client.close().unwrap();
        handle.join().unwrap();

        // Cleanup
        let _ = std::fs::remove_file(&path_clone);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn unix_multiple_messages() {
        let dir = std::env::temp_dir().join(format!("airl-test-multi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sock_path = dir.join("agent.sock");
        let _ = std::fs::remove_file(&sock_path);

        let listener = UnixListener::bind(&sock_path).unwrap();
        let path_clone = sock_path.clone();

        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = UnixTransport::from_stream(stream);
            for _ in 0..3 {
                let msg = server.recv_message().unwrap();
                server.send_message(&msg).unwrap();
            }
        });

        let mut client = UnixTransport::connect(&sock_path).unwrap();
        for i in 0..3 {
            let msg = format!("msg-{}", i);
            client.send_message(&msg).unwrap();
            let reply = client.recv_message().unwrap();
            assert_eq!(reply, msg);
        }

        client.close().unwrap();
        handle.join().unwrap();

        let _ = std::fs::remove_file(&path_clone);
        let _ = std::fs::remove_dir(&dir);
    }
}
