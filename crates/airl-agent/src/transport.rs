use std::fmt;
use std::io::{self, Read, Write};

/// Errors that can occur during transport operations.
#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    Disconnected,
    InvalidFrame(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "transport I/O error: {}", e),
            TransportError::Disconnected => write!(f, "transport disconnected"),
            TransportError::InvalidFrame(msg) => write!(f, "invalid frame: {}", msg),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<io::Error> for TransportError {
    fn from(e: io::Error) -> Self {
        TransportError::Io(e)
    }
}

/// Trait for bidirectional message transport between agents.
pub trait Transport: Send {
    fn send_message(&mut self, payload: &str) -> Result<(), TransportError>;
    fn recv_message(&mut self) -> Result<String, TransportError>;
    fn close(&mut self) -> Result<(), TransportError>;
}

/// Write a length-prefixed frame: [u32 BE length][UTF-8 payload].
pub fn write_frame(writer: &mut dyn Write, payload: &str) -> io::Result<()> {
    let bytes = payload.as_bytes();
    let len = bytes.len() as u32;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}

/// Read a length-prefixed frame and return the UTF-8 payload.
pub fn read_frame(reader: &mut dyn Read) -> io::Result<String> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_round_trip() {
        let msg = "hello, agent!";
        let mut buf = Vec::new();
        write_frame(&mut buf, msg).unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor).unwrap();
        assert_eq!(result, msg);
    }

    #[test]
    fn frame_empty_payload() {
        let mut buf = Vec::new();
        write_frame(&mut buf, "").unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn frame_multiple_messages() {
        let msgs = ["first", "second", "third"];
        let mut buf = Vec::new();
        for m in &msgs {
            write_frame(&mut buf, m).unwrap();
        }

        let mut cursor = Cursor::new(buf);
        for m in &msgs {
            let result = read_frame(&mut cursor).unwrap();
            assert_eq!(&result, m);
        }
    }

    #[test]
    fn frame_unicode() {
        let msg = "こんにちは世界 🌍";
        let mut buf = Vec::new();
        write_frame(&mut buf, msg).unwrap();

        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor).unwrap();
        assert_eq!(result, msg);
    }

    #[test]
    fn read_frame_truncated() {
        // Only write the length header, no payload
        let mut buf = Vec::new();
        buf.extend_from_slice(&10u32.to_be_bytes());
        let mut cursor = Cursor::new(buf);
        assert!(read_frame(&mut cursor).is_err());
    }
}
