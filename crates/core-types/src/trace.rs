use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{RunId, TaskId, VerifyResult};

/// Event stream for a single run — streamed to the client and persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceEvent {
    RunStarted {
        run_id: RunId,
        goal: String,
        ts: DateTime<Utc>,
    },
    PlanReady {
        run_id: RunId,
        n_tasks: usize,
        ts: DateTime<Utc>,
    },
    TaskAssigned {
        run_id: RunId,
        task_id: TaskId,
        model_id: String,
        connection_id: String,
        reason: String,
        ts: DateTime<Utc>,
    },
    TaskStarted {
        run_id: RunId,
        task_id: TaskId,
        ts: DateTime<Utc>,
    },
    TaskCompleted {
        run_id: RunId,
        task_id: TaskId,
        prompt_tokens: u32,
        completion_tokens: u32,
        cost_usd: f64,
        latency_ms: u64,
        verify_result: VerifyResult,
        ts: DateTime<Utc>,
    },
    TaskEscalated {
        run_id: RunId,
        task_id: TaskId,
        from_model: String,
        to_model: String,
        reason: String,
        ts: DateTime<Utc>,
    },
    TaskFailed {
        run_id: RunId,
        task_id: TaskId,
        error: String,
        ts: DateTime<Utc>,
    },
    RunCompleted {
        run_id: RunId,
        total_cost_usd: f64,
        total_tokens: u64,
        wall_ms: u64,
        ts: DateTime<Utc>,
    },
    RunFailed {
        run_id: RunId,
        error: String,
        ts: DateTime<Utc>,
    },
}

/// Full trace for a completed run (for the Trace UI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTrace {
    pub run_id: RunId,
    pub events: Vec<TraceEvent>,
    pub total_cost_usd: f64,
    pub total_tokens: u64,
    pub wall_ms: u64,
    pub completed_at: DateTime<Utc>,
}
