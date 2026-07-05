use crate::llm::{ToolDef, ToolFunction};

use std::path::{Path, PathBuf};
use tokio::process::Command;

/// The set of tools the agent can use.
/// Each variant maps to a tool the model can call.
#[derive(Debug)]
pub enum ToolCall {
    ReadFile { path: PathBuf },
    WriteFile { path: PathBuf, content: String },
    ListDir { path: PathBuf },
    RunCommand { command: String },
    Search { pattern: String, path: PathBuf },
    FetchNews { query: String },
}

#[derive(Debug)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
}

/// Parse a tool call from the model's JSON arguments
pub fn parse_tool_call(name: &str, args_json: &str, workdir: &Path) -> anyhow::Result<ToolCall> {
    let args: serde_json::Value = serde_json::from_str(args_json)
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));

    match name {
        "read_file" => {
            let path = args["path"].as_str().unwrap_or("");
            Ok(ToolCall::ReadFile {
                path: workdir.join(path),
            })
        }
        "write_file" => {
            let path = args["path"].as_str().unwrap_or("");
            let content = args["content"].as_str().unwrap_or("");
            Ok(ToolCall::WriteFile {
                path: workdir.join(path),
                content: content.to_string(),
            })
        }
        "list_dir" => {
            let path = args["path"].as_str().unwrap_or(".");
            Ok(ToolCall::ListDir {
                path: workdir.join(path),
            })
        }
        "run_command" => {
            let command = args["command"].as_str().unwrap_or("");
            Ok(ToolCall::RunCommand {
                command: command.to_string(),
            })
        }
        "search" => {
            let pattern = args["pattern"].as_str().unwrap_or("");
            let path = args["path"].as_str().unwrap_or(".");
            Ok(ToolCall::Search {
                pattern: pattern.to_string(),
                path: workdir.join(path),
            })
        }
        "fetch_news" => {
            let query = args["query"].as_str().unwrap_or("");
            Ok(ToolCall::FetchNews {
                query: query.to_string(),
            })
        }
        _ => anyhow::bail!("Unknown tool: {}", name),
    }
}

/// Execute a tool call and return the result
pub async fn execute(call: &ToolCall) -> ToolResult {
    match call {
        ToolCall::ReadFile { path } => {
            match tokio::fs::read_to_string(path).await {
                Ok(content) => {
                    // Truncate large files
                    let preview = if content.len() > 8000 {
                        format!(
                            "{}...\n[truncated, {} total bytes]",
                            &content[..8000],
                            content.len()
                        )
                    } else {
                        content
                    };
                    ToolResult {
                        success: true,
                        output: preview,
                    }
                }
                Err(e) => ToolResult {
                    success: false,
                    output: format!("Error reading {}: {}", path.display(), e),
                },
            }
        }

        ToolCall::WriteFile { path, content } => {
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            match tokio::fs::write(path, content).await {
                Ok(()) => ToolResult {
                    success: true,
                    output: format!("Wrote {} bytes to {}", content.len(), path.display()),
                },
                Err(e) => ToolResult {
                    success: false,
                    output: format!("Error writing {}: {}", path.display(), e),
                },
            }
        }

        ToolCall::ListDir { path } => match tokio::fs::read_dir(path).await {
            Ok(mut entries) => {
                let mut listing = Vec::new();
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let meta = entry.metadata().await.ok();
                    let kind = if meta.as_ref().is_some_and(|m| m.is_dir()) {
                        "dir"
                    } else {
                        "file"
                    };
                    listing.push(format!("{:4} {}", kind, name));
                }
                listing.sort();
                ToolResult {
                    success: true,
                    output: listing.join("\n"),
                }
            }
            Err(e) => ToolResult {
                success: false,
                output: format!("Error listing {}: {}", path.display(), e),
            },
        },

        ToolCall::RunCommand { command } => {
            let result = Command::new("bash").arg("-c").arg(command).output().await;

            match result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let combined = if stderr.is_empty() {
                        stdout.to_string()
                    } else {
                        format!("{}\nSTDERR:\n{}", stdout, stderr)
                    };
                    // Truncate long output
                    let truncated = if combined.len() > 4000 {
                        format!("{}...\n[truncated]", &combined[..4000])
                    } else {
                        combined
                    };
                    ToolResult {
                        success: output.status.success(),
                        output: truncated,
                    }
                }
                Err(e) => ToolResult {
                    success: false,
                    output: format!("Error running command: {}", e),
                },
            }
        }

        ToolCall::Search { pattern, path } => {
            // Use grep for search
            let result = Command::new("grep")
                .args(["-rn", "--include=*", pattern, &path.to_string_lossy()])
                .output()
                .await;

            match result {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let truncated = if stdout.len() > 4000 {
                        format!("{}...\n[truncated]", &stdout[..4000])
                    } else {
                        stdout.to_string()
                    };
                    ToolResult {
                        success: true,
                        output: if truncated.is_empty() {
                            "No matches found.".to_string()
                        } else {
                            truncated
                        },
                    }
                }
                Err(e) => ToolResult {
                    success: false,
                    output: format!("Error searching: {}", e),
                },
            }
        }

        ToolCall::FetchNews { query } => match fetch_arxiv(query).await {
            Ok(content) => ToolResult {
                success: true,
                output: content,
            },
            Err(e) => ToolResult {
                success: false,
                output: format!("Error fetching news: {}", e),
            },
        },
    }
}

/// Return the tool definitions for the OpenAI-compatible API
pub fn tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "read_file".to_string(),
                description: "Read the contents of a file".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file"
                        }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "write_file".to_string(),
                description: "Write content to a file (creates or overwrites)".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to the file"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write"
                        }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "list_dir".to_string(),
                description: "List files and directories in a path".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Relative path to list (default: current directory)"
                        }
                    },
                    "required": []
                }),
            },
        },
        ToolDef {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "run_command".to_string(),
                description: "Run a shell command and return its output".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The shell command to execute"
                        }
                    },
                    "required": ["command"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "search".to_string(),
                description: "Search for a pattern in files (grep)".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "The regex pattern to search for"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory to search in (default: current directory)"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
        },
        ToolDef {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "fetch_news".to_string(),
                description:
                    "Query arXiv to retrieve the latest research papers and summaries for a topic"
                        .to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The research query (e.g. 'psycholinguistics' or 'quantum computing')"
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
    ]
}

async fn fetch_arxiv(query: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let encoded_query = query.replace(" ", "+");
    let url = format!(
        "http://export.arxiv.org/api/query?search_query=all:{}&max_results=3",
        encoded_query
    );
    let res = client.get(&url).send().await?.text().await?;

    let mut results = Vec::new();
    for entry in res.split("<entry>") {
        if !entry.contains("</entry>") {
            continue;
        }
        let title = extract_tag(entry, "title").unwrap_or_else(|_| "No Title".to_string());
        let summary = extract_tag(entry, "summary").unwrap_or_else(|_| "No Summary".to_string());
        let author = extract_tag(entry, "author")
            .and_then(|a| extract_tag(&a, "name"))
            .unwrap_or_else(|_| "Unknown".to_string());

        results.push(format!(
            "Title: {}\nAuthor: {}\nSummary: {}\n",
            title.trim(),
            author.trim(),
            summary.trim()
        ));
    }

    if results.is_empty() {
        Ok("No papers found on arXiv.".to_string())
    } else {
        Ok(results.join("\n---\n\n"))
    }
}

fn extract_tag(source: &str, tag: &str) -> anyhow::Result<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    if let Some(start_idx) = source.find(&start_tag) {
        if let Some(end_idx) = source.find(&end_tag) {
            let content = &source[start_idx + start_tag.len()..end_idx];
            return Ok(content.to_string());
        }
    }
    anyhow::bail!("Tag not found: {}", tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tag() {
        let xml = "<entry><title>Test Title</title><summary>Test Abstract</summary></entry>";
        let title = extract_tag(xml, "title").unwrap();
        assert_eq!(title, "Test Title");
        let summary = extract_tag(xml, "summary").unwrap();
        assert_eq!(summary, "Test Abstract");
    }

    #[test]
    fn test_extract_tag_missing() {
        let xml = "<entry><summary>Test Abstract</summary></entry>";
        let res = extract_tag(xml, "title");
        assert!(res.is_err());
    }
}
