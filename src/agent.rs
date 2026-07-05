use crate::events::AgentEvent;
use crate::llm::{LlmClient, Message};
use crate::tools;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMode {
    Coder,
    Cpe,      // Client Platform Engineering
    Eval,     // Safety Evaluation
    Research, // Literature Review & Writing
    Architect,// System Design & Infrastructure
}

impl AgentMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cpe" => AgentMode::Cpe,
            "eval" => AgentMode::Eval,
            "research" => AgentMode::Research,
            "architect" => AgentMode::Architect,
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
        }
    }

    pub fn system_prompt(self) -> String {
        let skills_dir = PathBuf::from(".kota_skills");
        
        let base_path = skills_dir.join("base.md");
        let base = std::fs::read_to_string(base_path).unwrap_or_else(|_| "You are Kota.".to_string());

        let mode_file = match self {
            AgentMode::Coder => "coder.md",
            AgentMode::Cpe => "cpe.md",
            AgentMode::Eval => "eval.md",
            AgentMode::Research => "research.md",
            AgentMode::Architect => "architect.md",
        };

        let mode_path = skills_dir.join(mode_file);
        let mode_prompt = std::fs::read_to_string(mode_path).unwrap_or_else(|_| "".to_string());

        format!("{}\n{}", base, mode_prompt)
    }
}

pub struct Agent {
    llm: LlmClient,
    messages: Vec<Message>,
    max_tokens: usize,
    workdir: PathBuf,
    step: usize,
    mode: AgentMode,
}

impl Agent {
    pub fn new(llm: LlmClient, max_tokens: usize, workdir: &str, mode: AgentMode) -> Self {
        let messages = vec![Message {
            role: "system".to_string(),
            content: Some(mode.system_prompt()),
            tool_calls: None,
            tool_call_id: None,
        }];

        Self {
            llm,
            messages,
            max_tokens,
            workdir: PathBuf::from(workdir),
            step: 0,
            mode,
        }
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
        // Add user message
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
            let tool_defs = tools::tool_definitions();
            
            let tc_clone = pending_tool_calls.clone();
            let cb_clone = content_buf.clone();

            self.llm
                .chat_stream(
                    messages,
                    Some(tool_defs),
                    move |event| match event {
                        crate::llm::StreamEvent::Content(text) => {
                            cb_clone.lock().unwrap().push_str(&text);
                            let _ = tx_clone.send(AgentEvent::Token { text });
                        }
                        crate::llm::StreamEvent::Thinking(text) => {
                            let _ = tx_clone.send(AgentEvent::Thinking { text });
                        }
                        crate::llm::StreamEvent::ToolCall { id, name, arguments } => {
                            tc_clone.lock().unwrap().push((id, name, arguments));
                        }
                        crate::llm::StreamEvent::Done => {}
                    },
                )
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
                    let result = match tools::parse_tool_call(name, args, &self.workdir) {
                        Ok(call) => tools::execute(&call).await,
                        Err(e) => tools::ToolResult {
                            success: false,
                            output: format!("Failed to parse tool call: {}", e),
                        },
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
