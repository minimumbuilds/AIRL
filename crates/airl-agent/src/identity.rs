use std::net::SocketAddr;
use std::path::PathBuf;

/// Unique identity for an agent in the AIRL multi-agent system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId {
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub trust_level: TrustLevel,
    pub endpoint: Endpoint,
}

/// Capabilities that an agent can advertise.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Capability {
    ComputeGpu,
    ComputeCpu,
    WebSearch,
    CodeExecution,
    FileAccess,
    AgentSpawn,
    Custom(String),
}

/// Trust level for an agent, determining what operations it may perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrustLevel {
    None,
    Verified,
    Proven,
}

/// Network endpoint where an agent can be reached.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Endpoint {
    Tcp(SocketAddr),
    Unix(PathBuf),
    Stdio,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_id_equality() {
        let a = AgentId {
            name: "worker-1".into(),
            capabilities: vec![Capability::ComputeCpu],
            trust_level: TrustLevel::Verified,
            endpoint: Endpoint::Stdio,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn capability_custom() {
        let c = Capability::Custom("llm-inference".into());
        assert_eq!(c, Capability::Custom("llm-inference".into()));
        assert_ne!(c, Capability::ComputeGpu);
    }

    #[test]
    fn trust_level_ordering() {
        assert_ne!(TrustLevel::None, TrustLevel::Verified);
        assert_ne!(TrustLevel::Verified, TrustLevel::Proven);
    }

    #[test]
    fn endpoint_variants() {
        let tcp = Endpoint::Tcp("127.0.0.1:8080".parse().unwrap());
        let unix = Endpoint::Unix(PathBuf::from("/tmp/agent.sock"));
        let stdio = Endpoint::Stdio;
        assert_ne!(tcp, unix);
        assert_ne!(unix, stdio);
    }
}
