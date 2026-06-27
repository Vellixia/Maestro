use std::collections::HashMap;
use std::sync::Arc;

use core_types::{TaskGraph, TaskId};
use gateway::{
    types::{ChatMessage, ChatRequest, MessageContent, MessageRole},
    GatewayClient,
};
use tracing::debug;

pub struct Synthesizer {
    gateway: Arc<GatewayClient>,
    /// Model used to reduce multiple terminal outputs into one answer.
    synthesis_model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SynthesizerError {
    #[error("gateway: {0}")]
    Gateway(#[from] gateway::GatewayError),
    #[error("no outputs to synthesize")]
    NoOutputs,
}

impl Synthesizer {
    pub fn new(gateway: Arc<GatewayClient>) -> Self {
        Self {
            gateway,
            synthesis_model: "claude-haiku-4-5".into(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.synthesis_model = model.to_string();
        self
    }

    /// Produce the final answer from all task outputs.
    ///
    /// - Single terminal node → return its output directly (no extra LLM call).
    /// - Multiple terminal nodes → call synthesis model to reduce into one answer.
    pub async fn synthesize(
        &self,
        goal: &str,
        outputs: &HashMap<TaskId, String>,
        graph: &TaskGraph,
    ) -> Result<String, SynthesizerError> {
        if outputs.is_empty() {
            return Err(SynthesizerError::NoOutputs);
        }

        let terminal_ids = terminal_nodes(graph);

        // Collect terminal outputs in topological order.
        let terminal_outputs: Vec<String> = graph
            .nodes
            .iter()
            .filter(|n| terminal_ids.contains(&n.id))
            .filter_map(|n| outputs.get(&n.id).cloned())
            .collect();

        if terminal_outputs.is_empty() {
            // Fall back: return the last output in insertion order.
            return Ok(outputs.values().last().cloned().unwrap_or_default());
        }

        if terminal_outputs.len() == 1 {
            debug!("single terminal node, returning output directly");
            return Ok(terminal_outputs.into_iter().next().unwrap());
        }

        // Multiple terminal outputs: synthesize via LLM.
        debug!(n = terminal_outputs.len(), "synthesizing multiple terminal outputs");
        self.reduce_with_llm(goal, &terminal_outputs).await
    }

    async fn reduce_with_llm(
        &self,
        goal: &str,
        parts: &[String],
    ) -> Result<String, SynthesizerError> {
        let mut prompt = format!(
            "You are synthesizing the results of several parallel subtasks into one \
             final answer for the following GOAL.\n\nGOAL: {goal}\n\n"
        );

        for (i, part) in parts.iter().enumerate() {
            prompt.push_str(&format!("## Subtask {} Result\n{part}\n\n", i + 1));
        }

        prompt.push_str(
            "Now produce a single, coherent final answer that incorporates all the above results. \
             Be concise but complete.",
        );

        let req = ChatRequest {
            model: self.synthesis_model.clone(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Text(prompt),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            }],
            temperature: Some(0.3),
            max_tokens: Some(4096),
            stream: Some(false),
            top_p: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            extra: Default::default(),
        };

        let resp = self.gateway.chat(req).await?;
        match resp {
            gateway::GatewayResponse::Complete(r) => Ok(r
                .choices
                .first()
                .map(|c| c.message.content.text().to_string())
                .unwrap_or_default()),
            gateway::GatewayResponse::Stream(_) => {
                Ok(parts.join("\n\n---\n\n")) // fallback: concatenate
            }
        }
    }
}

/// Find the IDs of terminal nodes (nodes that no other node depends on).
fn terminal_nodes(graph: &TaskGraph) -> std::collections::HashSet<TaskId> {
    let all_deps: std::collections::HashSet<String> = graph
        .nodes
        .iter()
        .flat_map(|n| n.depends_on.iter().map(|d| d.0.clone()))
        .collect();

    graph
        .nodes
        .iter()
        .filter(|n| !all_deps.contains(&n.id.0))
        .map(|n| n.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use core_types::{OutputType, RunId, Stakes, TaskNode};

    fn make_graph(deps: Vec<(usize, Vec<usize>)>) -> TaskGraph {
        let ids: Vec<TaskId> = (0..deps.len()).map(|_| TaskId::new()).collect();
        let nodes = deps
            .into_iter()
            .enumerate()
            .map(|(i, (_, dep_idxs))| TaskNode {
                id: ids[i].clone(),
                instruction: format!("task {i}"),
                depends_on: dep_idxs.into_iter().map(|j| ids[j].clone()).collect(),
                output_type: OutputType::Text,
                stakes: Stakes::Low,
                skill_hints: vec![],
                max_context_tokens: None,
            })
            .collect();
        TaskGraph { run_id: RunId::new(), goal: "test".into(), nodes, created_at: Utc::now() }
    }

    #[test]
    fn terminal_with_linear_chain() {
        // t0 → t1 → t2 (terminal)
        let graph = make_graph(vec![(0, vec![]), (1, vec![0]), (2, vec![1])]);
        let terminals = terminal_nodes(&graph);
        assert_eq!(terminals.len(), 1);
        assert!(terminals.contains(&graph.nodes[2].id));
    }

    #[test]
    fn terminal_with_fan_in() {
        // t0, t1 → t2 (terminal)
        let graph = make_graph(vec![(0, vec![]), (1, vec![]), (2, vec![0, 1])]);
        let terminals = terminal_nodes(&graph);
        assert_eq!(terminals.len(), 1);
        assert!(terminals.contains(&graph.nodes[2].id));
    }

    #[test]
    fn multiple_terminals() {
        // t0 → t1, t0 → t2 (t1 and t2 both terminal)
        let graph = make_graph(vec![(0, vec![]), (1, vec![0]), (2, vec![0])]);
        let terminals = terminal_nodes(&graph);
        assert_eq!(terminals.len(), 2);
    }
}
