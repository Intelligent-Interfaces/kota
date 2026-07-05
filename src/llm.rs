use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Client for an OpenAI-compatible local LLM API (Ollama, llama-server, etc.)
pub struct LlmClient {
    client: Client,
    base_url: String,
    model: String,
}

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// A chunk from the streaming response
#[derive(Debug, Deserialize)]
pub struct StreamChunk {
    pub choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
pub struct StreamChoice {
    pub delta: StreamDelta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StreamDelta {
    pub content: Option<String>,
    pub tool_calls: Option<Vec<StreamToolCall>>,
    /// Some models (Qwen, DeepSeek) emit reasoning in a separate field
    pub reasoning_content: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct StreamToolCall {
    pub index: Option<usize>,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<StreamToolCallFunction>,
}

#[derive(Debug, Deserialize)]
pub struct StreamToolCallFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

/// Accumulated result from streaming
pub enum StreamEvent {
    /// Regular content token
    Content(String),
    /// Reasoning/thinking token
    Thinking(String),
    /// A complete tool call (accumulated from chunks)
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    /// Stream finished
    Done,
}

impl LlmClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    /// Stream a chat completion, yielding events as they arrive.
    /// The callback is called for each event.
    pub async fn chat_stream<F>(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDef>>,
        mut on_event: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(StreamEvent) + Send,
    {
        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
            tools,
            temperature: Some(0.2),
        };

        let url = format!("{}/chat/completions", self.base_url);
        let mut req = self.client.post(&url).json(&request);

        if let Ok(key) = std::env::var("KOTA_API_KEY") {
            req = req.bearer_auth(key);
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            req = req.bearer_auth(key);
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {}: {}", status, body);
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        // Accumulators for tool calls
        let mut tool_id = String::new();
        let mut tool_name = String::new();
        let mut tool_args = String::new();
        let mut in_tool_call = false;

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            // SSE format: lines starting with "data: "
            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() || !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    // Flush any pending tool call
                    if in_tool_call && !tool_name.is_empty() {
                        on_event(StreamEvent::ToolCall {
                            id: tool_id.clone(),
                            name: tool_name.clone(),
                            arguments: tool_args.clone(),
                        });
                    }
                    on_event(StreamEvent::Done);
                    return Ok(());
                }

                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                    for choice in &chunk.choices {
                        // Content tokens
                        if let Some(ref content) = choice.delta.content {
                            if !content.is_empty() {
                                on_event(StreamEvent::Content(content.clone()));
                            }
                        }

                        // Thinking/reasoning tokens
                        if let Some(ref thinking) = choice.delta.reasoning_content {
                            if !thinking.is_empty() {
                                on_event(StreamEvent::Thinking(thinking.clone()));
                            }
                        }

                        // Tool calls (accumulated across chunks)
                        if let Some(ref calls) = choice.delta.tool_calls {
                            for tc in calls {
                                if let Some(ref id) = tc.id {
                                    // New tool call starting
                                    if in_tool_call && !tool_name.is_empty() {
                                        on_event(StreamEvent::ToolCall {
                                            id: tool_id.clone(),
                                            name: tool_name.clone(),
                                            arguments: tool_args.clone(),
                                        });
                                    }
                                    tool_id = id.clone();
                                    tool_name.clear();
                                    tool_args.clear();
                                    in_tool_call = true;
                                }
                                if let Some(ref func) = tc.function {
                                    if let Some(ref name) = func.name {
                                        tool_name.push_str(name);
                                    }
                                    if let Some(ref args) = func.arguments {
                                        tool_args.push_str(args);
                                    }
                                }
                            }
                        }

                        // Check finish reason
                        if let Some(ref reason) = choice.finish_reason {
                            if reason == "tool_calls" && in_tool_call && !tool_name.is_empty() {
                                on_event(StreamEvent::ToolCall {
                                    id: tool_id.clone(),
                                    name: tool_name.clone(),
                                    arguments: tool_args.clone(),
                                });
                                tool_id.clear();
                                tool_name.clear();
                                tool_args.clear();
                                in_tool_call = false;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
