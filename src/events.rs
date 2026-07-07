use chrono::{DateTime, Utc};
use serde::Serialize;

/// Every agent action emits a typed event.
/// The TUI subscribes to these for rendering.
/// A log file subscribes for benchmarking.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    /// User submitted a prompt
    UserMessage { text: String },

    /// Agent started a new step (LLM call)
    StepStarted { step: usize, tokens_in: usize },

    /// Streaming token from the model
    Token { text: String },

    /// Model is emitting thinking/reasoning tokens
    Thinking { text: String },

    /// Model requested a tool call
    ToolCallStarted {
        step: usize,
        tool: String,
        args: serde_json::Value,
    },

    /// Tool call completed
    ToolCallFinished {
        step: usize,
        tool: String,
        duration_ms: u64,
        success: bool,
        result_preview: String,
    },

    /// Agent finished responding
    Done {
        step: usize,
        total_tokens: usize,
        duration_ms: u64,
    },

    /// Context budget warning
    BudgetWarning { used: usize, max: usize },

    /// Local command finished (unlocks UI)
    CommandFinished,

    /// Error
    Error { message: String },

    /// Telemetry Update
    TelemetryUpdate { rx_kbps: f64, tx_kbps: f64 },

    /// Network Threat Detected
    NetworkThreatDetected {
        severity: String,
        description: String,
    },
    /// Start an interactive ASCII art animation in the TUI
    StartArt { mode: String },
}

/// A timestamped event for logging
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct TimestampedEvent {
    pub timestamp: DateTime<Utc>,
    pub event: AgentEvent,
}

impl AgentEvent {
    #[allow(dead_code)]
    pub fn stamped(self) -> TimestampedEvent {
        TimestampedEvent {
            timestamp: Utc::now(),
            event: self,
        }
    }
}
