use chrono::Utc;
use core_types::{OutputType, RunId, Stakes, TaskGraph, TaskId, TaskNode};

/// A named template pattern that can short-circuit LLM decomposition.
pub struct TemplateMatch {
    pub name: &'static str,
    pub keywords: &'static [&'static str],
}

static TEMPLATES: &[TemplateMatch] = &[
    TemplateMatch {
        name: "research_report",
        keywords: &["research", "investigate", "find out", "look into", "survey"],
    },
    TemplateMatch {
        name: "code_feature",
        keywords: &["implement", "build a", "create a function", "write code", "code that"],
    },
    TemplateMatch {
        name: "bulk_classify",
        keywords: &["classify", "categorize", "label each", "for each item", "batch"],
    },
];

/// Returns Some(template_name) if the goal strongly matches a template.
pub fn detect_template(goal: &str) -> Option<&'static str> {
    let lower = goal.to_lowercase();
    for tmpl in TEMPLATES {
        if tmpl.keywords.iter().any(|kw| lower.contains(kw)) {
            return Some(tmpl.name);
        }
    }
    None
}

/// Build a research + report TaskGraph:
///   1. Gather background information on the topic
///   2. Identify key facts / data points
///   3. Write the final report synthesizing findings
pub fn research_report(goal: &str, run_id: RunId) -> TaskGraph {
    let t1 = TaskId::new();
    let t2 = TaskId::new();
    let t3 = TaskId::new();

    TaskGraph {
        run_id,
        goal: goal.to_string(),
        nodes: vec![
            TaskNode {
                id: t1.clone(),
                instruction: format!(
                    "Research and gather background information for the following goal. \
                     List key facts, sources, and relevant data points.\n\nGoal: {goal}"
                ),
                depends_on: vec![],
                output_type: OutputType::Text,
                stakes: Stakes::Medium,
                skill_hints: vec!["factuality".into(), "reasoning".into()],
                max_context_tokens: None,
            },
            TaskNode {
                id: t2.clone(),
                instruction: format!(
                    "Analyze and structure the gathered research for:\n\nGoal: {goal}\n\n\
                     Identify the most important insights and organize them logically."
                ),
                depends_on: vec![t1.clone()],
                output_type: OutputType::Text,
                stakes: Stakes::Medium,
                skill_hints: vec!["reasoning".into()],
                max_context_tokens: None,
            },
            TaskNode {
                id: t3,
                instruction: format!(
                    "Write a clear, well-structured final report for:\n\nGoal: {goal}\n\n\
                     The report should be concise, accurate, and actionable."
                ),
                depends_on: vec![t1, t2],
                output_type: OutputType::Text,
                stakes: Stakes::Medium,
                skill_hints: vec!["writing".into()],
                max_context_tokens: None,
            },
        ],
        created_at: Utc::now(),
    }
}

/// Build a code feature TaskGraph:
///   1. Plan the implementation approach
///   2. Write the core implementation
///   3. Write tests
///   4. Write documentation/docstrings
pub fn code_feature(goal: &str, run_id: RunId) -> TaskGraph {
    let t1 = TaskId::new();
    let t2 = TaskId::new();
    let t3 = TaskId::new();
    let t4 = TaskId::new();

    TaskGraph {
        run_id,
        goal: goal.to_string(),
        nodes: vec![
            TaskNode {
                id: t1.clone(),
                instruction: format!(
                    "Plan the implementation for the following coding task. \
                     Describe the approach, key functions/classes, data structures, and edge cases.\n\n\
                     Task: {goal}"
                ),
                depends_on: vec![],
                output_type: OutputType::Text,
                stakes: Stakes::Medium,
                skill_hints: vec!["coding".into(), "reasoning".into()],
                max_context_tokens: None,
            },
            TaskNode {
                id: t2.clone(),
                instruction: format!(
                    "Implement the core solution for:\n\nTask: {goal}\n\n\
                     Write clean, production-quality code following the plan."
                ),
                depends_on: vec![t1.clone()],
                output_type: OutputType::Code { language: "auto".into() },
                stakes: Stakes::High,
                skill_hints: vec!["coding".into()],
                max_context_tokens: None,
            },
            TaskNode {
                id: t3.clone(),
                instruction: format!(
                    "Write comprehensive unit tests for the implementation of:\n\n\
                     Task: {goal}\n\nCover happy paths, edge cases, and error conditions."
                ),
                depends_on: vec![t2.clone()],
                output_type: OutputType::Code { language: "auto".into() },
                stakes: Stakes::Medium,
                skill_hints: vec!["coding".into()],
                max_context_tokens: None,
            },
            TaskNode {
                id: t4,
                instruction: format!(
                    "Write clear documentation/docstrings for the implementation of:\n\n\
                     Task: {goal}\n\nInclude usage examples and parameter descriptions."
                ),
                depends_on: vec![t2],
                output_type: OutputType::Text,
                stakes: Stakes::Low,
                skill_hints: vec!["writing".into(), "coding".into()],
                max_context_tokens: None,
            },
        ],
        created_at: Utc::now(),
    }
}

/// Build a template TaskGraph from a detected template name.
pub fn build_from_template(name: &str, goal: &str, run_id: RunId) -> Option<TaskGraph> {
    match name {
        "research_report" => Some(research_report(goal, run_id)),
        "code_feature" => Some(code_feature(goal, run_id)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_research() {
        assert_eq!(detect_template("research the history of Rust"), Some("research_report"));
    }

    #[test]
    fn detects_code_feature() {
        assert_eq!(detect_template("implement a binary search tree in Rust"), Some("code_feature"));
    }

    #[test]
    fn no_match_for_generic_query() {
        assert_eq!(detect_template("what is the capital of France"), None);
    }

    #[test]
    fn research_report_graph_has_three_nodes() {
        let g = research_report("test goal", RunId::new());
        assert_eq!(g.nodes.len(), 3);
        // Last node depends on the first two
        assert_eq!(g.nodes[2].depends_on.len(), 2);
    }

    #[test]
    fn code_feature_graph_has_four_nodes() {
        let g = code_feature("test goal", RunId::new());
        assert_eq!(g.nodes.len(), 4);
    }
}
