mod templates;

use std::sync::Arc;

use chrono::Utc;
use core_types::{OutputType, RunId, Stakes, TaskGraph, TaskId, TaskNode};
use gateway::{
    types::{ChatMessage, ChatRequest, MessageContent, MessageRole},
    GatewayClient,
};
use sha2::{Digest, Sha256};
use storage::PlanCacheRepo;
use tracing::{debug, info, warn};

pub struct Planner {
    gateway: Arc<GatewayClient>,
    cache: Option<PlanCacheRepo>,
    /// Model used for decomposition (high-reasoning).
    planner_model: String,
    /// Max nodes in a single plan.
    max_tasks: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum PlannerError {
    #[error("gateway: {0}")]
    Gateway(#[from] gateway::GatewayError),
    #[error("plan parse failed: {0}")]
    Parse(String),
    #[error("cycle in generated plan")]
    CyclicPlan,
}

impl Planner {
    pub fn new(gateway: Arc<GatewayClient>) -> Self {
        Self {
            gateway,
            cache: None,
            planner_model: "claude-sonnet-4-6".into(),
            max_tasks: 12,
        }
    }

    pub fn with_cache(mut self, db: storage::Db) -> Self {
        self.cache = Some(PlanCacheRepo::new(db));
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.planner_model = model.to_string();
        self
    }

    /// Decompose a goal into a TaskGraph.
    ///
    /// Trivial goals (short, no decomposition signals) skip LLM planning
    /// and return a single-task graph immediately.
    ///
    /// Non-trivial goals are checked against the plan cache first; on a miss,
    /// the LLM decomposes and the result is stored for future hits.
    pub async fn plan(&self, goal: &str, run_id: RunId) -> Result<TaskGraph, PlannerError> {
        if is_trivial(goal) {
            debug!(goal = goal, "trivial fast-path");
            return Ok(single_task_graph(goal, run_id));
        }

        // Template match — skip LLM decomposition for known patterns.
        if let Some(tname) = templates::detect_template(goal) {
            if let Some(graph) = templates::build_from_template(tname, goal, run_id.clone()) {
                info!(run_id = %run_id, template = tname, "template fast-path");
                return Ok(graph);
            }
        }

        let hash = goal_hash(goal);

        // Cache lookup
        if let Some(cache) = &self.cache {
            if let Ok(Some(cached)) = cache.get(&hash).await {
                info!(run_id = %run_id, "plan cache hit");
                let _ = cache.increment_hit(&hash).await;
                // Deserialize the cached graph, replacing run_id with the new one.
                if let Ok(mut graph) =
                    serde_json::from_value::<TaskGraph>(cached.graph_json)
                {
                    graph.run_id = run_id;
                    // Regenerate all TaskIds so subtask IDs are unique per run.
                    for node in &mut graph.nodes {
                        node.id = TaskId::new();
                    }
                    return Ok(graph);
                }
            }
        }

        let result = match self.llm_decompose(goal, run_id.clone()).await {
            Ok(graph) => {
                info!(run_id = %run_id, n_tasks = graph.nodes.len(), "plan ready");
                graph
            }
            Err(e) => {
                warn!("LLM planning failed ({e}), falling back to single task");
                single_task_graph(goal, run_id)
            }
        };

        // Cache the plan (fire and forget).
        if let Some(cache) = &self.cache {
            if let Ok(json) = serde_json::to_value(&result) {
                let cache_clone = PlanCacheRepo::new(cache.db().clone());
                let hash_clone = hash;
                let goal_s = goal.to_string();
                tokio::spawn(async move {
                    let _ = cache_clone.put(&hash_clone, &goal_s, json).await;
                });
            }
        }

        Ok(result)
    }

    async fn llm_decompose(&self, goal: &str, run_id: RunId) -> Result<TaskGraph, PlannerError> {
        let prompt = build_planning_prompt(goal, self.max_tasks);

        let req = ChatRequest {
            model: self.planner_model.clone(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Text(prompt),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            }],
            temperature: Some(0.2),
            max_tokens: Some(2048),
            stream: Some(false),
            top_p: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            extra: Default::default(),
        };

        let resp = self.gateway.chat(req).await?;
        let text = match resp {
            gateway::GatewayResponse::Complete(r) => r
                .choices
                .first()
                .map(|c| c.message.content.text().to_string())
                .unwrap_or_default(),
            gateway::GatewayResponse::Stream(_) => {
                return Err(PlannerError::Parse("got stream from planner model".into()))
            }
        };

        parse_plan_response(&text, goal, run_id)
    }
}

// ── Trivial fast-path ────────────────────────────────────────────────────────

/// Returns true if the goal is simple enough to skip planning.
fn is_trivial(goal: &str) -> bool {
    let word_count = goal.split_whitespace().count();
    if word_count > 60 {
        return false;
    }
    // Decomposition signals → not trivial
    let signals = [
        "step", "steps", "plan", "pipeline", "multiple", "each", "for each",
        "compare", "research", "analyze", "then", "after that", "first",
        "second", "third", "finally", "build", "create and", "design and",
        "generate and", "write and",
    ];
    let lower = goal.to_lowercase();
    !signals.iter().any(|s| lower.contains(s))
}

fn single_task_graph(goal: &str, run_id: RunId) -> TaskGraph {
    TaskGraph {
        run_id,
        goal: goal.to_string(),
        nodes: vec![TaskNode {
            id: TaskId::new(),
            instruction: goal.to_string(),
            depends_on: vec![],
            output_type: OutputType::Text,
            stakes: Stakes::Low,
            skill_hints: vec![],
            max_context_tokens: None,
        }],
        created_at: Utc::now(),
    }
}

// ── Prompt construction ──────────────────────────────────────────────────────

fn build_planning_prompt(goal: &str, max_tasks: usize) -> String {
    format!(
        r#"You are a task decomposition engine. Break the following GOAL into a minimal set of subtasks (at most {max_tasks}).

GOAL: {goal}

Rules:
- Each task must be self-contained with a clear instruction.
- Express dependencies explicitly via depends_on (list of task IDs).
- Keep the graph as flat/parallel as possible — only add dependencies when a task truly needs the output of another.
- output_type: one of "text", "code", "json", "math", "classification"
- stakes: one of "trivial", "low", "medium", "high"
- skill_hints: array of skill names from: reasoning, coding, math, instruction_following, factuality, writing

Respond with ONLY a JSON object, no explanation:
{{
  "tasks": [
    {{
      "id": "t1",
      "instruction": "...",
      "depends_on": [],
      "output_type": "text",
      "stakes": "medium",
      "skill_hints": ["writing"]
    }},
    {{
      "id": "t2",
      "instruction": "...",
      "depends_on": ["t1"],
      "output_type": "code",
      "stakes": "high",
      "skill_hints": ["coding"]
    }}
  ]
}}"#
    )
}

// ── Plan response parser ─────────────────────────────────────────────────────

fn parse_plan_response(
    text: &str,
    goal: &str,
    run_id: RunId,
) -> Result<TaskGraph, PlannerError> {
    let json_str = extract_json(text);
    let val: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| PlannerError::Parse(format!("JSON parse: {e}")))?;

    let tasks_arr = val
        .get("tasks")
        .and_then(|t| t.as_array())
        .ok_or_else(|| PlannerError::Parse("missing 'tasks' array".into()))?;

    if tasks_arr.is_empty() {
        return Err(PlannerError::Parse("empty tasks array".into()));
    }

    // First pass: collect id → TaskId mapping for depends_on resolution.
    let mut id_map: std::collections::HashMap<String, TaskId> = std::collections::HashMap::new();
    for task in tasks_arr {
        let raw_id = task
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("t")
            .to_string();
        id_map.insert(raw_id, TaskId::new());
    }

    let mut nodes = Vec::new();
    for task in tasks_arr {
        let raw_id = task
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("t")
            .to_string();
        let task_id = id_map[&raw_id].clone();

        let instruction = task
            .get("instruction")
            .and_then(|v| v.as_str())
            .unwrap_or(goal)
            .to_string();

        let depends_on: Vec<TaskId> = task
            .get("depends_on")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| id_map.get(s).cloned())
                    .collect()
            })
            .unwrap_or_default();

        let output_type = parse_output_type(
            task.get("output_type").and_then(|v| v.as_str()).unwrap_or("text"),
        );

        let stakes = parse_stakes(
            task.get("stakes").and_then(|v| v.as_str()).unwrap_or("medium"),
        );

        let skill_hints: Vec<String> = task
            .get("skill_hints")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        nodes.push(TaskNode {
            id: task_id,
            instruction,
            depends_on,
            output_type,
            stakes,
            skill_hints,
            max_context_tokens: None,
        });
    }

    let graph = TaskGraph { run_id, goal: goal.to_string(), nodes, created_at: Utc::now() };

    // Validate: detect cycles early.
    graph
        .topological_order()
        .map_err(|_| PlannerError::CyclicPlan)?;

    Ok(graph)
}

fn parse_output_type(s: &str) -> OutputType {
    match s {
        "code" => OutputType::Code { language: "auto".into() },
        "json" => OutputType::Json { schema: None },
        "math" => OutputType::Math,
        "classification" => OutputType::Classification,
        _ => OutputType::Text,
    }
}

fn parse_stakes(s: &str) -> Stakes {
    match s {
        "trivial" => Stakes::Trivial,
        "low" => Stakes::Low,
        "high" => Stakes::High,
        _ => Stakes::Medium,
    }
}

fn goal_hash(goal: &str) -> String {
    let normalized = goal.trim().to_lowercase();
    let hash = Sha256::digest(normalized.as_bytes());
    hex::encode(hash)
}

fn extract_json(text: &str) -> String {
    let s = text.trim();
    // Strip markdown fences
    if let Some(inner) = s.strip_prefix("```json").or_else(|| s.strip_prefix("```")) {
        return inner.trim_end_matches("```").trim().to_string();
    }
    // Find first { ... } block
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if start <= end {
            return s[start..=end].to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trivial_short_query() {
        assert!(is_trivial("What is the capital of France?"));
    }

    #[test]
    fn non_trivial_multi_step() {
        assert!(!is_trivial(
            "Research the top 5 competitors, then compare their pricing, \
             then write a report with recommendations"
        ));
    }

    #[test]
    fn parse_valid_plan() {
        let json = r#"{"tasks":[
          {"id":"t1","instruction":"Search for data","depends_on":[],"output_type":"text","stakes":"low","skill_hints":["factuality"]},
          {"id":"t2","instruction":"Summarize findings","depends_on":["t1"],"output_type":"text","stakes":"medium","skill_hints":["writing"]}
        ]}"#;
        let graph = parse_plan_response(json, "my goal", RunId::new()).unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.nodes[1].depends_on.len(), 1);
    }

    #[test]
    fn single_task_fallback() {
        let graph = single_task_graph("Say hello", RunId::new());
        assert_eq!(graph.nodes.len(), 1);
    }
}
