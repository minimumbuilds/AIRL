use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;

use crate::identity::{AgentId, Endpoint};
use crate::protocol::{parse_task, serialize_result, ResultMessage};
use crate::registry::AgentRegistry;
use crate::task::TaskStatus;
use crate::tcp_transport::TcpTransport;
use crate::transport::{Transport, TransportError};
use airl_runtime::eval::Interpreter;

/// Errors from the agent runtime.
#[derive(Debug)]
pub enum AgentError {
    Transport(TransportError),
    TaskNotFound(String),
    Protocol(String),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::Transport(e) => write!(f, "agent transport error: {}", e),
            AgentError::TaskNotFound(id) => write!(f, "task not found: {}", id),
            AgentError::Protocol(msg) => write!(f, "protocol error: {}", msg),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<TransportError> for AgentError {
    fn from(e: TransportError) -> Self {
        AgentError::Transport(e)
    }
}

/// The agent runtime manages identity, peer registry, and pending tasks.
///
/// Phase 1 keeps this deliberately simple — the full message loop and
/// task execution pipeline will be wired up when the driver is ready.
pub struct AgentRuntime {
    pub identity: AgentId,
    pub registry: AgentRegistry,
    pending: HashMap<String, TaskStatus>,
    next_task_id: u64,
}

impl AgentRuntime {
    pub fn new(identity: AgentId) -> Self {
        Self {
            identity,
            registry: AgentRegistry::new(),
            pending: HashMap::new(),
            next_task_id: 0,
        }
    }

    /// Generate a unique task ID for this agent.
    pub fn next_task_id(&mut self) -> String {
        let id = format!("{}-task-{}", self.identity.name, self.next_task_id);
        self.next_task_id += 1;
        id
    }

    /// Record a task as pending.
    pub fn track_pending(&mut self, task_id: String) {
        self.pending.insert(task_id, TaskStatus::Complete);
    }

    /// Mark a pending task as completed with the given result.
    pub fn complete_task(&mut self, task_id: &str, status: TaskStatus) -> Result<(), AgentError> {
        if self.pending.contains_key(task_id) {
            self.pending.insert(task_id.to_string(), status);
            Ok(())
        } else {
            Err(AgentError::TaskNotFound(task_id.to_string()))
        }
    }

    /// Check if a task is still tracked.
    pub fn task_status(&self, task_id: &str) -> Option<&TaskStatus> {
        self.pending.get(task_id)
    }

    /// Remove a completed task from the pending map.
    pub fn remove_task(&mut self, task_id: &str) -> Option<TaskStatus> {
        self.pending.remove(task_id)
    }

    /// Number of pending tasks.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

/// Parse an endpoint string like "tcp:127.0.0.1:9001" or "unix:/tmp/airl.sock".
pub fn parse_endpoint(s: &str) -> Result<Endpoint, String> {
    if s == "stdio" {
        Ok(Endpoint::Stdio)
    } else if let Some(addr_str) = s.strip_prefix("tcp:") {
        let addr: SocketAddr = addr_str.parse()
            .map_err(|e| format!("invalid TCP address '{}': {}", addr_str, e))?;
        Ok(Endpoint::Tcp(addr))
    } else if let Some(path_str) = s.strip_prefix("unix:") {
        Ok(Endpoint::Unix(PathBuf::from(path_str)))
    } else {
        Err(format!("unknown endpoint format: '{}' (expected tcp:HOST:PORT, unix:/path, or stdio)", s))
    }
}

/// Load an AIRL module file and start listening for tasks.
pub fn run_agent_loop(module_path: &str, endpoint: &Endpoint) -> Result<(), AgentError> {
    // 1. Load module
    let source = std::fs::read_to_string(module_path)
        .map_err(|e| AgentError::Protocol(format!("cannot read {}: {}", module_path, e)))?;

    let mut interp = Interpreter::new();
    load_module(&source, &mut interp)?;

    eprintln!("Agent loaded: {}", module_path);

    // 2. Bind listener
    match endpoint {
        Endpoint::Tcp(addr) => {
            let listener = TcpListener::bind(addr)
                .map_err(|e| AgentError::Protocol(format!("cannot bind {}: {}", addr, e)))?;
            eprintln!("Listening on tcp:{}", addr);

            // 3. Accept loop
            loop {
                let (stream, peer) = listener.accept()
                    .map_err(|e| AgentError::Protocol(format!("accept error: {}", e)))?;
                eprintln!("Connection from {}", peer);

                let mut transport = TcpTransport::from_stream(stream);
                handle_connection(&mut transport, &mut interp);
                eprintln!("Connection closed from {}", peer);
            }
        }
        Endpoint::Stdio => {
            eprintln!("Agent listening on stdio");
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let mut reader = std::io::BufReader::new(stdin.lock());
            let mut writer = std::io::BufWriter::new(stdout.lock());

            // Send ready handshake so spawner knows we're initialized
            crate::transport::write_frame(&mut writer, "ready")
                .map_err(|e| AgentError::Protocol(format!("cannot send ready: {}", e)))?;

            handle_stdio_connection(&mut reader, &mut writer, &mut interp);
            Ok(())
        }
        _ => Err(AgentError::Protocol("unsupported endpoint type".into())),
    }
}

/// Parse AIRL source, evaluate top-level forms to register functions.
pub fn load_module(source: &str, interp: &mut Interpreter) -> Result<(), AgentError> {
    let mut lexer = airl_syntax::Lexer::new(source);
    let tokens = lexer.lex_all()
        .map_err(|d| AgentError::Protocol(format!("parse error: {}", d.message)))?;
    let sexprs = airl_syntax::parse_sexpr_all(&tokens)
        .map_err(|d| AgentError::Protocol(format!("parse error: {}", d.message)))?;
    let mut diags = airl_syntax::Diagnostics::new();

    for sexpr in &sexprs {
        match airl_syntax::parser::parse_top_level(sexpr, &mut diags) {
            Ok(top) => {
                interp.eval_top_level(&top)
                    .map_err(|e| AgentError::Protocol(format!("module error: {}", e)))?;
            }
            Err(_) => {} // skip unparseable forms
        }
    }
    Ok(())
}

/// Read-eval-respond loop on a single connection.
pub fn handle_connection(transport: &mut dyn Transport, interp: &mut Interpreter) {
    loop {
        let frame = match transport.recv_message() {
            Ok(f) => f,
            Err(_) => break, // disconnected or error -> close connection
        };

        let result_msg = match parse_task(&frame) {
            Ok(task) => {
                eprintln!("Task {}: calling {}({:?})", task.id, task.call, task.args);
                match interp.call_by_name(&task.call, task.args) {
                    Ok(value) => ResultMessage {
                        id: task.id,
                        success: true,
                        payload: Some(value),
                        error: None,
                    },
                    Err(e) => ResultMessage {
                        id: task.id,
                        success: false,
                        payload: None,
                        error: Some(format!("{}", e)),
                    },
                }
            }
            Err(e) => ResultMessage {
                id: "unknown".into(),
                success: false,
                payload: None,
                error: Some(format!("protocol error: {}", e)),
            },
        };

        let response = serialize_result(&result_msg);
        if transport.send_message(&response).is_err() {
            break;
        }
    }
}

/// Read-eval-respond loop on stdio (for child-process agents).
fn handle_stdio_connection(
    reader: &mut dyn std::io::Read,
    writer: &mut dyn std::io::Write,
    interp: &mut Interpreter,
) {
    use crate::transport::{read_frame, write_frame};

    loop {
        let frame = match read_frame(reader) {
            Ok(f) => f,
            Err(_) => break,
        };

        let result_msg = match parse_task(&frame) {
            Ok(task) => {
                eprintln!("Task {}: calling {}({:?})", task.id, task.call, task.args);
                match interp.call_by_name(&task.call, task.args) {
                    Ok(value) => ResultMessage {
                        id: task.id,
                        success: true,
                        payload: Some(value),
                        error: None,
                    },
                    Err(e) => ResultMessage {
                        id: task.id,
                        success: false,
                        payload: None,
                        error: Some(format!("{}", e)),
                    },
                }
            }
            Err(e) => ResultMessage {
                id: "unknown".into(),
                success: false,
                payload: None,
                error: Some(format!("protocol error: {}", e)),
            },
        };

        let response = serialize_result(&result_msg);
        if write_frame(writer, &response).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::*;
    use airl_runtime::value::Value;

    fn test_identity() -> AgentId {
        AgentId {
            name: "test-agent".into(),
            capabilities: vec![Capability::ComputeCpu],
            trust_level: TrustLevel::Verified,
            endpoint: Endpoint::Stdio,
        }
    }

    #[test]
    fn runtime_creation() {
        let rt = AgentRuntime::new(test_identity());
        assert_eq!(rt.identity.name, "test-agent");
        assert_eq!(rt.pending_count(), 0);
    }

    #[test]
    fn task_id_generation() {
        let mut rt = AgentRuntime::new(test_identity());
        assert_eq!(rt.next_task_id(), "test-agent-task-0");
        assert_eq!(rt.next_task_id(), "test-agent-task-1");
        assert_eq!(rt.next_task_id(), "test-agent-task-2");
    }

    #[test]
    fn track_and_complete_task() {
        let mut rt = AgentRuntime::new(test_identity());
        let id = rt.next_task_id();
        rt.track_pending(id.clone());
        assert_eq!(rt.pending_count(), 1);

        rt.complete_task(&id, TaskStatus::Complete).unwrap();
        assert_eq!(rt.task_status(&id), Some(&TaskStatus::Complete));

        let status = rt.remove_task(&id);
        assert_eq!(status, Some(TaskStatus::Complete));
        assert_eq!(rt.pending_count(), 0);
    }

    #[test]
    fn complete_nonexistent_task() {
        let mut rt = AgentRuntime::new(test_identity());
        let result = rt.complete_task("bogus", TaskStatus::Complete);
        assert!(result.is_err());
    }

    #[test]
    fn registry_integration() {
        let mut rt = AgentRuntime::new(test_identity());
        let peer = AgentId {
            name: "gpu-worker".into(),
            capabilities: vec![Capability::ComputeGpu],
            trust_level: TrustLevel::Verified,
            endpoint: Endpoint::Tcp("127.0.0.1:9000".parse().unwrap()),
        };
        rt.registry.register(peer);
        assert!(rt.registry.lookup("gpu-worker").is_some());
    }

    #[test]
    fn parse_tcp_endpoint() {
        let ep = parse_endpoint("tcp:127.0.0.1:9001").unwrap();
        assert!(matches!(ep, Endpoint::Tcp(addr) if addr.port() == 9001));
    }

    #[test]
    fn parse_unix_endpoint() {
        let ep = parse_endpoint("unix:/tmp/airl.sock").unwrap();
        assert!(matches!(ep, Endpoint::Unix(ref p) if p.to_str().unwrap() == "/tmp/airl.sock"));
    }

    #[test]
    fn parse_stdio_endpoint() {
        let ep = parse_endpoint("stdio").unwrap();
        assert!(matches!(ep, Endpoint::Stdio));
    }

    #[test]
    fn parse_invalid_endpoint() {
        assert!(parse_endpoint("garbage").is_err());
    }

    #[test]
    fn agent_loop_integration() {
        use crate::protocol::parse_result;
        use std::thread;
        use std::time::Duration;

        // Write a temp module file
        let dir = std::env::temp_dir().join("airl-test-agent");
        std::fs::create_dir_all(&dir).ok();
        let module_path = dir.join("worker.airl");
        std::fs::write(&module_path, r#"
            (defn add
              :sig [(a : i32) (b : i32) -> i32]
              :intent "add"
              :requires [(valid a) (valid b)]
              :ensures [(= result (+ a b))]
              :body (+ a b))
        "#).unwrap();

        // Find a free port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let module_path_str = module_path.to_str().unwrap().to_string();
        let endpoint = Endpoint::Tcp(addr);

        // Start agent in background thread
        let _handle = thread::spawn(move || {
            let _ = run_agent_loop(&module_path_str, &endpoint);
        });

        // Give agent time to bind
        thread::sleep(Duration::from_millis(200));

        // Connect as client
        let mut client = TcpTransport::connect(addr).unwrap();
        let task_str = r#"(task "t-1" :from "test" :call "add" :args [3 4])"#;
        client.send_message(task_str).unwrap();
        let response = client.recv_message().unwrap();
        client.close().ok();

        // Parse response
        let result = parse_result(&response).unwrap();
        assert!(result.success);
        assert_eq!(result.payload, Some(Value::Int(7)));

        // Cleanup
        std::fs::remove_file(&module_path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn load_module_registers_functions() {
        let source = r#"
            (defn double
              :sig [(x : i32) -> i32]
              :intent "double"
              :requires [(valid x)]
              :ensures [(valid result)]
              :body (* x 2))
        "#;
        let mut interp = Interpreter::new();
        load_module(source, &mut interp).unwrap();
        let result = interp.call_by_name("double", vec![Value::Int(21)]).unwrap();
        assert_eq!(result, Value::Int(42));
    }
}
