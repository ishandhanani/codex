//! NVIDIA Dynamo extension types for the Responses API.
//!
//! These types mirror the `nvext` field in Dynamo's OpenAI-compatible API surface.
//! When serialized into a `ResponsesApiRequest`, they enable Dynamo-specific features:
//! - **Session control**: sticky routing and KV isolation for subagent workloads
//! - **Cache control**: retention-based KV eviction with configurable TTL
//! - **Agent hints**: priority, expected output length, and speculative prefill
//!
//! Non-Dynamo providers ignore the `nvext` field (it is `skip_serializing_if = None`).

use serde::{Deserialize, Serialize};

/// NVIDIA Dynamo extensions to the OpenAI Responses API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NvExt {
    /// Session control for subagent KV isolation and sticky routing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_control: Option<SessionControl>,

    /// Cache control hint for retention-based KV eviction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,

    /// Agent-provided hints for request handling.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_hints: Option<AgentHints>,

    /// Extra fields to include in the response nvext (e.g., "worker_id", "timing").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_fields: Option<Vec<String>>,
}

/// Session control for subagent KV isolation and sticky routing.
///
/// Always requires `session_id`. The `action` field is optional:
/// - `action: "open"` on the first turn creates a streaming session on the worker
/// - `action: "close"` on the last turn frees session KV after generation
/// - No `action` on intermediate turns -- just provides `session_id` for sticky routing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionControl {
    /// Unique session identifier. Present on every turn for sticky routing.
    pub session_id: String,

    /// Lifecycle action: `"open"` or `"close"`. Omit on intermediate turns.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<SessionAction>,

    /// Inactivity timeout in seconds (default 300, only used with `action: "open"`).
    #[serde(default = "default_session_timeout")]
    pub timeout: u64,
}

fn default_session_timeout() -> u64 {
    300
}

/// Session lifecycle actions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionAction {
    Open,
    Close,
}

/// Cache control hint for retention-based KV eviction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheControl {
    /// Control type (currently only "ephemeral").
    #[serde(rename = "type")]
    pub control_type: String,

    /// TTL as seconds or shorthand ("5m", "1h"). Clamped by Dynamo to [300, 3600].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
}

/// Agent-provided hints for request handling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentHints {
    /// Unified request priority. Higher = more important.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,

    /// Expected output sequence length (number of output tokens).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub osl: Option<u32>,

    /// When true, speculatively prefill the next-turn prefix after completion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speculative_prefill: Option<bool>,
}

/// NVIDIA extension fields in the response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NvExtResponse {
    /// Worker ID information (prefill and decode worker IDs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<WorkerIdInfo>,

    /// Per-request timing information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<serde_json::Value>,
}

/// Worker ID information for disaggregated serving.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerIdInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefill_worker_id: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_worker_id: Option<u64>,
}

/// Dynamo-specific configuration for session and cache control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DynamoConfig {
    /// Inject session_control on subagent turns for sticky routing and KV isolation.
    #[serde(default = "default_true")]
    pub enable_session_control: bool,

    /// Inject cache_control on all turns for retention-based KV eviction.
    #[serde(default = "default_true")]
    pub enable_cache_control: bool,

    /// Default cache TTL (e.g., "5m", "1h", "600").
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl: String,

    /// Session inactivity timeout in seconds.
    #[serde(default = "default_session_timeout")]
    pub session_timeout: u64,

    /// Extra fields to request in response nvext (e.g., ["worker_id", "timing"]).
    #[serde(default)]
    pub extra_fields: Vec<String>,
}

impl Default for DynamoConfig {
    fn default() -> Self {
        Self {
            enable_session_control: true,
            enable_cache_control: true,
            cache_ttl: default_cache_ttl(),
            session_timeout: default_session_timeout(),
            extra_fields: vec!["worker_id".to_string()],
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_cache_ttl() -> String {
    "5m".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvext_serialization_round_trip() {
        let nvext = NvExt {
            session_control: Some(SessionControl {
                session_id: "codex-sub-1".to_string(),
                action: Some(SessionAction::Open),
                timeout: 120,
            }),
            cache_control: Some(CacheControl {
                control_type: "ephemeral".to_string(),
                ttl: Some("5m".to_string()),
            }),
            agent_hints: None,
            extra_fields: Some(vec!["worker_id".to_string()]),
        };

        let json = serde_json::to_value(&nvext).unwrap();
        assert_eq!(json["session_control"]["session_id"], "codex-sub-1");
        assert_eq!(json["session_control"]["action"], "open");
        assert_eq!(json["session_control"]["timeout"], 120);
        assert_eq!(json["cache_control"]["type"], "ephemeral");
        assert_eq!(json["cache_control"]["ttl"], "5m");
        assert!(json.get("agent_hints").is_none());

        let deserialized: NvExt = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized, nvext);
    }

    #[test]
    fn nvext_intermediate_turn_omits_action() {
        let nvext = NvExt {
            session_control: Some(SessionControl {
                session_id: "codex-sub-1".to_string(),
                action: None,
                timeout: 300,
            }),
            cache_control: None,
            agent_hints: None,
            extra_fields: None,
        };

        let json = serde_json::to_value(&nvext).unwrap();
        assert!(json["session_control"].get("action").is_none());
    }

    #[test]
    fn session_action_close_serialization() {
        let sc = SessionControl {
            session_id: "test".to_string(),
            action: Some(SessionAction::Close),
            timeout: 300,
        };
        let json = serde_json::to_value(&sc).unwrap();
        assert_eq!(json["action"], "close");
    }

    #[test]
    fn dynamo_config_defaults() {
        let config = DynamoConfig::default();
        assert!(config.enable_session_control);
        assert!(config.enable_cache_control);
        assert_eq!(config.cache_ttl, "5m");
        assert_eq!(config.session_timeout, 300);
        assert_eq!(config.extra_fields, vec!["worker_id"]);
    }
}
