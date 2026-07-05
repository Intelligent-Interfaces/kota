mod agent;
mod events;
mod llm;
pub mod memory;
mod server;
mod sensing;
mod skills;
mod tools;
mod tui;

use clap::Parser;

#[derive(Parser)]
#[command(name = "kota", about = "TUI agent coder & computer assistant")]
struct Cli {
    /// Base URL for the local LLM API (OpenAI-compatible)
    #[arg(long, default_value = "http://localhost:11434/v1")]
    api_url: String,

    /// Model name to use
    #[arg(long, default_value = "qwen3:8b")]
    model: String,

    /// Working directory to operate in
    #[arg(long, default_value = ".")]
    workdir: String,

    /// Max context tokens before budget warning
    #[arg(long, default_value_t = 24000)]
    max_tokens: usize,

    /// Initial mode (coder, cpe, eval, research, librarian)
    #[arg(long, default_value = "coder")]
    mode: String,

    /// Port for the remote web server UI
    #[arg(long, default_value_t = 8765)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let llm_client = llm::LlmClient::new(&cli.api_url, &cli.model);
    let startup_mode = agent::AgentMode::from_str(&cli.mode);
    let mut agent =
        agent::Agent::new(llm_client, cli.max_tokens, &cli.workdir, startup_mode).await?;

    let (tx, rx1) = tokio::sync::broadcast::channel::<events::AgentEvent>(100);
    let (input_tx, mut input_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let tx_clone = tx.clone();
    tokio::spawn(async move {
        while let Some(user_input) = input_rx.recv().await {
            if user_input.starts_with("/mode ") {
                let mode_str = user_input.trim_start_matches("/mode ").trim();
                let new_mode = agent::AgentMode::from_str(mode_str);
                agent.set_mode(new_mode);
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!(
                        "SYSTEM: Mode changed to {}",
                        new_mode.to_str().to_uppercase()
                    ),
                });
                continue;
            }

            let _ = tx_clone.send(events::AgentEvent::UserMessage {
                text: user_input.clone(),
            });

            if let Err(e) = agent.process(&user_input, tx_clone.clone()).await {
                let _ = tx_clone.send(events::AgentEvent::Error {
                    message: e.to_string(),
                });
            }
        }
    });

    let port = cli.port;
    let tx_server = tx.clone();
    let input_tx_server = input_tx.clone();
    tokio::spawn(async move {
        server::start(port, input_tx_server, tx_server).await;
    });

    let tx_telemetry = tx.clone();
    tokio::spawn(async move {
        sensing::run_telemetry_loop(tx_telemetry).await;
    });

    tui::run(rx1, input_tx, startup_mode, port).await
}
