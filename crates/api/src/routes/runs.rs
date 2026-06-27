use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use storage::{RunRepo, TraceEventRepo};

use crate::{error::{ApiError, ApiResult}, state::AppState};

#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize { 20 }

#[derive(Debug, Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub goal: String,
    pub status: String,
    pub total_cost_usd: f64,
    pub total_tokens: i64,
    pub wall_ms: Option<i64>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

pub async fn list_runs(
    State(state): State<AppState>,
    Query(q): Query<ListRunsQuery>,
) -> ApiResult<Json<Vec<RunSummary>>> {
    let repo = RunRepo::new(state.db.clone());
    let runs = repo.list_recent(q.limit).await.map_err(ApiError::Storage)?;
    let summaries = runs
        .into_iter()
        .map(|r| RunSummary {
            run_id: r.run_id,
            goal: r.goal,
            status: r.status,
            total_cost_usd: r.total_cost,
            total_tokens: r.total_tokens,
            wall_ms: r.wall_ms,
            created_at: r.created_at.to_rfc3339(),
            completed_at: r.completed_at.map(|t| t.to_rfc3339()),
        })
        .collect();
    Ok(Json(summaries))
}

pub async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<RunSummary>> {
    let repo = RunRepo::new(state.db.clone());
    let r = repo.get(&run_id).await.map_err(|e| match e {
        storage::StorageError::NotFound(_) => ApiError::NotFound(run_id),
        other => ApiError::Storage(other),
    })?;
    Ok(Json(RunSummary {
        run_id: r.run_id,
        goal: r.goal,
        status: r.status,
        total_cost_usd: r.total_cost,
        total_tokens: r.total_tokens,
        wall_ms: r.wall_ms,
        created_at: r.created_at.to_rfc3339(),
        completed_at: r.completed_at.map(|t| t.to_rfc3339()),
    }))
}

#[derive(Debug, Serialize)]
pub struct TraceEntry {
    pub event_type: String,
    pub data: serde_json::Value,
    pub ts: String,
}

pub async fn get_run_plan(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let repo = RunRepo::new(state.db.clone());
    let run = repo.get(&run_id).await.map_err(|e| match e {
        storage::StorageError::NotFound(_) => ApiError::NotFound(run_id),
        other => ApiError::Storage(other),
    })?;
    Ok(Json(run.plan_graph.unwrap_or(serde_json::Value::Null)))
}

pub async fn get_run_trace(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<Vec<TraceEntry>>> {
    let repo = TraceEventRepo::new(state.db.clone());
    let events = repo.list_for_run(&run_id).await.map_err(ApiError::Storage)?;
    let entries = events
        .into_iter()
        .map(|e| TraceEntry {
            event_type: e.event_type,
            data: e.data,
            ts: e.ts.to_rfc3339(),
        })
        .collect();
    Ok(Json(entries))
}
