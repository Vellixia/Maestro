use axum::{extract::{Query, State}, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error::ApiResult, state::AppState};

#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    pub days: Option<u32>,
    pub limit: Option<usize>,
}

pub async fn usage_stats(
    State(state): State<AppState>,
    Query(q): Query<UsageQuery>,
) -> ApiResult<Json<Value>> {
    let days = q.days.unwrap_or(7);
    let stats = storage::repos::UsageRepo::new(state.db.clone())
        .stats_last_n_days(days)
        .await?;
    Ok(Json(json!({
        "period_days": days,
        "total_requests": stats.total_requests,
        "total_prompt_tokens": stats.total_prompt_tokens,
        "total_completion_tokens": stats.total_completion_tokens,
        "total_cost_usd": stats.total_cost_usd,
    })))
}

pub async fn usage_recent(
    State(state): State<AppState>,
    Query(q): Query<UsageQuery>,
) -> ApiResult<Json<Value>> {
    let limit = q.limit.unwrap_or(50);
    let rows = storage::repos::UsageRepo::new(state.db.clone())
        .list_recent(limit)
        .await?;
    Ok(Json(json!({"data": rows})))
}
