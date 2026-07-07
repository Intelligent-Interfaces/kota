use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

#[derive(Deserialize, Debug, Clone)]
pub struct McpServerConfig {
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct McpConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

pub struct McpClient {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl McpClient {
    pub async fn start(name: &str, config: &McpServerConfig) -> anyhow::Result<Self> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .envs(&config.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to acquire stdin for MCP server {}", name))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to acquire stdout for MCP server {}", name))?;
        let stdout = BufReader::new(stdout);

        let mut client = Self {
            name: name.to_string(),
            child,
            stdin,
            stdout,
            next_id: 1,
        };

        client.initialize().await?;
        Ok(client)
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let mut req_str = serde_json::to_string(&req)?;
        req_str.push('\n');
        self.stdin.write_all(req_str.as_bytes()).await?;
        self.stdin.flush().await?;

        let mut line = String::new();
        while self.stdout.read_line(&mut line).await? > 0 {
            let val: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => {
                    line.clear();
                    continue;
                }
            };

            if val.get("id").and_then(|v| v.as_i64()) == Some(id) {
                if let Some(err) = val.get("error") {
                    anyhow::bail!("MCP server returned error: {}", err);
                }
                return Ok(val
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null));
            }
            line.clear();
        }
        anyhow::bail!("Connection closed before receiving response")
    }

    async fn send_notification(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<()> {
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let mut req_str = serde_json::to_string(&req)?;
        req_str.push('\n');
        self.stdin.write_all(req_str.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn initialize(&mut self) -> anyhow::Result<()> {
        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "kota",
                "version": "0.1.0"
            }
        });

        let _result = self.send_request("initialize", init_params).await?;
        self.send_notification("notifications/initialized", serde_json::json!({}))
            .await?;
        Ok(())
    }

    pub async fn list_tools(&mut self) -> anyhow::Result<Vec<McpTool>> {
        let res = self
            .send_request("tools/list", serde_json::json!({}))
            .await?;
        let tools_val = res
            .get("tools")
            .ok_or_else(|| anyhow::anyhow!("No tools field in list_tools response"))?;
        let tools: Vec<McpTool> = serde_json::from_value(tools_val.clone())?;
        Ok(tools)
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<String> {
        let call_params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });

        let res = self.send_request("tools/call", call_params).await?;

        if let Some(content) = res.get("content").and_then(|c| c.as_array()) {
            let mut texts = Vec::new();
            for block in content {
                if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        texts.push(text.to_string());
                    }
                }
            }
            return Ok(texts.join("\n"));
        }

        anyhow::bail!("Invalid response format from tool call (missing content array)")
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[derive(Clone)]
pub struct McpManager {
    clients: Arc<Mutex<HashMap<String, McpClient>>>,
    tools: Arc<Mutex<HashMap<String, (String, McpTool)>>>,
}

impl McpManager {
    pub async fn load_from_config<P: AsRef<Path>>(config_path: P) -> anyhow::Result<Self> {
        let clients = Arc::new(Mutex::new(HashMap::new()));
        let tools = Arc::new(Mutex::new(HashMap::new()));

        if !config_path.as_ref().exists() {
            return Ok(Self { clients, tools });
        }

        let content = tokio::fs::read_to_string(config_path).await?;
        let config: McpConfig = serde_json::from_str(&content)?;

        for (name, server_config) in config.mcp_servers {
            match McpClient::start(&name, &server_config).await {
                Ok(mut client) => match client.list_tools().await {
                    Ok(server_tools) => {
                        let mut t_guard = tools.lock().await;
                        for tool in server_tools {
                            t_guard.insert(tool.name.clone(), (name.clone(), tool));
                        }
                        clients.lock().await.insert(name, client);
                    }
                    Err(e) => {
                        eprintln!("Failed to list tools for MCP server '{}': {}", name, e);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to start MCP server '{}': {}", name, e);
                }
            }
        }

        Ok(Self { clients, tools })
    }

    pub async fn list_all_tools(&self) -> Vec<McpTool> {
        let t_guard = self.tools.lock().await;
        t_guard.values().map(|(_, tool)| tool.clone()).collect()
    }

    pub async fn has_tool(&self, tool_name: &str) -> bool {
        let t_guard = self.tools.lock().await;
        t_guard.contains_key(tool_name)
    }

    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<String> {
        let (server_name, _) = {
            let t_guard = self.tools.lock().await;
            t_guard
                .get(tool_name)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Unknown MCP tool: {}", tool_name))?
        };

        let mut c_guard = self.clients.lock().await;
        let client = c_guard
            .get_mut(&server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP server not running: {}", server_name))?;
        client.call_tool(tool_name, arguments).await
    }
}
