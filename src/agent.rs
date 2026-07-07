use crate::events::AgentEvent;
use crate::llm::{LlmClient, Message};
use crate::skills::SkillComposer;
use crate::tools;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMode {
    Coder,
    Cpe,       // Client Platform Engineering
    Eval,      // Safety Evaluation
    Research,  // Literature Review & Writing
    Architect, // System Design & Infrastructure
    Librarian, // LLM Wiki Maintenance & Knowledge Compiling
}

impl AgentMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cpe" => AgentMode::Cpe,
            "eval" => AgentMode::Eval,
            "research" => AgentMode::Research,
            "architect" => AgentMode::Architect,
            "librarian" => AgentMode::Librarian,
            _ => AgentMode::Coder,
        }
    }

    pub fn to_str(self) -> &'static str {
        match self {
            AgentMode::Coder => "coder",
            AgentMode::Cpe => "cpe",
            AgentMode::Eval => "eval",
            AgentMode::Research => "research",
            AgentMode::Architect => "architect",
            AgentMode::Librarian => "librarian",
        }
    }

    pub fn system_prompt(self) -> String {
        let composer = SkillComposer::new(".kota_skills");
        let mut weights = HashMap::new();

        match self {
            AgentMode::Coder => {
                weights.insert("coder", 1.0);
                weights.insert("eval", 0.2); // Option keyboard: mix in evaluation skills
            }
            AgentMode::Cpe => {
                weights.insert("cpe", 1.0);
                weights.insert("architect", 0.4);
            }
            AgentMode::Eval => {
                weights.insert("eval", 1.0);
            }
            AgentMode::Research => {
                weights.insert("research", 1.0);
                weights.insert("coder", 0.3); // Mix in coding for empirical research
            }
            AgentMode::Architect => {
                weights.insert("architect", 1.0);
                weights.insert("cpe", 0.5);
            }
            AgentMode::Librarian => {
                weights.insert("librarian", 1.0);
                weights.insert("research", 0.4); // Mix in research skills for synthesis
            }
        };

        composer.compose(&weights)
    }
}

use crate::memory::MemoryStore;

pub struct Agent {
    llm: LlmClient,
    messages: Vec<Message>,
    max_tokens: usize,
    workdir: PathBuf,
    step: usize,
    mode: AgentMode,
    memory: MemoryStore,
    session_id: String,
    mcp: Option<crate::mcp::McpManager>,
}

impl Agent {
    pub async fn new(
        llm: LlmClient,
        max_tokens: usize,
        workdir: &str,
        mode: AgentMode,
    ) -> anyhow::Result<Self> {
        let memory = MemoryStore::new(".kota_memory.db").await?;
        let session_id = uuid::Uuid::new_v4().to_string();

        memory.save_conversation(&session_id, mode.to_str()).await?;

        let sys_prompt = mode.system_prompt();
        let sys_msg = Message {
            role: "system".to_string(),
            content: Some(sys_prompt.clone()),
            tool_calls: None,
            tool_call_id: None,
        };

        memory
            .save_message(&session_id, "system", &sys_prompt)
            .await?;

        let messages = vec![sys_msg];
        let workdir_path = PathBuf::from(workdir);
        let mcp_config_path = workdir_path.join("mcp_config.json");
        let mcp = crate::mcp::McpManager::load_from_config(mcp_config_path)
            .await
            .ok();

        Ok(Self {
            llm,
            messages,
            max_tokens,
            workdir: workdir_path,
            step: 0,
            mode,
            memory,
            session_id,
            mcp,
        })
    }

    pub fn set_mode(&mut self, mode: AgentMode) {
        self.mode = mode;
        if let Some(sys_msg) = self.messages.first_mut() {
            if sys_msg.role == "system" {
                sys_msg.content = Some(mode.system_prompt());
            }
        }
    }

    /// Process a user message and stream events back through the channel.
    /// The agent loop handles tool calls automatically.
    pub async fn process(
        &mut self,
        user_input: &str,
        tx: broadcast::Sender<AgentEvent>,
    ) -> anyhow::Result<()> {
        // Save to Turso DB
        self.memory
            .save_message(&self.session_id, "user", user_input)
            .await?;

        // Episodic Memory Retrieval (Mental Time Travel)
        // Extract a salient keyword (longest word) to retrieve distant memory context
        let keyword = user_input
            .split_whitespace()
            .max_by_key(|w| w.len())
            .unwrap_or("");
        if keyword.len() > 4 {
            if let Ok(memories) = self.memory.query_episodic_memory(keyword, 2).await {
                if !memories.is_empty() {
                    let episodic_context = format!(
                        "EPISODIC MEMORY RECALL (Past context regarding '{}'):\n{}",
                        keyword,
                        memories.join("\n---\n")
                    );
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: Some(episodic_context),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }
            }
        }

        // Add user message to local cache
        self.messages.push(Message {
            role: "user".to_string(),
            content: Some(user_input.to_string()),
            tool_calls: None,
            tool_call_id: None,
        });

        // Agent loop: keep going until the model responds without a tool call
        loop {
            self.step += 1;
            let step = self.step;
            let start = Instant::now();

            // Rough token estimate (4 chars per token)
            let approx_tokens: usize = self
                .messages
                .iter()
                .map(|m| m.content.as_ref().map_or(0, |c| c.len() / 4))
                .sum();

            if approx_tokens > self.max_tokens * 3 / 4 {
                let _ = tx.send(AgentEvent::BudgetWarning {
                    used: approx_tokens,
                    max: self.max_tokens,
                });
            }

            let _ = tx.send(AgentEvent::StepStarted {
                step,
                tokens_in: approx_tokens,
            });

            // Collect the streamed response
            let content_buf = Arc::new(Mutex::new(String::new()));
            let pending_tool_calls = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
            let tx_clone = tx.clone();

            let messages = self.messages.clone();
            let tool_defs = tools::tool_definitions(self.mcp.as_ref()).await;

            let tc_clone = pending_tool_calls.clone();
            let cb_clone = content_buf.clone();

            self.llm
                .chat_stream(messages, Some(tool_defs), move |event| match event {
                    crate::llm::StreamEvent::Content(text) => {
                        cb_clone.lock().unwrap().push_str(&text);
                        let _ = tx_clone.send(AgentEvent::Token { text });
                    }
                    crate::llm::StreamEvent::Thinking(text) => {
                        let _ = tx_clone.send(AgentEvent::Thinking { text });
                    }
                    crate::llm::StreamEvent::ToolCall {
                        id,
                        name,
                        arguments,
                    } => {
                        tc_clone.lock().unwrap().push((id, name, arguments));
                    }
                    crate::llm::StreamEvent::Done => {}
                })
                .await?;

            let pending_tool_calls = pending_tool_calls.lock().unwrap().clone();
            let content_buf = content_buf.lock().unwrap().clone();

            // If there were tool calls, execute them and loop back
            if !pending_tool_calls.is_empty() {
                // Add assistant message with tool calls
                let tool_call_responses: Vec<crate::llm::ToolCallResponse> = pending_tool_calls
                    .iter()
                    .map(|(id, name, args)| crate::llm::ToolCallResponse {
                        id: id.clone(),
                        call_type: "function".to_string(),
                        function: crate::llm::ToolCallFunction {
                            name: name.clone(),
                            arguments: args.clone(),
                        },
                    })
                    .collect();

                let content_str = if content_buf.is_empty() {
                    serde_json::to_string(&tool_call_responses).unwrap_or_default()
                } else {
                    format!(
                        "{}\nTool Calls: {}",
                        content_buf,
                        serde_json::to_string(&tool_call_responses).unwrap_or_default()
                    )
                };
                let _ = self
                    .memory
                    .save_message(&self.session_id, "assistant", &content_str)
                    .await;

                self.messages.push(Message {
                    role: "assistant".to_string(),
                    content: if content_buf.is_empty() {
                        None
                    } else {
                        Some(content_buf.clone())
                    },
                    tool_calls: Some(tool_call_responses),
                    tool_call_id: None,
                });

                // Execute each tool call
                for (id, name, args) in &pending_tool_calls {
                    let _ = tx.send(AgentEvent::ToolCallStarted {
                        step,
                        tool: name.clone(),
                        args: serde_json::from_str(args).unwrap_or(serde_json::Value::Null),
                    });

                    let tool_start = Instant::now();
                    let mut has_mcp_tool = false;
                    if let Some(ref m) = self.mcp {
                        if m.has_tool(name).await {
                            has_mcp_tool = true;
                        }
                    }

                    let result = if name == "delegate_task" {
                        match serde_json::from_str::<serde_json::Value>(args) {
                            Ok(args_val) => {
                                let task_desc = args_val["task"].as_str().unwrap_or("");
                                let mode_str = args_val["mode"].as_str().unwrap_or("coder");
                                let sub_mode = AgentMode::from_str(mode_str);
                                let sub_llm = self.llm.clone();
                                match Agent::new(
                                    sub_llm,
                                    self.max_tokens,
                                    &self.workdir.to_string_lossy(),
                                    sub_mode,
                                )
                                .await
                                {
                                    Ok(mut sub_agent) => {
                                        let (sub_tx, mut sub_rx) = broadcast::channel(100);
                                        let _ = tx.send(AgentEvent::Token {
                                            text: format!("\n[Spawning sub-agent in mode: {} to run task...]\n", mode_str)
                                        });
                                        let task_desc_clone = task_desc.to_string();
                                        let tx_clone = tx.clone();
                                        let mode_clone = mode_str.to_string();
                                        tokio::spawn(async move {
                                            while let Ok(event) = sub_rx.recv().await {
                                                if let AgentEvent::Token { text } = event {
                                                    let _ = tx_clone.send(AgentEvent::Token {
                                                        text: format!(
                                                            "Sub[{}]> {}",
                                                            mode_clone, text
                                                        ),
                                                    });
                                                }
                                            }
                                        });
                                        match Box::pin(sub_agent.process(&task_desc_clone, sub_tx))
                                            .await
                                        {
                                            Ok(()) => {
                                                let final_response = sub_agent
                                                    .messages
                                                    .iter()
                                                    .rfind(|m| {
                                                        m.role == "assistant" && m.content.is_some()
                                                    })
                                                    .and_then(|m| m.content.clone())
                                                    .unwrap_or_else(|| {
                                                        "Subagent finished with no response."
                                                            .to_string()
                                                    });
                                                tools::ToolResult {
                                                    success: true,
                                                    output: final_response,
                                                }
                                            }
                                            Err(e) => tools::ToolResult {
                                                success: false,
                                                output: format!("Subagent failed: {}", e),
                                            },
                                        }
                                    }
                                    Err(e) => tools::ToolResult {
                                        success: false,
                                        output: format!("Failed to create subagent: {}", e),
                                    },
                                }
                            }
                            Err(e) => tools::ToolResult {
                                success: false,
                                output: format!("Failed to parse delegate_task arguments: {}", e),
                            },
                        }
                    } else if has_mcp_tool {
                        let args_val: serde_json::Value =
                            serde_json::from_str(args).unwrap_or(serde_json::Value::Null);
                        match self.mcp.as_ref().unwrap().call_tool(name, args_val).await {
                            Ok(output) => tools::ToolResult {
                                success: true,
                                output,
                            },
                            Err(e) => tools::ToolResult {
                                success: false,
                                output: format!("MCP tool call failed: {}", e),
                            },
                        }
                    } else {
                        match tools::parse_tool_call(name, args, &self.workdir) {
                            Ok(call) => tools::execute(&call).await,
                            Err(e) => tools::ToolResult {
                                success: false,
                                output: format!("Failed to parse tool call: {}", e),
                            },
                        }
                    };
                    let duration = tool_start.elapsed().as_millis() as u64;

                    let preview = if result.output.len() > 200 {
                        format!("{}...", &result.output[..200])
                    } else {
                        result.output.clone()
                    };

                    let _ = tx.send(AgentEvent::ToolCallFinished {
                        step,
                        tool: name.clone(),
                        duration_ms: duration,
                        success: result.success,
                        result_preview: preview,
                    });

                    // Add tool result to conversation
                    let _ = self
                        .memory
                        .save_message(&self.session_id, "tool", &result.output)
                        .await;

                    self.messages.push(Message {
                        role: "tool".to_string(),
                        content: Some(result.output),
                        tool_calls: None,
                        tool_call_id: Some(id.clone()),
                    });
                }

                // Loop back for the model to process tool results
                continue;
            }

            // No tool calls — we're done
            if !content_buf.is_empty() {
                let _ = self
                    .memory
                    .save_message(&self.session_id, "assistant", &content_buf)
                    .await;

                self.messages.push(Message {
                    role: "assistant".to_string(),
                    content: Some(content_buf),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }

            let duration = start.elapsed().as_millis() as u64;
            let _ = tx.send(AgentEvent::Done {
                step,
                total_tokens: approx_tokens,
                duration_ms: duration,
            });

            break;
        }

        Ok(())
    }

    /// Reset conversation history (keep system prompt)
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.messages.truncate(1);
        self.step = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_mode_parsing() {
        assert_eq!(AgentMode::from_str("cpe"), AgentMode::Cpe);
        assert_eq!(AgentMode::from_str("EVAL"), AgentMode::Eval);
        assert_eq!(AgentMode::from_str("research"), AgentMode::Research);
        assert_eq!(AgentMode::from_str("architect"), AgentMode::Architect);
        assert_eq!(AgentMode::from_str("librarian"), AgentMode::Librarian);
        assert_eq!(AgentMode::from_str("unknown"), AgentMode::Coder);
    }

    #[test]
    fn test_mode_prompts_contain_fields() {
        let cpe = AgentMode::Cpe.system_prompt();
        assert!(cpe.contains("launchd"));
        assert!(cpe.contains("osqueryi"));

        let research = AgentMode::Research.system_prompt();
        assert!(research.contains("Statistical Physics"));
        assert!(research.contains("Psycholinguistics"));
        assert!(research.contains("Quantum Computing"));
    }
}
