use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A run is one top-level orchestration request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunId(pub String);

impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What kind of output a task produces — drives verifier selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    Text,
    Code { language: String },
    Json { schema: Option<String> },
    Math,
    Classification,
}

/// Stakes level — controls verification strictness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stakes {
    /// Fast-path: skip verification.
    Trivial,
    /// Light self-consistency check.
    Low,
    /// Full output-type specific verification.
    Medium,
    /// Verification + human-review option on escalation failure.
    High,
}

/// A single node in the task DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: TaskId,
    pub instruction: String,
    /// IDs of tasks whose outputs feed into this task's input.
    pub depends_on: Vec<TaskId>,
    pub output_type: OutputType,
    pub stakes: Stakes,
    /// Hint from the planner about what skills this task needs.
    pub skill_hints: Vec<String>,
    /// Max tokens for the subtask's context (assembled by executor).
    pub max_context_tokens: Option<u32>,
}

/// The full task graph for one orchestration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGraph {
    pub run_id: RunId,
    pub goal: String,
    pub nodes: Vec<TaskNode>,
    pub created_at: DateTime<Utc>,
}

impl TaskGraph {
    /// Returns nodes in a valid topological order (respecting depends_on).
    /// Returns Err if a cycle is detected.
    pub fn topological_order(&self) -> Result<Vec<&TaskNode>, String> {
        use std::collections::{HashMap, HashSet, VecDeque};

        let node_map: HashMap<&str, &TaskNode> =
            self.nodes.iter().map(|n| (n.id.0.as_str(), n)).collect();

        let mut in_degree: HashMap<&str, usize> = self.nodes.iter()
            .map(|n| (n.id.0.as_str(), n.depends_on.len()))
            .collect();

        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in &self.nodes {
            for dep in &node.depends_on {
                dependents.entry(dep.0.as_str()).or_default().push(node.id.0.as_str());
            }
        }

        let mut queue: VecDeque<&str> = in_degree.iter()
            .filter(|(_, &d)| d == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut result = Vec::new();
        let mut visited: HashSet<&str> = HashSet::new();

        while let Some(id) = queue.pop_front() {
            if visited.contains(id) {
                continue;
            }
            visited.insert(id);
            result.push(*node_map.get(id).ok_or_else(|| format!("unknown task id: {id}"))?);
            if let Some(deps) = dependents.get(id) {
                for &dep in deps {
                    if let Some(d) = in_degree.get_mut(dep) {
                        *d = d.saturating_sub(1);
                        if *d == 0 {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err("Task graph contains a cycle".to_string());
        }

        Ok(result)
    }

    /// Returns groups of tasks that can run in parallel (all deps satisfied by previous groups).
    pub fn parallel_groups(&self) -> Result<Vec<Vec<&TaskNode>>, String> {
        use std::collections::{HashMap, HashSet};

        let order = self.topological_order()?;
        let mut level: HashMap<&str, usize> = HashMap::new();

        for node in &order {
            let lv = node.depends_on.iter()
                .map(|d| level.get(d.0.as_str()).copied().unwrap_or(0) + 1)
                .max()
                .unwrap_or(0);
            level.insert(node.id.0.as_str(), lv);
        }

        let max_level = level.values().copied().max().unwrap_or(0);
        let mut groups: Vec<Vec<&TaskNode>> = vec![vec![]; max_level + 1];

        for node in &self.nodes {
            let lv = *level.get(node.id.0.as_str()).unwrap_or(&0);
            groups[lv].push(node);
        }

        Ok(groups.into_iter().filter(|g| !g.is_empty()).collect())
    }
}

/// A single attempt to execute a task node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAttempt {
    pub id: String,
    pub task_id: TaskId,
    pub run_id: RunId,
    pub connection_id: String,
    pub model_id: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub output: Option<String>,
    pub verify_result: VerifyResult,
    /// If this attempt was triggered by escalation, the previous attempt id.
    pub escalated_from: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyResult {
    Passed,
    Failed { reason: String },
    Skipped,
}

impl VerifyResult {
    pub fn passed(&self) -> bool {
        matches!(self, VerifyResult::Passed | VerifyResult::Skipped)
    }
}
