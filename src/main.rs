mod agent;
mod llm;
mod tools;
mod tui;
mod events;
mod server;

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let llm_client = llm::LlmClient::new(&cli.api_url, &cli.model);
    let mut agent = agent::Agent::new(llm_client, cli.max_tokens, &cli.workdir);

    let (tx, rx1) = tokio::sync::broadcast::channel::<events::AgentEvent>(100);
    let (input_tx, mut input_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let tx_clone = tx.clone();
    tokio::spawn(async move {
        while let Some(user_input) = input_rx.recv().await {
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

    let tx_server = tx.clone();
    let input_tx_server = input_tx.clone();
    tokio::spawn(async move {
        server::start(input_tx_server, tx_server).await;
    });

    tui::run(rx1, input_tx).await
}
