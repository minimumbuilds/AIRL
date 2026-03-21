use std::net::{TcpStream, SocketAddr, Shutdown};

use crate::transport::{Transport, TransportError, write_frame, read_frame};

/// Transport over a TCP connection using length-prefixed framing.
pub struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    /// Connect to a remote agent at the given address.
    pub fn connect(addr: SocketAddr) -> Result<Self, TransportError> {
        let stream = TcpStream::connect(addr).map_err(TransportError::Io)?;
        Ok(Self { stream })
    }

    /// Wrap an already-established TCP stream (e.g. from a listener).
    pub fn from_stream(stream: TcpStream) -> Self {
        Self { stream }
    }
}

impl Transport for TcpTransport {
    fn send_message(&mut self, payload: &str) -> Result<(), TransportError> {
        write_frame(&mut self.stream, payload).map_err(TransportError::Io)
    }

    fn recv_message(&mut self) -> Result<String, TransportError> {
        read_frame(&mut self.stream).map_err(TransportError::Io)
    }

    fn close(&mut self) -> Result<(), TransportError> {
        self.stream.shutdown(Shutdown::Both).map_err(TransportError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn tcp_round_trip() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = TcpTransport::from_stream(stream);
            let msg = server.recv_message().unwrap();
            server.send_message(&format!("echo: {}", msg)).unwrap();
        });

        let mut client = TcpTransport::connect(addr).unwrap();
        client.send_message("hello").unwrap();
        let reply = client.recv_message().unwrap();
        assert_eq!(reply, "echo: hello");

        client.close().unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn tcp_multiple_messages() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = TcpTransport::from_stream(stream);
            for _ in 0..3 {
                let msg = server.recv_message().unwrap();
                server.send_message(&msg).unwrap();
            }
        });

        let mut client = TcpTransport::connect(addr).unwrap();
        for i in 0..3 {
            let msg = format!("msg-{}", i);
            client.send_message(&msg).unwrap();
            let reply = client.recv_message().unwrap();
            assert_eq!(reply, msg);
        }

        client.close().unwrap();
        handle.join().unwrap();
    }
}
