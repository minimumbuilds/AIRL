use airl_runtime::value::Value;

/// Result of executing a task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub id: String,
    pub status: TaskStatus,
    pub payload: Option<Value>,
}

/// Status of a completed task.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Complete,
    Error(String),
    Timeout,
}

impl TaskResult {
    /// Create a successful result with an optional payload.
    pub fn complete(id: impl Into<String>, payload: Option<Value>) -> Self {
        Self {
            id: id.into(),
            status: TaskStatus::Complete,
            payload,
        }
    }

    /// Create an error result.
    pub fn error(id: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: TaskStatus::Error(msg.into()),
            payload: None,
        }
    }

    /// Create a timeout result.
    pub fn timeout(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: TaskStatus::Timeout,
            payload: None,
        }
    }

    /// Returns true if the task completed successfully.
    pub fn is_complete(&self) -> bool {
        self.status == TaskStatus::Complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_result_complete() {
        let r = TaskResult::complete("task-1", Some(Value::Int(42)));
        assert!(r.is_complete());
        assert_eq!(r.id, "task-1");
    }

    #[test]
    fn task_result_error() {
        let r = TaskResult::error("task-2", "something went wrong");
        assert!(!r.is_complete());
        assert_eq!(r.status, TaskStatus::Error("something went wrong".into()));
    }

    #[test]
    fn task_result_timeout() {
        let r = TaskResult::timeout("task-3");
        assert!(!r.is_complete());
        assert_eq!(r.status, TaskStatus::Timeout);
    }
}
