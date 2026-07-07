mod agent;
mod events;
mod llm;
pub mod mcp;
pub mod memory;
mod sensing;
mod server;
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

    /// A single query to run non-interactively and print to stdout
    #[arg(long)]
    query: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let llm_client = llm::LlmClient::new(&cli.api_url, &cli.model);
    let startup_mode = agent::AgentMode::from_str(&cli.mode);
    let mut agent =
        agent::Agent::new(llm_client, cli.max_tokens, &cli.workdir, startup_mode).await?;

    if let Some(query) = cli.query {
        let (tx, mut rx) = tokio::sync::broadcast::channel::<events::AgentEvent>(100);

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = agent.process(&query, tx_clone).await {
                let _ = tx.send(events::AgentEvent::Error {
                    message: e.to_string(),
                });
            }
        });

        while let Ok(event) = rx.recv().await {
            match event {
                events::AgentEvent::Token { text } => {
                    print!("{}", text);
                    use std::io::Write;
                    std::io::stdout().flush().unwrap();
                }
                events::AgentEvent::ToolCallStarted { tool, args, .. } => {
                    println!("\n🔧 [Tool Call Started]: {} with {:?}", tool, args);
                }
                events::AgentEvent::ToolCallFinished {
                    tool,
                    success,
                    result_preview,
                    ..
                } => {
                    println!("\n🔧 [Tool Call Finished]: {} (Success: {})", tool, success);
                    println!("Preview: {}", result_preview);
                }
                events::AgentEvent::Done { duration_ms, .. } => {
                    println!("\n✅ Done in {}ms", duration_ms);
                    break;
                }
                events::AgentEvent::Error { message } => {
                    println!("\n❌ Error: {}", message);
                    break;
                }
                _ => {}
            }
        }
        return Ok(());
    }

    let (tx, rx1) = tokio::sync::broadcast::channel::<events::AgentEvent>(10000);
    let (input_tx, mut input_rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();

    let workdir_str = cli.workdir.clone();
    let model_str = cli.model.clone();
    let max_tokens = cli.max_tokens;
    let api_url = cli.api_url.clone();

    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let mut active_mode = startup_mode;
        while let Some((user_input, source)) = input_rx.recv().await {
            let trimmed = user_input.trim();
            if trimmed == "/help" {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let help_text = "\
Available local commands (do not consume tokens or write to history):
  /help         - Display this help message
  /modes        - List all available agent modes and descriptions
  /status       - Display current agent configuration & session status
  /mode <name>  - Switch the agent to a different mode
  /art <type>   - Render an interactive ASCII animation (cat, clouds, plasma, lizard)";
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!("SYSTEM:\n{}", help_text),
                    source: "system".to_string(),
                });
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            if trimmed == "/art" || trimmed == "/art help" {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let help_text = "\
Available art animations:
  /art cat      - Lounging beach cat with animated waves
  /art clouds   - Parallax scrolling ASCII clouds
  /art plasma   - Abstract mathematical wave generator
  /art lizard   - Centered wiggling ASCII lizard
Press any key to exit the animation once started.";
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!("SYSTEM:\n{}", help_text),
                    source: "system".to_string(),
                });
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            if user_input.starts_with("/art ") {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let art_type = user_input.trim_start_matches("/art ").trim().to_lowercase();
                if art_type == "cat"
                    || art_type == "clouds"
                    || art_type == "plasma"
                    || art_type == "lizard"
                {
                    let _ = tx_clone.send(events::AgentEvent::StartArt { mode: art_type });
                } else {
                    let _ = tx_clone.send(events::AgentEvent::UserMessage {
                        text: format!(
                            "SYSTEM:\nUnknown art mode '{}'. Try: cat, clouds, plasma, lizard.",
                            art_type
                        ),
                        source: "system".to_string(),
                    });
                }
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            if trimmed == "/modes" {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let modes_text = "\
Available agent modes (composed of weighted skill vectors):
  coder     - Software Engineering & Testing (1.0 coder, 0.2 eval)
  cpe       - Client Platform Engineering (1.0 cpe, 0.4 architect)
  eval      - Safety and Red-teaming Evaluation (1.0 eval)
  research  - Literature Review & Scientific Synthesis (1.0 research, 0.3 coder)
  architect - Systems Design & Infrastructure (1.0 architect, 0.5 cpe)
  librarian - LLM Wiki Maintenance & Knowledge Compiling (1.0 librarian, 0.4 research)";
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!("SYSTEM:\n{}", modes_text),
                    source: "system".to_string(),
                });
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            if trimmed == "/status" {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let status_text = format!(
                    "Agent Status:\n  Active Mode: {}\n  Model: {}\n  Endpoint: {}\n  Workdir: {}\n  Token Budget: {}",
                    active_mode.to_str().to_uppercase(),
                    model_str,
                    api_url,
                    workdir_str,
                    max_tokens
                );
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!("SYSTEM:\n{}", status_text),
                    source: "system".to_string(),
                });
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            if trimmed == "/mode" {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: "SYSTEM:\nUsage: /mode <name>\nUse /modes to list all available modes."
                        .to_string(),
                    source: "system".to_string(),
                });
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            if user_input.starts_with("/mode ") {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let mode_str = user_input.trim_start_matches("/mode ").trim();
                let new_mode = agent::AgentMode::from_str(mode_str);
                agent.set_mode(new_mode);
                active_mode = new_mode;
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!(
                        "SYSTEM: Mode changed to {}",
                        new_mode.to_str().to_uppercase()
                    ),
                    source: "system".to_string(),
                });
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            let _ = tx_clone.send(events::AgentEvent::UserMessage {
                text: user_input.clone(),
                source: source.clone(),
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
