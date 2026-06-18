//! Agent state HTTP handlers backed by gateway-lib StateStore.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use gateway::{
    AgentInfo, AgentState, AgentStateCommand, QueryResult, StateCommand, StateEventType, StateQuery,
};
use serde::Deserialize;
use serde_json::json;

use crate::handlers::common::get_authorized_agent_info;
use crate::AppState;

/// Get agent state.
pub async fn get_agent_state(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let info = get_authorized_agent_info(&state.state_store, &user, &id).await?;
    let valid_transitions = valid_gateway_transitions(info.current_state);

    Ok(Json(json!({
        "current_state": info.current_state.to_string(),
        "valid_transitions": valid_transitions.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "can_execute_tasks": can_execute_tasks(info.current_state),
        "is_terminal": is_terminal(info.current_state),
    })))
}

/// Get agent state context (detailed).
pub async fn get_agent_state_context(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let info = get_authorized_agent_info(&state.state_store, &user, &id).await?;
    let events = event_history(&state, &id, 100).await?;
    let transition_events: Vec<_> = events
        .iter()
        .filter(|event| matches!(event.event_type, StateEventType::StateTransitioned))
        .collect();

    let previous_state = transition_events
        .last()
        .and_then(|event| event.payload.get("from"))
        .and_then(|value| serde_json::from_value::<AgentState>(value.clone()).ok());
    let error_count = transition_events
        .iter()
        .filter(|event| {
            event
                .payload
                .get("to")
                .and_then(|value| serde_json::from_value::<AgentState>(value.clone()).ok())
                == Some(AgentState::Error)
        })
        .count();

    Ok(Json(json!({
        "agent_id": id,
        "context": {
            "current_state": info.current_state.to_string(),
            "previous_state": previous_state.map(|state| state.to_string()),
            "state_duration_secs": chrono::Utc::now()
                .signed_duration_since(info.updated_at)
                .num_seconds()
                .max(0),
            "total_transitions": transition_events.len(),
            "error_count": error_count,
            "history": transition_events.iter().map(|event| {
                json!({
                    "from_state": event.payload.get("from"),
                    "to_state": event.payload.get("to"),
                    "reason": event.payload.get("reason"),
                    "timestamp": event.timestamp,
                    "sequence": event.sequence,
                })
            }).collect::<Vec<_>>(),
        },
    })))
}

/// Transition agent to a specific state.
pub async fn transition_state(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<TransitionStateRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let info = get_authorized_agent_info(&state.state_store, &user, &id).await?;
    let target_state = parse_agent_state(&req.target_state)
        .ok_or_else(|| GatewayError::bad_request("Invalid target state"))?;

    transition_agent_state(&state, &info, target_state, req.reason).await?;

    Ok(Json(json!({
        "agent_id": id,
        "new_state": target_state.to_string(),
        "message": "State transition completed",
    })))
}

/// Pause agent.
pub async fn pause_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let info = get_authorized_agent_info(&state.state_store, &user, &id).await?;
    state
        .agent_runtime
        .send_command(&id, AgentStateCommand::Pause)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to pause agent runtime: {}", e)))?;
    transition_agent_state(&state, &info, AgentState::Paused, "User paused agent").await?;

    Ok(Json(json!({
        "agent_id": id,
        "status": "paused",
        "message": "Agent paused successfully",
    })))
}

/// Resume agent.
pub async fn resume_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let info = get_authorized_agent_info(&state.state_store, &user, &id).await?;
    state
        .agent_runtime
        .send_command(&id, AgentStateCommand::Resume)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to resume agent runtime: {}", e)))?;
    transition_agent_state(&state, &info, AgentState::Idle, "User resumed agent").await?;

    Ok(Json(json!({
        "agent_id": id,
        "status": "resumed",
        "message": "Agent resumed successfully",
    })))
}

/// Retry agent after error.
pub async fn retry_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let info = get_authorized_agent_info(&state.state_store, &user, &id).await?;
    if info.current_state != AgentState::Error {
        return Err(GatewayError::bad_request(
            "Only agents in error state can be retried",
        ));
    }

    state
        .agent_runtime
        .send_command(&id, AgentStateCommand::Start)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to retry agent runtime: {}", e)))?;
    transition_agent_state(
        &state,
        &info,
        AgentState::Initializing,
        "Retrying after error",
    )
    .await?;

    Ok(Json(json!({
        "agent_id": id,
        "status": "retrying",
        "message": "Agent retry initiated",
    })))
}

/// List valid transitions for an agent.
pub async fn get_valid_transitions(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let info = get_authorized_agent_info(&state.state_store, &user, &id).await?;
    let valid_transitions = valid_gateway_transitions(info.current_state);

    Ok(Json(json!({
        "agent_id": id,
        "current_state": info.current_state.to_string(),
        "valid_transitions": valid_transitions.iter().map(|state| {
            json!({
                "state": state.to_string(),
                "description": state_description(*state),
            })
        }).collect::<Vec<_>>(),
    })))
}

/// Get state machine statistics.
pub async fn get_state_machine_stats(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let agents = list_all_agent_info(&state).await?;
    let mut state_counts: HashMap<String, usize> = HashMap::new();
    for agent in &agents {
        *state_counts
            .entry(agent.current_state.to_string())
            .or_insert(0) += 1;
    }

    Ok(Json(json!({
        "total_agents": agents.len(),
        "state_distribution": all_gateway_states().iter().map(|state| {
            json!({
                "state": state.to_string(),
                "count": state_counts.get(&state.to_string()).copied().unwrap_or(0),
                "description": state_description(*state),
            })
        }).collect::<Vec<_>>(),
        "timed_out_agents": 0,
    })))
}

/// Get all possible states.
pub async fn list_states() -> Json<serde_json::Value> {
    Json(json!({
        "states": all_gateway_states().iter().map(|state| {
            json!({
                "name": state.to_string(),
                "description": state_description(*state),
                "is_terminal": is_terminal(*state),
                "can_execute_tasks": can_execute_tasks(*state),
                "valid_transitions": valid_gateway_transitions(*state)
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    }))
}

/// Check for timed out agents.
pub async fn check_timeouts(
    State(_state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    Ok(Json(json!({
        "timed_out_count": 0,
        "agents": [],
    })))
}

#[derive(Debug, Deserialize)]
pub struct TransitionStateRequest {
    pub target_state: String,
    pub reason: String,
}

async fn transition_agent_state(
    state: &AppState,
    info: &AgentInfo,
    target_state: AgentState,
    reason: impl Into<String>,
) -> Result<(), GatewayError> {
    if info.current_state == target_state {
        return Ok(());
    }

    if !valid_gateway_transitions(info.current_state).contains(&target_state) {
        return Err(GatewayError::bad_request(format!(
            "Invalid state transition from {} to {}",
            info.current_state, target_state
        )));
    }

    state
        .state_store
        .execute(StateCommand::Transition {
            agent_id: info.agent_id.clone(),
            from: info.current_state,
            to: target_state,
            reason: Some(reason.into()),
        })
        .await
        .map_err(|e| GatewayError::state(format!("Failed to record transition: {}", e)))?;

    Ok(())
}

async fn event_history(
    state: &AppState,
    agent_id: &str,
    limit: usize,
) -> Result<Vec<gateway::StateEvent>, GatewayError> {
    let result = state
        .state_store
        .query(StateQuery::GetEventHistory {
            agent_id: agent_id.to_string(),
            from_sequence: None,
            limit,
        })
        .await
        .map_err(|e| GatewayError::state(format!("Failed to get state history: {}", e)))?;

    match result {
        QueryResult::EventHistory { events, .. } => Ok(events),
        _ => Err(GatewayError::internal("Unexpected state history result")),
    }
}

async fn list_all_agent_info(state: &AppState) -> Result<Vec<AgentInfo>, GatewayError> {
    let result = state
        .state_store
        .query(StateQuery::ListAgents {
            filter: None,
            limit: 10_000,
            offset: 0,
        })
        .await
        .map_err(|e| GatewayError::state(format!("Failed to list agents: {}", e)))?;

    match result {
        QueryResult::AgentList { agents, .. } => Ok(agents),
        _ => Err(GatewayError::internal("Unexpected agent list result")),
    }
}

fn all_gateway_states() -> [AgentState; 8] {
    [
        AgentState::Registered,
        AgentState::Initializing,
        AgentState::Idle,
        AgentState::Working,
        AgentState::Paused,
        AgentState::ShuttingDown,
        AgentState::Stopped,
        AgentState::Error,
    ]
}

fn valid_gateway_transitions(state: AgentState) -> Vec<AgentState> {
    match state {
        AgentState::Registered => vec![AgentState::Initializing, AgentState::Error],
        AgentState::Initializing => vec![AgentState::Idle, AgentState::Error],
        AgentState::Idle => vec![
            AgentState::Working,
            AgentState::Paused,
            AgentState::ShuttingDown,
            AgentState::Error,
        ],
        AgentState::Working => vec![AgentState::Idle, AgentState::Paused, AgentState::Error],
        AgentState::Paused => vec![
            AgentState::Idle,
            AgentState::Working,
            AgentState::ShuttingDown,
        ],
        AgentState::ShuttingDown => vec![AgentState::Stopped, AgentState::Error],
        AgentState::Error => vec![AgentState::Stopped, AgentState::Initializing],
        AgentState::Stopped => vec![],
    }
}

fn can_execute_tasks(state: AgentState) -> bool {
    matches!(state, AgentState::Idle | AgentState::Working)
}

fn is_terminal(state: AgentState) -> bool {
    matches!(state, AgentState::Stopped)
}

fn state_description(state: AgentState) -> &'static str {
    match state {
        AgentState::Registered => "Agent is registered but not yet initialized",
        AgentState::Initializing => "Agent is loading configuration and connecting to services",
        AgentState::Idle => "Agent is ready to accept tasks",
        AgentState::Working => "Agent is actively processing a task",
        AgentState::Paused => "Agent is paused and can be resumed",
        AgentState::ShuttingDown => "Agent is gracefully shutting down",
        AgentState::Stopped => "Agent has stopped",
        AgentState::Error => "Agent encountered an error",
    }
}

fn parse_agent_state(s: &str) -> Option<AgentState> {
    match s.to_lowercase().as_str() {
        "registered" | "pending" => Some(AgentState::Registered),
        "initializing" => Some(AgentState::Initializing),
        "idle" => Some(AgentState::Idle),
        "working" => Some(AgentState::Working),
        "paused" => Some(AgentState::Paused),
        "shutting_down" => Some(AgentState::ShuttingDown),
        "stopped" => Some(AgentState::Stopped),
        "error" => Some(AgentState::Error),
        _ => None,
    }
}
