use crate::llm::{Message, StreamEvent, ToolDef};
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::process::Command;

#[derive(Clone)]
pub struct VertexClient {
    client: Client,
    project_id: String,
    region: String,
    model: String,
}

impl VertexClient {
    pub fn new(project_id: &str, region: &str, model: &str) -> Self {
        Self {
            client: Client::new(),
            project_id: project_id.to_string(),
            region: region.to_string(),
            model: model.to_string(),
        }
    }

    fn get_token() -> String {
        if let Ok(token) = std::env::var("GCP_ACCESS_TOKEN") {
            return token;
        }
        let output = Command::new("gcloud")
            .args(["auth", "print-access-token"])
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout).trim().to_string();
            }
        }
        String::new()
    }

    pub async fn chat_stream<F>(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDef>>,
        mut on_event: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(StreamEvent) + Send,
    {
        let token = Self::get_token();
        if token.is_empty() {
            anyhow::bail!("Failed to get GCP access token. Ensure gcloud is authenticated.");
        }

        let mut contents = Vec::new();
        for m in messages {
            let role = match m.role.as_str() {
                "system" => "user", // Gemini system instructions usually go to a separate field, but for simplicity we can pass as user if needed.
                "assistant" => "model",
                "tool" => "function",
                _ => "user",
            };

            let mut parts = Vec::new();
            if role == "function" {
                // It's a tool response
                let content_str = m.content.clone().unwrap_or_default();
                let tool_call_id = m
                    .tool_call_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string());

                parts.push(json!({
                    "functionResponse": {
                        "name": tool_call_id, // We loosely map id to name here since OpenAI groups them
                        "response": {
                            "name": tool_call_id,
                            "content": content_str
                        }
                    }
                }));
            } else if let Some(calls) = m.tool_calls {
                for tc in calls {
                    let args: Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| json!({}));
                    parts.push(json!({
                        "functionCall": {
                            "name": tc.function.name,
                            "args": args
                        }
                    }));
                }
            } else if let Some(text) = m.content {
                parts.push(json!({ "text": text }));
            }

            contents.push(json!({
                "role": role,
                "parts": parts
            }));
        }

        // Move the first "system" prompt into systemInstruction if available
        let mut system_instruction = None;
        if !contents.is_empty()
            && contents[0]["role"] == "user"
            && contents[0]["parts"][0]["text"]
                .as_str()
                .unwrap_or("")
                .starts_with("You are Antigravity")
        {
            let sys = contents.remove(0);
            system_instruction = Some(sys["parts"][0].clone());
        }

        let mut request_json = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": 0.2
            }
        });

        if let Some(sys_inst) = system_instruction {
            request_json["systemInstruction"] = json!({
                "parts": [sys_inst]
            });
        }

        if let Some(t_defs) = tools {
            let mut funcs = Vec::new();
            for t in t_defs {
                funcs.push(json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "parameters": t.function.parameters
                }));
            }
            request_json["tools"] = json!([{
                "functionDeclarations": funcs
            }]);
        }

        let url = format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:streamGenerateContent?alt=sse",
            self.region, self.project_id, self.region, self.model
        );

        let response = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&request_json)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Vertex API error {}: {}", status, body);
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() || !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    on_event(StreamEvent::Done);
                    return Ok(());
                }

                if let Ok(json_val) = serde_json::from_str::<Value>(data) {
                    if let Some(candidates) = json_val["candidates"].as_array() {
                        if let Some(candidate) = candidates.first() {
                            if let Some(parts) = candidate["content"]["parts"].as_array() {
                                for part in parts {
                                    if let Some(text) = part["text"].as_str() {
                                        if !text.is_empty() {
                                            on_event(StreamEvent::Content(text.to_string()));
                                        }
                                    }
                                    if let Some(func_call) = part.get("functionCall") {
                                        if let Some(name) = func_call["name"].as_str() {
                                            let args = func_call["args"].to_string();
                                            // Gemini returns function calls complete in one chunk
                                            on_event(StreamEvent::ToolCall {
                                                id: uuid::Uuid::new_v4().to_string(), // Gen fake ID
                                                name: name.to_string(),
                                                arguments: args,
                                            });
                                        }
                                    }
                                }
                            }
                            if let Some(reason) = candidate["finishReason"].as_str() {
                                if reason == "STOP" || reason == "MAX_TOKENS" {
                                    // stream will end soon
                                }
                            }
                        }
                    }
                }
            }
        }
        on_event(StreamEvent::Done);
        Ok(())
    }
}
