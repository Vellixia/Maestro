use bollard::container::{Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::Docker;
use futures_util::StreamExt;
use futures_util::TryStreamExt;
use tracing::debug;

use core_types::VerifyResult;

/// Execute a code snippet in a Docker sandbox to verify it compiles/runs.
/// Falls back to structural syntax checks if Docker is unavailable.
pub async fn verify_code(code: &str, language: &str) -> VerifyResult {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            debug!("Docker unavailable, falling back to syntax check: {e}");
            return syntax_check(code);
        }
    };

    let image = match language {
        "python" | "py" => "python:3.12-alpine",
        "javascript" | "js" => "node:22-alpine",
        "typescript" | "ts" => "node:22-alpine",
        "rust" | "rs" => "rust:1.85-alpine",
        "go" => "golang:1.23-alpine",
        "ruby" | "rb" => "ruby:3.3-alpine",
        _ => return syntax_check(code),
    };

    // Best-effort pull (image may be cached).
    let _ = docker
        .create_image(
            Some(CreateImageOptions {
                from_image: image,
                tag: "latest",
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await;

    let container = docker
        .create_container(
            Some(CreateContainerOptions {
                name: "",
                platform: None,
            }),
            Config {
                image: Some(image),
                cmd: Some(vec!["cat"]),
                attach_stdin: Some(false),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                network_disabled: Some(true),
                ..Default::default()
            },
        )
        .await;

    let container_id = match container {
        Ok(c) => c.id,
        Err(e) => {
            debug!("Docker container creation failed: {e}");
            return syntax_check(code);
        }
    };

    // Start the container.
    if let Err(e) = docker
        .start_container(&container_id, None::<StartContainerOptions<String>>)
        .await
    {
        debug!("Failed to start container: {e}");
        let _ = docker.remove_container(&container_id, None::<RemoveContainerOptions>).await;
        return syntax_check(code);
    }

    // Run code via exec.
    let exec_cmd = match language {
        "python" | "py" => vec!["python3", "-c", code],
        "javascript" | "js" => vec!["node", "-e", code],
        "typescript" | "ts" => vec!["npx", "--yes", "tsx", "-e", code],
        "rust" | "rs" => vec!["sh", "-c", "cat > /tmp/test.rs && rustc -o /tmp/test /tmp/test.rs 2>&1 && /tmp/test"],
        "go" => vec!["sh", "-c", "cat > /tmp/test.go && go run /tmp/test.go 2>&1"],
        "ruby" | "rb" => vec!["ruby", "-e", code],
        _ => return syntax_check(code),
    };

    let exec = docker
        .create_exec(
            &container_id,
            CreateExecOptions {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(exec_cmd),
                ..Default::default()
            },
        )
        .await;

    let exec_id = match exec {
        Ok(e) => e.id,
        Err(e) => {
            debug!("Failed to create exec: {e}");
            let _ = docker.remove_container(&container_id, None::<RemoveContainerOptions>).await;
            return syntax_check(code);
        }
    };

    let mut exec_output = String::new();
    let mut had_error = false;

    match docker.start_exec(&exec_id, None).await {
        Ok(StartExecResults::Attached { mut output, input: _ }) => {
            while let Some(chunk) = output.next().await {
                match chunk {
                    Ok(log_output) => {
                        let bytes = log_output.into_bytes();
                        let s = String::from_utf8_lossy(&bytes);
                        exec_output.push_str(&s);
                    }
                    Err(e) => {
                        debug!("Exec log error: {e}");
                    }
                }
            }
        }
        Ok(StartExecResults::Detached) => {
            debug!("Exec detached (unexpected)");
            had_error = true;
        }
        Err(e) => {
            debug!("Exec stream error: {e}");
            had_error = true;
        }
    }

    // Best-effort cleanup.
    let _ = docker
        .remove_container(
            &container_id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await;

    if had_error && exec_output.is_empty() {
        return syntax_check(code);
    }

    // Check if output contains error indicators.
    let lower = exec_output.to_lowercase();
    if lower.contains("error") || lower.contains("panic") || lower.contains("traceback") {
        return VerifyResult::Failed {
            reason: format!("code execution error: {}", exec_output.trim()),
        };
    }

    VerifyResult::Passed
}

fn syntax_check(code: &str) -> VerifyResult {
    if code.trim().is_empty() {
        return VerifyResult::Failed { reason: "empty code response".into() };
    }
    if !balanced(code) {
        return VerifyResult::Failed { reason: "unbalanced braces/brackets in code".into() };
    }
    VerifyResult::Passed
}

fn balanced(code: &str) -> bool {
    let mut depth: i32 = 0;
    for ch in code.chars() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}
