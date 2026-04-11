use airl_runtime::value::Value;
use airl_syntax::sexpr::{SExpr, AtomKind};
use airl_syntax::{Lexer, parse_sexpr_all};

/// A task request sent from client to worker.
#[derive(Debug, Clone)]
pub struct TaskMessage {
    pub id: String,
    pub from: String,
    pub call: String,
    pub args: Vec<Value>,
}

/// A result response sent from worker to client.
#[derive(Debug, Clone)]
pub struct ResultMessage {
    pub id: String,
    pub success: bool,
    pub payload: Option<Value>,
    pub error: Option<String>,
}

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Serialize a Value to a safe wire representation.
/// For strings, properly escapes interior quotes/backslashes.
/// For all other types, uses the Display representation directly.
fn value_to_wire(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("\"{}\"", escape_str(s)),
        other => format!("{}", other),
    }
}

/// Serialize a task message to an AIRL S-expression string.
pub fn serialize_task(msg: &TaskMessage) -> String {
    let args_str: Vec<String> = msg.args.iter().map(value_to_wire).collect();
    format!(
        r#"(task "{}" :from "{}" :call "{}" :args [{}])"#,
        escape_str(&msg.id), escape_str(&msg.from), escape_str(&msg.call), args_str.join(" ")
    )
}

/// Serialize a result message to an AIRL S-expression string.
pub fn serialize_result(msg: &ResultMessage) -> String {
    if msg.success {
        let payload_str = msg.payload.as_ref()
            .map(value_to_wire)
            .unwrap_or_else(|| "nil".into());
        format!(
            r#"(result "{}" :status :complete :payload {})"#,
            escape_str(&msg.id), payload_str
        )
    } else {
        let err_str = msg.error.as_deref().unwrap_or("unknown error");
        format!(
            r#"(result "{}" :status :error :message "{}")"#,
            escape_str(&msg.id), escape_str(err_str)
        )
    }
}

/// Parse a task message from an AIRL S-expression string.
pub fn parse_task(input: &str) -> Result<TaskMessage, String> {
    let sexprs = lex_and_parse(input)?;
    let list = match sexprs.get(0) {
        Some(SExpr::List(items, _)) => items,
        Some(_) => return Err("expected list".into()),
        None => return Err("empty input".into()),
    };

    // First element should be symbol "task"
    if list.first().and_then(|s| s.as_symbol()) != Some("task") {
        return Err("expected (task ...) form".into());
    }

    // Second element is the task ID (string)
    let id = match list.get(1) {
        Some(SExpr::Atom(a)) => match &a.kind {
            AtomKind::Str(s) => s.clone(),
            _ => return Err("expected string task ID".into()),
        },
        _ => return Err("expected task ID".into()),
    };

    // Extract keyword-value pairs
    let mut from = String::new();
    let mut call = String::new();
    let mut args = Vec::new();

    let mut i = 2;
    while i < list.len() {
        if let Some(kw) = list[i].as_keyword() {
            match kw {
                "from" => {
                    i += 1;
                    match list.get(i) {
                        Some(SExpr::Atom(a)) => match &a.kind {
                            AtomKind::Str(s) => from = s.clone(),
                            _ => return Err("expected string value after :from".into()),
                        },
                        Some(_) => return Err("expected string value after :from".into()),
                        None => return Err("missing value after :from".into()),
                    }
                }
                "call" => {
                    i += 1;
                    match list.get(i) {
                        Some(SExpr::Atom(a)) => match &a.kind {
                            AtomKind::Str(s) => call = s.clone(),
                            _ => return Err("expected string value after :call".into()),
                        },
                        Some(_) => return Err("expected string value after :call".into()),
                        None => return Err("missing value after :call".into()),
                    }
                }
                "args" => {
                    i += 1;
                    match list.get(i) {
                        Some(SExpr::BracketList(items, _)) => {
                            for item in items {
                                args.push(sexpr_to_value(item)?);
                            }
                        }
                        Some(_) => return Err("expected bracket list after :args".into()),
                        None => return Err("missing value after :args".into()),
                    }
                }
                _ => {} // skip unknown keywords
            }
        }
        i += 1;
    }

    if call.is_empty() {
        return Err("task missing required :call field".into());
    }
    if from.is_empty() {
        return Err("task missing required :from field".into());
    }

    Ok(TaskMessage { id, from, call, args })
}

/// Parse a result message from an AIRL S-expression string.
pub fn parse_result(input: &str) -> Result<ResultMessage, String> {
    let sexprs = lex_and_parse(input)?;
    let list = match sexprs.get(0) {
        Some(SExpr::List(items, _)) => items,
        Some(_) => return Err("expected list".into()),
        None => return Err("empty input".into()),
    };

    // First element should be symbol "result"
    if list.first().and_then(|s| s.as_symbol()) != Some("result") {
        return Err("expected (result ...) form".into());
    }

    // Second element is the result ID (string)
    let id = match list.get(1) {
        Some(SExpr::Atom(a)) => match &a.kind {
            AtomKind::Str(s) => s.clone(),
            _ => return Err("expected string result ID".into()),
        },
        _ => return Err("expected result ID".into()),
    };

    // Extract keyword-value pairs
    let mut success = false;
    let mut payload = None;
    let mut error = None;

    let mut i = 2;
    while i < list.len() {
        if let Some(kw) = list[i].as_keyword() {
            match kw {
                "status" => {
                    i += 1;
                    match list.get(i).and_then(|s| s.as_keyword()) {
                        Some(s) => success = s == "complete",
                        None => return Err("expected keyword value after :status".into()),
                    }
                }
                "payload" => {
                    i += 1;
                    match list.get(i) {
                        Some(expr) => payload = Some(sexpr_to_value(expr)?),
                        None => return Err("missing value after :payload".into()),
                    }
                }
                "message" => {
                    i += 1;
                    match list.get(i) {
                        Some(SExpr::Atom(a)) => match &a.kind {
                            AtomKind::Str(s) => error = Some(s.clone()),
                            _ => return Err("expected string value after :message".into()),
                        },
                        Some(_) => return Err("expected string value after :message".into()),
                        None => return Err("missing value after :message".into()),
                    }
                }
                _ => {} // skip unknown keywords
            }
        }
        i += 1;
    }

    Ok(ResultMessage { id, success, payload, error })
}

/// Convert an S-expression atom to a Value.
pub fn sexpr_to_value(sexpr: &SExpr) -> Result<Value, String> {
    match sexpr {
        SExpr::Atom(atom) => match &atom.kind {
            AtomKind::Integer(v) => Ok(Value::Int(*v)),
            AtomKind::Float(v) => Ok(Value::Float(*v)),
            AtomKind::Str(v) => Ok(Value::Str(v.clone())),
            AtomKind::Bool(v) => Ok(Value::Bool(*v)),
            AtomKind::Nil => Ok(Value::Nil),
            AtomKind::Symbol(s) => {
                // Capitalized symbols might be variant constructors
                Ok(Value::Str(s.clone())) // treat as string for now
            }
            AtomKind::Keyword(k) => Ok(Value::Str(format!(":{}", k))),
            AtomKind::Arrow => Ok(Value::Str("->".into())),
            AtomKind::Version(major, minor, patch) => Ok(Value::Str(format!("{}.{}.{}", major, minor, patch))),
        }
        SExpr::List(items, _) => {
            // Could be a variant: (Ok 42)
            if let Some(SExpr::Atom(a)) = items.first() {
                if let AtomKind::Symbol(name) = &a.kind {
                    if name.chars().next().map_or(false, |c| c.is_uppercase()) && items.len() == 2 {
                        let inner = sexpr_to_value(&items[1])?;
                        return Ok(Value::Variant(name.clone(), Box::new(inner)));
                    }
                }
            }
            // Otherwise treat as a list
            let vals: Result<Vec<_>, _> = items.iter().map(sexpr_to_value).collect();
            Ok(Value::List(vals?))
        }
        SExpr::BracketList(items, _) => {
            let vals: Result<Vec<_>, _> = items.iter().map(sexpr_to_value).collect();
            Ok(Value::List(vals?))
        }
    }
}

/// Convenience: parse a single value from a string.
pub fn sexpr_to_value_str(input: &str) -> Result<Value, String> {
    let sexprs = lex_and_parse(input)?;
    sexpr_to_value(&sexprs[0])
}

fn lex_and_parse(input: &str) -> Result<Vec<SExpr>, String> {
    let mut lexer = Lexer::new(input);
    let tokens = lexer.lex_all().map_err(|d| d.message)?;
    parse_sexpr_all(&tokens).map_err(|d| d.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_task_message() {
        let msg = TaskMessage {
            id: "t-001".into(),
            from: "cli".into(),
            call: "add".into(),
            args: vec![Value::Int(3), Value::Int(4)],
        };
        let s = serialize_task(&msg);
        assert!(s.contains("task"));
        assert!(s.contains("t-001"));
        assert!(s.contains(":call"));
        assert!(s.contains("add"));
    }

    #[test]
    fn parse_task_round_trip() {
        let msg = TaskMessage {
            id: "t-002".into(),
            from: "cli".into(),
            call: "multiply".into(),
            args: vec![Value::Int(6), Value::Int(7)],
        };
        let s = serialize_task(&msg);
        let parsed = parse_task(&s).unwrap();
        assert_eq!(parsed.id, "t-002");
        assert_eq!(parsed.call, "multiply");
        assert_eq!(parsed.args.len(), 2);
    }

    #[test]
    fn serialize_result_success() {
        let msg = ResultMessage {
            id: "t-001".into(),
            success: true,
            payload: Some(Value::Int(7)),
            error: None,
        };
        let s = serialize_result(&msg);
        assert!(s.contains("result"));
        assert!(s.contains(":complete"));
        assert!(s.contains("7"));
    }

    #[test]
    fn serialize_result_error() {
        let msg = ResultMessage {
            id: "t-001".into(),
            success: false,
            payload: None,
            error: Some("function not found".into()),
        };
        let s = serialize_result(&msg);
        assert!(s.contains(":error"));
        assert!(s.contains("function not found"));
    }

    #[test]
    fn parse_result_success_round_trip() {
        let msg = ResultMessage {
            id: "t-003".into(),
            success: true,
            payload: Some(Value::Int(42)),
            error: None,
        };
        let s = serialize_result(&msg);
        let parsed = parse_result(&s).unwrap();
        assert_eq!(parsed.id, "t-003");
        assert!(parsed.success);
        assert_eq!(parsed.payload, Some(Value::Int(42)));
    }

    #[test]
    fn sexpr_to_value_integers() {
        assert_eq!(sexpr_to_value_str("42").unwrap(), Value::Int(42));
    }

    #[test]
    fn sexpr_to_value_string() {
        assert_eq!(sexpr_to_value_str(r#""hello""#).unwrap(), Value::Str("hello".into()));
    }

    #[test]
    fn sexpr_to_value_bool() {
        assert_eq!(sexpr_to_value_str("true").unwrap(), Value::Bool(true));
    }

    #[test]
    fn serialize_task_escapes_string_args() {
        let msg = TaskMessage {
            id: "t-esc".into(),
            from: "cli".into(),
            call: "echo".into(),
            args: vec![Value::Str("hello \"world\"".into())],
        };
        let s = serialize_task(&msg);
        // The string arg should be safely escaped on the wire
        let parsed = parse_task(&s).unwrap();
        assert_eq!(parsed.args[0], Value::Str("hello \"world\"".into()));
    }

    #[test]
    fn parse_task_empty_input_returns_err() {
        assert!(parse_task("").is_err());
    }

    #[test]
    fn parse_result_empty_input_returns_err() {
        assert!(parse_result("").is_err());
    }
}
