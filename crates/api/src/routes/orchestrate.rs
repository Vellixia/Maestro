use axum::{
    extract::State,
    response::{IntoResponse, Response, Sse},
    Json,
};
use axum::response::sse::Event;
use chrono::Utc;
use core_types::{Policy, RunId, TraceEvent};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use serde_json::json;
use storage::{RunRepo, TraceEventRepo};
use storage::repos::run::StoredRun;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tracing::info;

use crate::{error::{ApiError, ApiResult}, state::AppState};

#[derive(Debug, Deserialize)]
pub struct OrchestrateRequest {
    pub goal: String,
    #[serde(default)]
    pub policy: Policy,
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Serialize)]
pub struct OrchestrateResult {
    pub run_id: String,
    pub result: String,
    pub total_cost_usd: f64,
    pub total_tokens: u64,
    pub wall_ms: u64,
}

pub async fn orchestrate(
    State(state): State<AppState>,
    Json(req): Json<OrchestrateRequest>,
) -> ApiResult<Response> {
    let run_id = RunId::new();
    info!(run_id = %run_id, goal = %req.goal, "orchestrate request");

    // Persist the run record immediately.
    let run_repo = RunRepo::new(state.db.clone());
    let stored_run = StoredRun {
        id: None,
        run_id: run_id.0.clone(),
        goal: req.goal.clone(),
        status: "running".into(),
        policy: serde_json::to_value(&req.policy).unwrap_or_default(),
        total_cost: 0.0,
        total_tokens: 0,
        wall_ms: None,
        error: None,
        created_at: Utc::now(),
        completed_at: None,
    };
    let _ = run_repo.create(&stored_run).await;

    if req.stream {
        return orchestrate_streaming(state, req, run_id).await;
    }
    orchestrate_blocking(state, req, run_id).await
}

async fn orchestrate_streaming(
    state: AppState,
    req: OrchestrateRequest,
    run_id: RunId,
) -> ApiResult<Response> {
    let (trace_tx, trace_rx) = mpsc::channel::<TraceEvent>(128);

    let run_id_clone = run_id.clone();
    let goal = req.goal.clone();
    let policy = req.policy.clone();
    let state_clone = state.clone();

    tokio::spawn(async move {
        let _ = run_pipeline(&state_clone, &goal, policy, run_id_clone, Some(trace_tx)).await;
    });

    let event_stream = ReceiverStream::new(trace_rx)
        .map(|ev| {
            let data = serde_json::to_string(&ev).unwrap_or_default();
            Ok::<Event, std::convert::Infallible>(
                Event::default()
                    .event(event_type_name(&ev))
                    .data(data),
            )
        })
        .chain(stream::once(async {
            Ok::<Event, std::convert::Infallible>(Event::default().data("[DONE]"))
        }));

    Ok(Sse::new(event_stream).into_response())
}

async fn orchestrate_blocking(
    state: AppState,
    req: OrchestrateRequest,
    run_id: RunId,
) -> ApiResult<Response> {
    let result = run_pipeline(&state, &req.goal, req.policy, run_id.clone(), None).await;
    match result {
        Ok(out) => Ok(Json(json!({
            "run_id": run_id.0,
            "result": out.result,
            "total_cost_usd": out.total_cost_usd,
            "total_tokens": out.total_tokens,
            "wall_ms": out.wall_ms,
        }))
        .into_response()),
        Err(e) => Err(ApiError::Internal(e.to_string())),
    }
}

/// Core orchestration pipeline: plan → execute → synthesize.
/// Persists run completion/failure and all trace events to SurrealDB.
async fn run_pipeline(
    state: &AppState,
    goal: &str,
    policy: Policy,
    run_id: RunId,
    trace_tx: Option<mpsc::Sender<TraceEvent>>,
) -> Result<OrchestrateResult, String> {
    let run_repo = RunRepo::new(state.db.clone());
    let _trace_repo = TraceEventRepo::new(state.db.clone());

    // Intercept trace events: fan-out to both SSE channel and SurrealDB.
    let (inner_tx, mut inner_rx) = mpsc::channel::<TraceEvent>(256);

    // Spawn a task that drains inner_rx → SSE tx + DB.
    let trace_repo_2 = TraceEventRepo::new(state.db.clone());
    let run_id_str = run_id.0.clone();
    let fwd_tx = trace_tx.clone();
    tokio::spawn(async move {
        while let Some(ev) = inner_rx.recv().await {
            // Forward to SSE stream.
            if let Some(tx) = &fwd_tx {
                let _ = tx.send(ev.clone()).await;
            }
            // Persist to DB.
            let ev_type = event_type_name(&ev);
            let data = serde_json::to_value(&ev).unwrap_or_default();
            let _ = trace_repo_2
                .append(&run_id_str, ev_type, data, Utc::now())
                .await;
        }
    });

    // 1. Plan
    let graph = state
        .planner
        .plan(goal, run_id.clone())
        .await
        .map_err(|e| format!("planning failed: {e}"))?;

    // 2. Execute
    let exec_result = state
        .executor
        .execute(&graph, &policy, Some(inner_tx))
        .await
        .map_err(|e| {
            let err_str = format!("execution failed: {e}");
            let repo = RunRepo::new(state.db.clone());
            let id = run_id.0.clone();
            let msg = err_str.clone();
            tokio::spawn(async move { let _ = repo.fail(&id, &msg).await; });
            err_str
        })?;

    // 3. Synthesize
    let result = state
        .synthesizer
        .synthesize(goal, &exec_result.outputs, &graph)
        .await
        .map_err(|e| format!("synthesis failed: {e}"))?;

    // Persist completion.
    let _ = run_repo
        .complete(
            &run_id.0,
            exec_result.total_cost_usd,
            exec_result.total_tokens as i64,
            exec_result.wall_ms as i64,
        )
        .await;

    Ok(OrchestrateResult {
        run_id: graph.run_id.0,
        result,
        total_cost_usd: exec_result.total_cost_usd,
        total_tokens: exec_result.total_tokens,
        wall_ms: exec_result.wall_ms,
    })
}

fn event_type_name(ev: &TraceEvent) -> &'static str {
    match ev {
        TraceEvent::RunStarted { .. } => "run_started",
        TraceEvent::PlanReady { .. } => "plan_ready",
        TraceEvent::TaskAssigned { .. } => "task_assigned",
        TraceEvent::TaskStarted { .. } => "task_started",
        TraceEvent::TaskCompleted { .. } => "task_completed",
        TraceEvent::TaskEscalated { .. } => "task_escalated",
        TraceEvent::TaskFailed { .. } => "task_failed",
        TraceEvent::RunCompleted { .. } => "run_completed",
        TraceEvent::RunFailed { .. } => "run_failed",
    }
}
