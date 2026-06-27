use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use core_types::{
    CapabilityProfile, Policy, RunId, SkillDimension, TaskGraph, TaskId,
    TaskNode, TraceEvent, VerifyResult,
};
use gateway::{
    types::{ChatMessage, ChatRequest, MessageContent, MessageRole},
    GatewayClient,
};
use registry::ModelRegistry;
use tokio::sync::{mpsc, Semaphore};
use tracing::{debug, warn};
use tools::ToolExecutor;

/// Max concurrent requests per connection (free-tier guard).
const DEFAULT_CONN_PERMITS: usize = 4;
/// Max tool-call turns per task before forcing a final answer.
const MAX_TOOL_TURNS: usize = 6;

pub struct DagExecutor {
    gateway: Arc<GatewayClient>,
    registry: Arc<ModelRegistry>,
    /// Per-connection semaphores to bound parallel requests against the same provider.
    conn_semaphores: Arc<dashmap::DashMap<String, Arc<Semaphore>>>,
    tool_executor: Arc<ToolExecutor>,
}

impl DagExecutor {
    pub fn new(gateway: Arc<GatewayClient>, registry: Arc<ModelRegistry>) -> Self {
        Self {
            gateway,
            registry,
            conn_semaphores: Arc::new(dashmap::DashMap::new()),
            tool_executor: Arc::new(ToolExecutor::new()),
        }
    }

}

#[derive(Debug)]
pub struct ExecutionResult {
    /// Raw output string per task node.
    pub outputs: HashMap<TaskId, String>,
    pub trace_events: Vec<TraceEvent>,
    pub total_cost_usd: f64,
    pub total_tokens: u64,
    pub wall_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutorError {
    #[error("gateway: {0}")]
    Gateway(#[from] gateway::GatewayError),
    #[error("routing: {0}")]
    Routing(String),
    #[error("task cycle")]
    Cycle,
    #[error("task {0} failed after all escalations: {1}")]
    TaskFailed(String, String),
    #[error("registry: {0}")]
    Registry(#[from] registry::RegistryError),
}

impl DagExecutor {
    /// Execute a full TaskGraph, returning outputs and trace.
    /// `trace_tx` receives live TraceEvents as tasks run (None = discard).
    pub async fn execute(
        &self,
        graph: &TaskGraph,
        policy: &Policy,
        trace_tx: Option<mpsc::Sender<TraceEvent>>,
    ) -> Result<ExecutionResult, ExecutorError> {
        let wall_start = Instant::now();
        let run_id = graph.run_id.clone();

        let send = |ev: TraceEvent| {
            if let Some(tx) = &trace_tx {
                let _ = tx.try_send(ev);
            }
        };

        send(TraceEvent::RunStarted {
            run_id: run_id.clone(),
            goal: graph.goal.clone(),
            ts: Utc::now(),
        });

        // Topological groups — each group can run in parallel.
        let groups = graph
            .parallel_groups()
            .map_err(|_| ExecutorError::Cycle)?;

        send(TraceEvent::PlanReady {
            run_id: run_id.clone(),
            n_tasks: graph.nodes.len(),
            ts: Utc::now(),
        });

        // Fetch all routable profiles once.
        let profiles = self.registry.list_routable().await?;

        let mut outputs: HashMap<TaskId, String> = HashMap::new();
        let mut trace_events: Vec<TraceEvent> = Vec::new();
        let mut total_cost_usd: f64 = 0.0;
        let mut total_tokens: u64 = 0;

        for group in &groups {
            // Run all tasks in this group in parallel.
            let mut join_set = tokio::task::JoinSet::new();

            for &node in group {
                let context = assemble_context(node, &outputs);
                let profiles_clone = profiles.clone();
                let policy_clone = policy.clone();
                let gateway = Arc::clone(&self.gateway);
                let registry = Arc::clone(&self.registry);
                let tools = Arc::clone(&self.tool_executor);
                let node_clone = node.clone();
                let run_id_clone = run_id.clone();
                let trace_tx_clone = trace_tx.clone();
                let spent_so_far = total_cost_usd;
                let conn_semaphores = Arc::clone(&self.conn_semaphores);

                join_set.spawn(async move {
                    execute_node(
                        &node_clone,
                        &context,
                        &profiles_clone,
                        &policy_clone,
                        &gateway,
                        &registry,
                        &run_id_clone,
                        trace_tx_clone,
                        spent_so_far,
                        conn_semaphores,
                        tools,
                    )
                    .await
                });
            }

            // Collect results from this group.
            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok((task_id, output, cost, tokens, events))) => {
                        outputs.insert(task_id, output);
                        total_cost_usd += cost;
                        total_tokens += tokens;
                        trace_events.extend(events);
                    }
                    Ok(Err(e)) => {
                        let err_str = e.to_string();
                        send(TraceEvent::RunFailed {
                            run_id: run_id.clone(),
                            error: err_str.clone(),
                            ts: Utc::now(),
                        });
                        return Err(e);
                    }
                    Err(join_err) => {
                        return Err(ExecutorError::Routing(format!("task panicked: {join_err}")));
                    }
                }
            }
        }

        let wall_ms = wall_start.elapsed().as_millis() as u64;

        send(TraceEvent::RunCompleted {
            run_id: run_id.clone(),
            total_cost_usd,
            total_tokens,
            wall_ms,
            ts: Utc::now(),
        });

        Ok(ExecutionResult { outputs, trace_events, total_cost_usd, total_tokens, wall_ms })
    }
}

// ── Per-node execution ───────────────────────────────────────────────────────

type NodeResult = Result<(TaskId, String, f64, u64, Vec<TraceEvent>), ExecutorError>;

#[allow(clippy::too_many_arguments)]
async fn execute_node(
    node: &TaskNode,
    context: &str,
    profiles: &[CapabilityProfile],
    policy: &Policy,
    gateway: &GatewayClient,
    registry: &ModelRegistry,
    run_id: &RunId,
    trace_tx: Option<mpsc::Sender<TraceEvent>>,
    spent_usd: f64,
    conn_semaphores: Arc<dashmap::DashMap<String, Arc<Semaphore>>>,
    tool_executor: Arc<ToolExecutor>,
) -> NodeResult {
    let send = |ev: TraceEvent| {
        if let Some(tx) = &trace_tx {
            let _ = tx.try_send(ev);
        }
    };

    let mut events: Vec<TraceEvent> = Vec::new();

    let context_tokens = (context.len() as u32) / 4;
    let requirement = classifier::classify(&node.instruction, context_tokens);

    let routing = router::route(&requirement, policy, profiles, spent_usd)
        .map_err(|e| ExecutorError::Routing(e.to_string()))?;

    let full_instruction = if context.is_empty() {
        node.instruction.clone()
    } else {
        format!("Context from previous tasks:\n{context}\n\n---\n\nTask: {}", node.instruction)
    };

    let candidates = std::iter::once(&routing.primary)
        .chain(routing.escalation_ladder.iter());

    let mut cost_usd: f64 = 0.0;
    let mut tokens: u64 = 0;
    let mut last_output = String::new();
    let mut escalated_from: Option<String> = None;

    for profile in candidates {
        let ev = TraceEvent::TaskAssigned {
            run_id: run_id.clone(),
            task_id: node.id.clone(),
            model_id: profile.model_id.clone(),
            connection_id: profile.connection_id.0.clone(),
            reason: if escalated_from.is_some() { "escalated" } else { "primary" }.into(),
            ts: Utc::now(),
        };
        send(ev.clone());
        events.push(ev);

        let ev = TraceEvent::TaskStarted {
            run_id: run_id.clone(),
            task_id: node.id.clone(),
            ts: Utc::now(),
        };
        send(ev.clone());
        events.push(ev);

        // Enable tools for models that support them.
        let tools_for_req = if profile.hard.supports_tools {
            Some(ToolExecutor::definitions())
        } else {
            None
        };

        let initial_message = ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Text(full_instruction.clone()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        };

        // ── Tool-calling loop ────────────────────────────────────────────────
        let mut messages: Vec<ChatMessage> = vec![initial_message];
        let mut tool_turn = 0;
        let mut prompt_tok_total: u32 = 0;
        let mut comp_tok_total: u32 = 0;
        let task_start = Instant::now();
        let output: String;

        loop {
            let req = ChatRequest {
                model: profile.model_id.clone(),
                messages: messages.clone(),
                temperature: Some(0.3),
                max_tokens: node.max_context_tokens,
                stream: Some(false),
                top_p: None,
                tools: tools_for_req.clone(),
                tool_choice: None,
                response_format: None,
                extra: Default::default(),
            };

            // Acquire per-connection semaphore permit before making the network call.
            let conn_permits = conn_semaphores
                .entry(profile.connection_id.0.clone())
                .or_insert_with(|| Arc::new(Semaphore::new(DEFAULT_CONN_PERMITS)))
                .clone();
            let _permit = conn_permits.acquire().await.ok();

            let resp = match gateway
                .chat_on_connection(req, &profile.connection_id.0, &profile.model_id)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!(model = %profile.model_id, "chat failed: {e}");
                    output = String::new();
                    break;
                }
            };

            let complete = match resp {
                gateway::GatewayResponse::Complete(r) => r,
                gateway::GatewayResponse::Stream(_) => {
                    warn!("executor got stream for non-stream request");
                    output = String::new();
                    break;
                }
            };

            prompt_tok_total += complete.usage.prompt_tokens;
            comp_tok_total += complete.usage.completion_tokens;

            let choice = match complete.choices.first() {
                Some(c) => c,
                None => { output = String::new(); break; }
            };

            // Check if model wants to call tools.
            let tool_calls = choice.message.tool_calls.as_ref()
                .filter(|tc| !tc.is_empty() && tool_turn < MAX_TOOL_TURNS);

            if let Some(calls) = tool_calls {
                // Append assistant turn with tool calls.
                messages.push(choice.message.clone());

                // Execute each tool call and append results.
                for tc in calls {
                    let call_id = tc["id"].as_str().map(String::from);
                    let fn_name = tc["function"]["name"].as_str().unwrap_or("unknown");
                    let fn_args: serde_json::Value = tc["function"]["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(serde_json::Value::Object(Default::default()));

                    debug!(tool = fn_name, "executing tool call");
                    let result = tool_executor.execute(fn_name, fn_args).await;

                    messages.push(ChatMessage {
                        role: MessageRole::Tool,
                        content: MessageContent::Text(result),
                        tool_call_id: call_id,
                        name: Some(fn_name.to_string()),
                        tool_calls: None,
                    });
                }
                tool_turn += 1;
                continue; // loop back with tool results
            }

            // No tool calls — this is the final answer.
            output = choice.message.content.text().to_string();
            break;
        }

        let latency_ms = task_start.elapsed().as_millis() as u64;
        let node_cost = estimate_cost(profile, prompt_tok_total, comp_tok_total);
        cost_usd += node_cost;
        tokens += (prompt_tok_total + comp_tok_total) as u64;

        if output.is_empty() {
            escalated_from = Some(profile.model_id.clone());
            continue;
        }

        last_output = output.clone();

        let verify_result = verifier::verify(
            &output,
            &node.instruction,
            &node.output_type,
            &node.stakes,
            gateway,
        )
        .await;

        debug!(task_id = %node.id, model = %profile.model_id, passed = verify_result.passed(), "verify");

        let ev = TraceEvent::TaskCompleted {
            run_id: run_id.clone(),
            task_id: node.id.clone(),
            prompt_tokens: prompt_tok_total,
            completion_tokens: comp_tok_total,
            cost_usd: node_cost,
            latency_ms,
            verify_result: verify_result.clone(),
            ts: Utc::now(),
        };
        send(ev.clone());
        events.push(ev);

        let _ = registry
            .apply_online_update(
                &profile.connection_id.0,
                &profile.model_id,
                primary_skill_hint(&node.skill_hints),
                verify_result.passed(),
            )
            .await;

        if verify_result.passed() {
            return Ok((node.id.clone(), output, cost_usd, tokens, events));
        }

        let reason = match &verify_result {
            VerifyResult::Failed { reason } => reason.clone(),
            _ => "verification skipped or failed".into(),
        };

        if let Some(next_model) = next_in_ladder(&profile.model_id, &routing.escalation_ladder) {
            let ev = TraceEvent::TaskEscalated {
                run_id: run_id.clone(),
                task_id: node.id.clone(),
                from_model: profile.model_id.clone(),
                to_model: next_model.clone(),
                reason,
                ts: Utc::now(),
            };
            send(ev.clone());
            events.push(ev);
            escalated_from = Some(profile.model_id.clone());
        }
    }

    // All candidates tried — return last output if we have one, else fail.
    if !last_output.is_empty() {
        warn!(task_id = %node.id, "all models tried, returning last output despite failed verification");
        return Ok((node.id.clone(), last_output, cost_usd, tokens, events));
    }

    let ev = TraceEvent::TaskFailed {
        run_id: run_id.clone(),
        task_id: node.id.clone(),
        error: "all models exhausted".into(),
        ts: Utc::now(),
    };
    events.push(ev);
    Err(ExecutorError::TaskFailed(
        node.id.0.clone(),
        "all models exhausted without output".into(),
    ))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Build context string from dependency outputs.
fn assemble_context(node: &TaskNode, outputs: &HashMap<TaskId, String>) -> String {
    if node.depends_on.is_empty() {
        return String::new();
    }
    node.depends_on
        .iter()
        .filter_map(|dep_id| outputs.get(dep_id))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

fn estimate_cost(profile: &CapabilityProfile, prompt_tokens: u32, comp_tokens: u32) -> f64 {
    let in_cost = (prompt_tokens as f64 / 1_000_000.0) * profile.ops.cost_in_per_m;
    let out_cost = (comp_tokens as f64 / 1_000_000.0) * profile.ops.cost_out_per_m;
    in_cost + out_cost
}

fn primary_skill_hint(hints: &[String]) -> SkillDimension {
    for h in hints {
        match h.as_str() {
            "coding" => return SkillDimension::Coding,
            "math" => return SkillDimension::Math,
            "reasoning" => return SkillDimension::Reasoning,
            "writing" => return SkillDimension::Writing,
            "factuality" => return SkillDimension::Factuality,
            _ => {}
        }
    }
    SkillDimension::Reasoning
}

fn next_in_ladder(
    current_model: &str,
    ladder: &[CapabilityProfile],
) -> Option<String> {
    // Find the model after the current one in the ladder.
    let mut found = false;
    for p in ladder {
        if found {
            return Some(p.model_id.clone());
        }
        if p.model_id == current_model {
            found = true;
        }
    }
    // current_model is not in the ladder — return first entry (it's the escalation target).
    ladder.first().map(|p| p.model_id.clone())
}
