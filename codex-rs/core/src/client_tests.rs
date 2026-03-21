use super::AuthRequestTelemetryContext;
use super::ModelClient;
use super::PendingUnauthorizedRetry;
use super::UnauthorizedRecoveryExecution;
use codex_otel::SessionTelemetry;
use codex_protocol::ThreadId;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::models::BaseInstructions;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;
use serde_json::json;

fn test_model_client(session_source: SessionSource) -> ModelClient {
    let provider = crate::model_provider_info::create_oss_provider_with_base_url(
        "https://example.com/v1",
        crate::model_provider_info::WireApi::Responses,
    );
    ModelClient::new(
        None,
        ThreadId::new(),
        provider,
        session_source,
        None,
        false,
        false,
        None,
    )
}

fn test_dynamo_extension_model_client(session_source: SessionSource) -> ModelClient {
    ModelClient::new(
        None,
        ThreadId::new(),
        crate::model_provider_info::create_dynamo_provider(),
        session_source,
        None,
        false,
        false,
        None,
    )
}

fn test_model_info() -> ModelInfo {
    serde_json::from_value(json!({
        "slug": "gpt-test",
        "display_name": "gpt-test",
        "description": "desc",
        "default_reasoning_level": "medium",
        "supported_reasoning_levels": [
            {"effort": "medium", "description": "medium"}
        ],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 1,
        "upgrade": null,
        "base_instructions": "base instructions",
        "model_messages": null,
        "supports_reasoning_summaries": false,
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "truncation_policy": {"mode": "bytes", "limit": 10000},
        "supports_parallel_tool_calls": false,
        "supports_image_detail_original": false,
        "context_window": 272000,
        "auto_compact_token_limit": null,
        "experimental_supported_tools": []
    }))
    .expect("deserialize test model info")
}

fn test_session_telemetry() -> SessionTelemetry {
    SessionTelemetry::new(
        ThreadId::new(),
        "gpt-test",
        "gpt-test",
        None,
        None,
        None,
        "test-originator".to_string(),
        false,
        "test-terminal".to_string(),
        SessionSource::Cli,
    )
}

fn spawned_subagent_session_source() -> SessionSource {
    SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: ThreadId::new(),
        depth: 1,
        agent_path: None,
        agent_nickname: None,
        agent_role: None,
    })
}

#[test]
fn build_subagent_headers_sets_other_subagent_label() {
    let client = test_model_client(SessionSource::SubAgent(SubAgentSource::Other(
        "memory_consolidation".to_string(),
    )));
    let headers = client.build_subagent_headers();
    let value = headers
        .get("x-openai-subagent")
        .and_then(|value| value.to_str().ok());
    assert_eq!(value, Some("memory_consolidation"));
}

#[tokio::test]
async fn summarize_memories_returns_empty_for_empty_input() {
    let client = test_model_client(SessionSource::Cli);
    let model_info = test_model_info();
    let session_telemetry = test_session_telemetry();

    let output = client
        .summarize_memories(Vec::new(), &model_info, None, &session_telemetry)
        .await
        .expect("empty summarize request should succeed");
    assert_eq!(output.len(), 0);
}

#[test]
fn auth_request_telemetry_context_tracks_attached_auth_and_retry_phase() {
    let auth_context = AuthRequestTelemetryContext::new(
        Some(crate::auth::AuthMode::Chatgpt),
        &crate::api_bridge::CoreAuthProvider::for_test(Some("access-token"), Some("workspace-123")),
        PendingUnauthorizedRetry::from_recovery(UnauthorizedRecoveryExecution {
            mode: "managed",
            phase: "refresh_token",
        }),
    );

    assert_eq!(auth_context.auth_mode, Some("Chatgpt"));
    assert!(auth_context.auth_header_attached);
    assert_eq!(auth_context.auth_header_name, Some("authorization"));
    assert!(auth_context.retry_after_unauthorized);
    assert_eq!(auth_context.recovery_mode, Some("managed"));
    assert_eq!(auth_context.recovery_phase, Some("refresh_token"));
}

#[test]
fn dynamo_provider_request_fields_open_session_on_first_request() {
    let client = test_dynamo_extension_model_client(spawned_subagent_session_source());
    let model_info = test_model_info();
    let provider = client
        .state
        .provider
        .to_api_provider(None)
        .expect("provider should be valid");
    let session = client.new_session();
    let prompt = crate::client_common::Prompt {
        base_instructions: BaseInstructions {
            text: "base instructions".to_string(),
        },
        ..Default::default()
    };

    let request = session
        .build_responses_request(
            &provider,
            &prompt,
            &model_info,
            None,
            ReasoningSummary::None,
            None,
        )
        .expect("request should build");

    assert_eq!(
        request
            .provider_request_fields
            .0
            .get("nvext")
            .and_then(|value| value.get("session_control"))
            .and_then(|session_control| session_control.get("action"))
            .and_then(|action| action.as_str()),
        Some("open")
    );
    assert_eq!(
        request
            .provider_request_fields
            .0
            .get("nvext")
            .and_then(|value| value.get("session_control"))
            .and_then(|session_control| session_control.get("timeout"))
            .and_then(serde_json::Value::as_u64),
        Some(30)
    );
    assert_eq!(
        request
            .provider_request_fields
            .0
            .get("nvext")
            .and_then(|value| value.get("cache_control"))
            .and_then(|cache_control| cache_control.get("type"))
            .and_then(|control_type| control_type.as_str()),
        Some("ephemeral")
    );
}

#[test]
fn dynamo_provider_request_fields_omit_open_action_after_first_request() {
    let client = test_dynamo_extension_model_client(spawned_subagent_session_source());
    let model_info = test_model_info();
    let provider = client
        .state
        .provider
        .to_api_provider(None)
        .expect("provider should be valid");
    let session = client.new_session();
    let prompt = crate::client_common::Prompt {
        base_instructions: BaseInstructions {
            text: "base instructions".to_string(),
        },
        ..Default::default()
    };

    let first_request = session
        .build_responses_request(
            &provider,
            &prompt,
            &model_info,
            None,
            ReasoningSummary::None,
            None,
        )
        .expect("first request should build");
    let second_request = session
        .build_responses_request(
            &provider,
            &prompt,
            &model_info,
            None,
            ReasoningSummary::None,
            None,
        )
        .expect("second request should build");

    assert_eq!(
        first_request
            .provider_request_fields
            .0
            .get("nvext")
            .and_then(|value| value.get("session_control"))
            .and_then(|session_control| session_control.get("action"))
            .and_then(|action| action.as_str()),
        Some("open")
    );
    assert_eq!(
        second_request
            .provider_request_fields
            .0
            .get("nvext")
            .and_then(|value| value.get("session_control"))
            .and_then(|session_control| session_control.get("action")),
        None
    );
}
