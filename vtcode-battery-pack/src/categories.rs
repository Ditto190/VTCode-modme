/// A curated category within the battery pack.
///
/// Categories group related crates and declare whether the consumer should
/// pick at most one or any number from the group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryPackCategory {
    /// Core agent runtime and event loop.
    Core,
    /// Ratatui-based terminal UI and design system.
    Tui,
    /// Config loader components shared across VT Code.
    Config,
    /// Shared traits for paths, telemetry, and error reporting.
    Commons,
    /// Procedural macros for VT Code.
    Macros,
    /// Open protocols: A2A, ACP (Zed), MCP, ATIF.
    Protocols,
    /// LLM provider abstraction and 21+ provider implementations.
    Llm,
    /// Skill discovery, loading, validation, and bundling.
    Skills,
    /// Command safety detection, execution policies, and sandboxing.
    Safety,
    /// Shell execution, PTY management, and execution telemetry.
    Execution,
    /// Session state, durable loop state, and progress tracking.
    State,
    /// Tool specs, code indexing, fuzzy search, and outline mode.
    Tools,
    /// Agent evaluation framework: pass@k / pass^k metrics.
    Eval,
    /// OAuth and credential storage.
    Auth,
}

impl BatteryPackCategory {
    /// Unique identifier used as the Cargo feature name.
    pub fn id(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Tui => "tui",
            Self::Config => "config",
            Self::Commons => "commons",
            Self::Macros => "macros",
            Self::Protocols => "protocols",
            Self::Llm => "llm",
            Self::Skills => "skills",
            Self::Safety => "safety",
            Self::Execution => "execution",
            Self::State => "state",
            Self::Tools => "tools",
            Self::Eval => "eval",
            Self::Auth => "auth",
        }
    }

    /// Human-readable category name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Core => "Core Runtime",
            Self::Tui => "Terminal UI",
            Self::Config => "Configuration",
            Self::Commons => "Shared Utilities",
            Self::Macros => "Procedural Macros",
            Self::Protocols => "Agent Protocols",
            Self::Llm => "LLM Providers",
            Self::Skills => "Skills",
            Self::Safety => "Safety",
            Self::Execution => "Execution",
            Self::State => "State & Memory",
            Self::Tools => "Tools & Search",
            Self::Eval => "Evaluation",
            Self::Auth => "Authentication",
        }
    }

    /// Short description of what the category contains.
    pub fn description(self) -> &'static str {
        match self {
            Self::Core => "The agent runtime, TUI, and event loop",
            Self::Tui => "Ratatui-based terminal UI and design system",
            Self::Config => "Config loader components shared across VT Code",
            Self::Commons => "Shared traits for paths, telemetry, and error reporting",
            Self::Macros => "Procedural macros for VT Code",
            Self::Protocols => "Open protocols: A2A, ACP (Zed), MCP, ATIF",
            Self::Llm => "Provider abstraction and 21+ provider implementations",
            Self::Skills => "Skill discovery, loading, validation, and bundling",
            Self::Safety => "Command safety detection, execution policies, and sandboxing",
            Self::Execution => "Shell execution, PTY management, and execution telemetry",
            Self::State => "Session state, durable loop state, and progress tracking",
            Self::Tools => "Tool specs, code indexing, fuzzy search, and outline mode",
            Self::Eval => "Agent evaluation framework: pass@k / pass^k metrics",
            Self::Auth => "Authentication and OAuth flows shared across VT Code",
        }
    }

    /// All categories in the pack.
    pub fn all() -> &'static [Self] {
        &[
            Self::Core,
            Self::Tui,
            Self::Config,
            Self::Commons,
            Self::Macros,
            Self::Protocols,
            Self::Llm,
            Self::Skills,
            Self::Safety,
            Self::Execution,
            Self::State,
            Self::Tools,
            Self::Eval,
            Self::Auth,
        ]
    }
}
