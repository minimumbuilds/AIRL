use std::io::{self, Read, Write};
use crate::value::Value;

/// Write a length-prefixed frame: [u32 BE length][UTF-8 payload].
pub fn write_frame(writer: &mut dyn Write, payload: &str) -> io::Result<()> {
    let bytes = payload.as_bytes();
    if bytes.len() > u32::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("frame payload too large: {} bytes exceeds u32::MAX", bytes.len()),
        ));
    }
    let len = bytes.len() as u32;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(bytes)?;
    writer.flush()
}

const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

/// Read a length-prefixed frame.
pub fn read_frame(reader: &mut dyn Read) -> io::Result<String> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame size {} exceeds {} byte limit", len, MAX_FRAME_SIZE),
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Format a task message as an AIRL S-expression.
pub fn format_task(id: &str, fn_name: &str, args: &[Value]) -> String {
    let args_str: Vec<String> = args.iter().map(|v| format!("{}", v)).collect();
    format!(
        r#"(task "{}" :from "self" :call "{}" :args [{}])"#,
        id, fn_name, args_str.join(" ")
    )
}

/// Parse a result message. Returns Ok(value) on success, Err(message) on failure.
pub fn parse_result_message(response: &str) -> Result<Value, String> {
    // Parse as S-expression
    let mut lexer = airl_syntax::Lexer::new(response);
    let tokens = lexer.lex_all().map_err(|d| d.message)?;
    let sexprs = airl_syntax::parse_sexpr_all(&tokens).map_err(|d| d.message)?;

    if sexprs.is_empty() {
        return Err("empty response".into());
    }

    let items = match &sexprs[0] {
        airl_syntax::sexpr::SExpr::List(items, _) => items,
        _ => return Err("expected list".into()),
    };

    // Walk items looking for :status and :payload/:message
    let mut status_complete = false;
    let mut payload: Option<Value> = None;
    let mut error_msg: Option<String> = None;

    let mut i = 0;
    while i < items.len() {
        if let Some(kw) = items[i].as_keyword() {
            match kw {
                "status" => {
                    if i + 1 < items.len() {
                        if let Some(s) = items[i + 1].as_keyword() {
                            status_complete = s == "complete";
                        }
                        i += 1;
                    }
                }
                "payload" => {
                    if i + 1 < items.len() {
                        payload = sexpr_to_value(&items[i + 1]).ok();
                        i += 1;
                    }
                }
                "message" => {
                    if i + 1 < items.len() {
                        if let airl_syntax::sexpr::SExpr::Atom(a) = &items[i + 1] {
                            if let airl_syntax::sexpr::AtomKind::Str(s) = &a.kind {
                                error_msg = Some(s.clone());
                            }
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }

    if status_complete {
        Ok(payload.unwrap_or(Value::Unit))
    } else {
        Err(error_msg.unwrap_or_else(|| "unknown error".into()))
    }
}

/// Convert an S-expression to a Value (for parsing result payloads).
fn sexpr_to_value(sexpr: &airl_syntax::sexpr::SExpr) -> Result<Value, String> {
    use airl_syntax::sexpr::{SExpr, AtomKind};
    match sexpr {
        SExpr::Atom(a) => match &a.kind {
            AtomKind::Integer(v) => Ok(Value::Int(*v)),
            AtomKind::Float(v) => Ok(Value::Float(*v)),
            AtomKind::Str(v) => Ok(Value::Str(v.clone())),
            AtomKind::Bool(v) => Ok(Value::Bool(*v)),
            AtomKind::Nil => Ok(Value::Nil),
            AtomKind::Symbol(s) => Ok(Value::Str(s.clone())),
            AtomKind::Keyword(k) => Ok(Value::Str(format!(":{}", k))),
            AtomKind::Arrow => Ok(Value::Str("->".into())),
            AtomKind::Version(major, minor, patch) => Ok(Value::Str(format!("{}.{}.{}", major, minor, patch))),
        }
        SExpr::List(items, _) => {
            // Check for variant: (Ok 42)
            if let Some(SExpr::Atom(a)) = items.first() {
                if let AtomKind::Symbol(name) = &a.kind {
                    if name.chars().next().map_or(false, |c| c.is_uppercase()) && items.len() == 2 {
                        let inner = sexpr_to_value(&items[1])?;
                        return Ok(Value::Variant(name.clone(), Box::new(inner)));
                    }
                }
            }
            let vals: Result<Vec<_>, _> = items.iter().map(sexpr_to_value).collect();
            Ok(Value::List(vals?))
        }
        SExpr::BracketList(items, _) => {
            let vals: Result<Vec<_>, _> = items.iter().map(sexpr_to_value).collect();
            Ok(Value::List(vals?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frame_round_trip() {
        let msg = "hello";
        let mut buf = Vec::new();
        write_frame(&mut buf, msg).unwrap();
        let mut cursor = Cursor::new(buf);
        assert_eq!(read_frame(&mut cursor).unwrap(), "hello");
    }

    #[test]
    fn format_task_message() {
        let msg = format_task("t-1", "add", &[Value::Int(3), Value::Int(4)]);
        assert!(msg.contains("task"));
        assert!(msg.contains("t-1"));
        assert!(msg.contains(":call"));
        assert!(msg.contains("add"));
        assert!(msg.contains("3"));
        assert!(msg.contains("4"));
    }

    #[test]
    fn parse_success_result() {
        let response = r#"(result "t-1" :status :complete :payload 42)"#;
        let val = parse_result_message(response).unwrap();
        assert_eq!(val, Value::Int(42));
    }

    #[test]
    fn parse_error_result() {
        let response = r#"(result "t-1" :status :error :message "not found")"#;
        let err = parse_result_message(response).unwrap_err();
        assert!(err.contains("not found"));
    }
}
