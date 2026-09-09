mod agent;
mod events;
mod llm;
pub mod mcp;
pub mod memory;
mod power;
mod sensing;
mod server;
mod skills;
mod telemetry;
mod tools;
mod tui;
pub mod verifier;
mod vertex;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kota", about = "TUI agent coder & computer assistant")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

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

#[derive(Subcommand)]
enum Commands {
    /// Verify a LaTeX document for syntax hygiene, citation provenance, table honesty, cross-references, and semantic claims
    Verify {
        /// Path to the .tex file
        path: PathBuf,
        /// Optional path to the .bib file (defaults to inferred from \bibliography or directory)
        #[arg(long)]
        bib: Option<PathBuf>,
        /// Enable semantic claim verification and passage confidence scoring
        #[arg(long)]
        claims: bool,
        /// Maximum candidate claims to extract and evaluate
        #[arg(long, default_value_t = 5)]
        max_claims: usize,
        /// Base URL for the LLM API (for claim verification)
        #[arg(long, default_value = "http://localhost:11434/v1")]
        api_url: String,
        /// Model name to use for semantic claim verification
        #[arg(long, default_value = "qwen3:8b")]
        model: String,
        /// Output the verification report as JSON
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Some(Commands::Verify {
        path,
        bib,
        claims,
        max_claims,
        api_url,
        model,
        json,
    }) = cli.command
    {
        let llm_client = if claims {
            Some(llm::LlmClient::new(&api_url, &model))
        } else {
            None
        };

        match verifier::DocumentVerifier::verify_async(
            &path,
            bib.as_ref(),
            claims,
            llm_client.as_ref(),
            max_claims,
        )
        .await
        {
            Ok(report) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&report)?);
                } else {
                    println!(
                        "\n🔍 Running Cognitive Voxel Verification Pipeline on: {}",
                        report.file_path
                    );
                    println!("──────────────────────────────────────────────────────────────────────────");
                    println!("📄 Total Lines: {}", report.total_lines);
                    println!(
                        "📊 Lyapunov Stability Exponent: {:.2} (Target: λ < 0)",
                        report.lyapunov_stability
                    );
                    if let Some(mean) = report.mean_confidence_score {
                        println!("🧠 Mean Semantic Claim Confidence: {:.1}%", mean * 100.0);
                    }
                    println!(
                        "Status: {}",
                        if report.passed {
                            "✅ PASSED (Reviewer-Ready)"
                        } else {
                            "❌ FAILED (Violations Found)"
                        }
                    );
                    println!("──────────────────────────────────────────────────────────────────────────");

                    if report.diagnostics.is_empty() {
                        println!("✨ Zero violations found! Document satisfies all cognitive voxel invariants.");
                    } else {
                        for d in &report.diagnostics {
                            let icon = match d.severity {
                                verifier::DiagnosticSeverity::Error => "🔴 [ERROR]",
                                verifier::DiagnosticSeverity::Warning => "🟡 [WARN]",
                                verifier::DiagnosticSeverity::Info => "ℹ️ [INFO]",
                            };
                            println!("\n{} [{}] Line {}: {}", icon, d.voxel, d.line, d.message);
                            if let Some(ref s) = d.snippet {
                                println!("   Snippet:   \"{}\"", s.trim());
                            }
                            if let Some(ref sugg) = d.suggestion {
                                println!("   Suggested: {}", sugg);
                            }
                        }
                        println!(
                            "\nSummary: {} error(s), {} warning(s), {} info(s).",
                            report.summary.errors, report.summary.warnings, report.summary.infos
                        );
                    }

                    if !report.claims.is_empty() {
                        println!(
                            "\n🔬 [VoxelClaim] Semantic Claims & Passage Confidence Evaluation:"
                        );
                        println!("──────────────────────────────────────────────────────────────────────────");
                        for (i, c) in report.claims.iter().enumerate() {
                            let badge = match c.confidence_grade {
                                verifier::ConfidenceGrade::High => "🟢 [HIGH]",
                                verifier::ConfidenceGrade::Moderate => "🟡 [MODERATE]",
                                verifier::ConfidenceGrade::Low => "🟠 [LOW]",
                                verifier::ConfidenceGrade::Fragile => "🔴 [FRAGILE]",
                            };
                            println!(
                                "\nClaim #{}: {} ({:.1}%) - [{}] Line {}",
                                i + 1,
                                badge,
                                c.confidence_score * 100.0,
                                c.claim_type,
                                c.line
                            );
                            println!("   Passage:    \"{}\"", c.passage.trim());
                            println!("   Rationale:  {}", c.rationale);
                            if let Some(ref ev) = c.evidence_found {
                                println!("   Evidence:   {}", ev);
                            }
                            if let Some(ref sugg) = c.suggestion {
                                println!("   Suggestion: {}", sugg);
                            }
                        }
                    }
                    println!("──────────────────────────────────────────────────────────────────────────\n");
                }
                if !report.passed {
                    std::process::exit(1);
                }
                return Ok(());
            }
            Err(e) => {
                eprintln!("Verification failed: {}", e);
                std::process::exit(1);
            }
        }
    }

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

    let rx_telemetry = tx.subscribe();

    // Try to get Project ID from env, or fallback to active gcloud config
    let project_id = std::env::var("GCP_PROJECT_ID").unwrap_or_else(|_| {
        let output = std::process::Command::new("gcloud")
            .args(["config", "get-value", "project"])
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout).trim().to_string();
            }
        }
        String::new()
    });

    if !project_id.is_empty() {
        let telemetry_worker =
            telemetry::TelemetryWorker::new(&project_id, "kota_telemetry", "metrics");
        tokio::spawn(async move {
            telemetry_worker.run(rx_telemetry).await;
        });
    }

    let (input_tx, mut input_rx) = tokio::sync::mpsc::unbounded_channel::<(String, String)>();

    let workdir_str = cli.workdir.clone();
    let model_str = cli.model.clone();
    let max_tokens = cli.max_tokens;
    let api_url = cli.api_url.clone();

    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let mut active_mode = startup_mode;
        let mut current_model = model_str.clone();
        let mut current_api_url = api_url.clone();
        let original_model = model_str.clone();
        let original_api_url = api_url.clone();
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
  /power <on|off> - Toggle hardware power monitoring (macpow)
  /eco <on|off>   - Toggle lightweight Eco LLM (llama3.2:1b)
  /cloud <on|off> - Toggle cloud inference API offloading (Groq)
  /gcp <on|off>   - Toggle GCP Vertex AI inference (Gemini 1.5 Pro)
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
                    current_model,
                    current_api_url,
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

            if user_input.starts_with("/power ") {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let val = user_input.trim_start_matches("/power ").trim();
                let enabled = val == "on";
                let _ = tx_clone.send(events::AgentEvent::PowerConfig { enabled });
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!(
                        "SYSTEM: Power monitoring {}",
                        if enabled { "enabled" } else { "disabled" }
                    ),
                    source: "system".to_string(),
                });
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            if user_input.starts_with("/eco ") {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let val = user_input.trim_start_matches("/eco ").trim();
                if val == "on" {
                    current_model = "llama3.2:1b".to_string();
                } else {
                    current_model = original_model.clone();
                }
                agent.set_llm(llm::LlmClient::new(&current_api_url, &current_model));
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!("SYSTEM: Eco mode {}. Model is now {}", val, current_model),
                    source: "system".to_string(),
                });
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            if user_input.starts_with("/cloud ") {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let val = user_input.trim_start_matches("/cloud ").trim();
                if val == "on" {
                    current_api_url = "https://api.groq.com/openai/v1".to_string();
                    current_model = "llama3-8b-8192".to_string();
                    agent.set_llm(llm::LlmClient::new(&current_api_url, &current_model));
                } else {
                    current_api_url = original_api_url.clone();
                    current_model = original_model.clone();
                    agent.set_llm(llm::LlmClient::new(&current_api_url, &current_model));
                }
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!(
                        "SYSTEM: Cloud mode {}. Using {} via {}",
                        val, current_model, current_api_url
                    ),
                    source: "system".to_string(),
                });
                let _ = tx_clone.send(events::AgentEvent::CommandFinished);
                continue;
            }

            if user_input.starts_with("/gcp ") {
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: user_input.clone(),
                    source: source.clone(),
                });
                let val = user_input.trim_start_matches("/gcp ").trim();
                if val == "on" {
                    current_model = "gemini-1.5-pro-002".to_string();
                    current_api_url = "Vertex AI".to_string();
                    let project_id = std::env::var("GCP_PROJECT_ID").unwrap_or_else(|_| {
                        let output = std::process::Command::new("gcloud")
                            .args(["config", "get-value", "project"])
                            .output();
                        if let Ok(out) = output {
                            if out.status.success() {
                                return String::from_utf8_lossy(&out.stdout).trim().to_string();
                            }
                        }
                        String::new()
                    });
                    let region =
                        std::env::var("GCP_REGION").unwrap_or_else(|_| "us-central1".to_string());
                    agent.set_llm(llm::LlmClient::new_vertex(
                        &project_id,
                        &region,
                        &current_model,
                    ));
                } else {
                    current_api_url = original_api_url.clone();
                    current_model = original_model.clone();
                    agent.set_llm(llm::LlmClient::new(&current_api_url, &current_model));
                }
                let _ = tx_clone.send(events::AgentEvent::UserMessage {
                    text: format!(
                        "SYSTEM: GCP mode {}. Using {} via {}",
                        val, current_model, current_api_url
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

    let tx_power = tx.clone();
    let rx_power = tx.subscribe();
    tokio::spawn(async move {
        power::start_power_monitor(tx_power, rx_power).await;
    });

    tui::run(rx1, input_tx, startup_mode, port).await
}
