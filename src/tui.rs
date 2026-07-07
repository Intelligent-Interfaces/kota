use crate::agent::AgentMode;
use crate::events::AgentEvent;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::io::stdout;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

/// Application state
struct App {
    input: String,
    output: Vec<(LineKind, String)>,
    scroll: u16,
    busy: bool,
    thinking_buf: String,
    active_tools: Vec<String>,
    last_duration_ms: u64,
    step_count: usize,
    mode: AgentMode,
    rx_kbps: f64,
    tx_kbps: f64,
}

#[derive(Clone)]
enum LineKind {
    User,
    Assistant,
    Thinking,
    ToolStart,
    ToolDone,
    System,
    Error,
}

impl App {
    fn new(startup_mode: AgentMode, port: u16) -> Self {
        Self {
            input: String::new(),
            output: vec![
                (LineKind::System, "   _  __      _".into()),
                (LineKind::System, "  | |/ /___  | |_  __ _   🦎".into()),
                (LineKind::System, "  | ' // _ \\ | __|/ _` |".into()),
                (LineKind::System, "  | . \\ (_) || |_| (_| |".into()),
                (LineKind::System, "  |_|\\_\\___/  \\__|\\__,_|".into()),
                (LineKind::System, "  [  COMPUTING ASSIST  ]".into()),
                (LineKind::System, "".into()),
                (
                    LineKind::System,
                    " ── SYSTEM LOADED ──────────────────────────────────────────".into(),
                ),
                (
                    LineKind::System,
                    format!("  • Port  : http://localhost:{} (Remote UI)", port),
                ),
                (
                    LineKind::System,
                    "  • Keys  : Ctrl+C (Quit) | Ctrl+R (Reset)".into(),
                ),
                (
                    LineKind::System,
                    " ───────────────────────────────────────────────────────────".into(),
                ),
                (LineKind::System, "".into()),
            ],
            scroll: 0,
            busy: false,
            thinking_buf: String::new(),
            active_tools: Vec::new(),
            last_duration_ms: 0,
            step_count: 0,
            mode: startup_mode,
            rx_kbps: 0.0,
            tx_kbps: 0.0,
        }
    }

    fn push_line(&mut self, kind: LineKind, text: String) {
        self.output.push((kind, text));
        // Don't reset scroll — if the user has scrolled up to read history,
        // keep them there. They can PageDown back to the live tail.
    }

    fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::UserMessage { text } => {
                if text.starts_with("SYSTEM:") {
                    if text.starts_with("SYSTEM: Mode changed to ") {
                        let mode_str = text.trim_start_matches("SYSTEM: Mode changed to ").trim();
                        self.mode = AgentMode::from_str(mode_str);
                    }
                    let content = text.trim_start_matches("SYSTEM:").trim();
                    for line in content.lines() {
                        self.push_line(LineKind::System, format!("  {}", line));
                    }
                } else {
                    self.push_line(LineKind::User, format!("▶ {}", text));
                }
            }

            AgentEvent::StepStarted { step, tokens_in } => {
                self.step_count = step;
                self.push_line(
                    LineKind::System,
                    format!("  ── step {} ({} tokens) ──", step, tokens_in),
                );
            }

            AgentEvent::Token { text } => {
                if let Some((LineKind::Assistant, last)) = self.output.last_mut() {
                    last.push_str(&text);
                } else {
                    self.push_line(LineKind::Assistant, format!("  {}", text));
                }
                self.scroll = 0;
            }

            AgentEvent::Thinking { text } => {
                self.thinking_buf.push_str(&text);
                if let Some((LineKind::Thinking, last)) = self.output.last_mut() {
                    last.push_str(&text);
                } else {
                    self.push_line(LineKind::Thinking, format!("  💭 {}", text));
                }
            }

            AgentEvent::ToolCallStarted { tool, args, .. } => {
                self.active_tools.push(tool.clone());
                self.push_line(
                    LineKind::ToolStart,
                    format!("  🔧 {}({})", tool, truncate_json(&args, 60)),
                );
            }

            AgentEvent::ToolCallFinished {
                tool,
                duration_ms,
                success,
                result_preview,
                ..
            } => {
                self.active_tools.retain(|t| t != &tool);
                let icon = if success { "✓" } else { "✗" };
                self.push_line(
                    LineKind::ToolDone,
                    format!("  {} {} ({}ms)", icon, tool, duration_ms),
                );
                if !result_preview.is_empty() {
                    let preview = result_preview.lines().next().unwrap_or("");
                    if !preview.is_empty() {
                        self.push_line(
                            LineKind::System,
                            format!("    → {}", truncate(preview, 80)),
                        );
                    }
                }
            }

            AgentEvent::Done { duration_ms, .. } => {
                self.last_duration_ms = duration_ms;
                self.busy = false;
                self.thinking_buf.clear();
                self.push_line(
                    LineKind::System,
                    format!("  ── done ({}ms) ──", duration_ms),
                );
            }

            AgentEvent::BudgetWarning { used, max } => {
                self.push_line(
                    LineKind::Error,
                    format!("  ⚠ context budget: {}/{} tokens", used, max),
                );
            }

            AgentEvent::CommandFinished => {
                self.busy = false;
            }

            AgentEvent::Error { message } => {
                self.push_line(LineKind::Error, format!("❌ {}", message));
                self.busy = false;
            }
            AgentEvent::TelemetryUpdate { rx_kbps, tx_kbps } => {
                self.rx_kbps = rx_kbps;
                self.tx_kbps = tx_kbps;
            }
            AgentEvent::NetworkThreatDetected {
                severity,
                description,
            } => {
                self.push_line(
                    LineKind::Error,
                    format!("🚨 THREAT [{}]: {}", severity, description),
                );
            }
        }
    }
}

pub async fn run(
    mut rx: broadcast::Receiver<AgentEvent>,
    input_tx: mpsc::UnboundedSender<String>,
    startup_mode: AgentMode,
    port: u16,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut app = App::new(startup_mode, port);

    loop {
        terminal.draw(|frame| draw(frame, &app))?;

        // Poll for keyboard/mouse events
        if event::poll(std::time::Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) => {
                    match (key.modifiers, key.code) {
                        (KeyModifiers::CONTROL, KeyCode::Char('c')) => break,

                        (KeyModifiers::CONTROL, KeyCode::Char('r')) => {
                            app.output.clear();
                            app.push_line(LineKind::System, "── conversation reset ──".into());
                            // Note: agent reset would need a separate channel message
                        }

                        (_, KeyCode::PageUp) => {
                            app.scroll = app.scroll.saturating_add(5);
                        }

                        (_, KeyCode::PageDown) => {
                            app.scroll = app.scroll.saturating_sub(5);
                            // Clamp to 0 so we snap back to live-follow mode
                            if app.scroll < 5 {
                                app.scroll = 0;
                            }
                        }

                        // G = jump to bottom (live-follow)
                        (_, KeyCode::Char('g')) if key.modifiers == KeyModifiers::NONE => {
                            app.scroll = 0;
                        }

                        (_, KeyCode::Enter) if !app.input.is_empty() && !app.busy => {
                            let input = app.input.clone();
                            app.input.clear();
                            app.busy = true;
                            app.thinking_buf.clear();
                            let _ = input_tx.send(input);
                        }

                        (_, KeyCode::Backspace) => {
                            app.input.pop();
                        }

                        (_, KeyCode::Char(c)) if !app.busy => {
                            app.input.push(c);
                        }

                        _ => {}
                    }
                }
                Event::Mouse(mouse_event) => match mouse_event.kind {
                    MouseEventKind::ScrollUp => {
                        app.scroll = app.scroll.saturating_add(2);
                    }
                    MouseEventKind::ScrollDown => {
                        app.scroll = app.scroll.saturating_sub(2);
                        if app.scroll < 2 {
                            app.scroll = 0;
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        // Drain agent events
        while let Ok(event) = rx.try_recv() {
            app.handle_event(event);
        }
    }

    disable_raw_mode()?;
    stdout().execute(DisableMouseCapture)?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // output
            Constraint::Length(3), // input
            Constraint::Length(1), // status
        ])
        .split(area);

    // Output
    let output_lines: Vec<Line> = app
        .output
        .iter()
        .map(|(kind, text)| {
            let style = match kind {
                LineKind::User => Style::default().fg(Color::Cyan).bold(),
                LineKind::Assistant => Style::default().fg(Color::White),
                LineKind::Thinking => Style::default().fg(Color::DarkGray).italic(),
                LineKind::ToolStart => Style::default().fg(Color::Yellow),
                LineKind::ToolDone => Style::default().fg(Color::Green),
                LineKind::System => Style::default().fg(Color::DarkGray),
                LineKind::Error => Style::default().fg(Color::Red),
            };
            Line::styled(text.as_str(), style)
        })
        .collect();

    // Output Border Style
    let output_style = if app.busy {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let inner_width = chunks[0].width.saturating_sub(2).max(1);
    let inner_height = chunks[0].height.saturating_sub(2);

    let mut total_lines: u16 = 0;
    for (_, text) in &app.output {
        if text.is_empty() {
            total_lines += 1;
            continue;
        }
        for sub_line in text.lines() {
            let w = sub_line.chars().count() as u16;
            if w == 0 {
                total_lines += 1;
            } else {
                let extra = if sub_line.contains(' ') { 1 } else { 0 };
                total_lines += w.div_ceil(inner_width) + extra;
            }
        }
    }

    let has_conversation = app.output.iter().any(|(kind, _)| {
        matches!(
            kind,
            LineKind::User
                | LineKind::Assistant
                | LineKind::Error
                | LineKind::ToolStart
                | LineKind::ToolDone
        )
    });

    let max_scroll = if has_conversation {
        total_lines.saturating_sub(inner_height)
    } else {
        0
    };
    let actual_scroll = max_scroll.saturating_sub(app.scroll);

    let output_widget = Paragraph::new(output_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(output_style)
                .title(Span::styled(
                    " kota ",
                    Style::default().bold().fg(Color::Cyan),
                )),
        )
        .wrap(Wrap { trim: false })
        .scroll((actual_scroll, 0));
    frame.render_widget(output_widget, chunks[0]);

    // Input
    let input_title = if app.busy { " working... " } else { " prompt " };
    let input_style = if app.busy {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let input_widget = Paragraph::new(app.input.as_str()).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(Span::styled(
                input_title,
                Style::default().bold().fg(Color::Cyan),
            ))
            .border_style(input_style),
    );
    frame.render_widget(input_widget, chunks[1]);

    if !app.busy {
        frame.set_cursor_position((chunks[1].x + app.input.len() as u16 + 1, chunks[1].y + 1));
    }

    // Status bar
    let scroll_str = if app.scroll > 0 {
        " | ↑ scrolled (PgDn/g to tail)".to_string()
    } else {
        String::new()
    };
    let tools_str = if app.active_tools.is_empty() {
        String::new()
    } else {
        format!(" | 🔧 {}", app.active_tools.join(", "))
    };
    let status = format!(
        " mode: {} | step {} | last: {}ms | tx: {:.1}kbps | rx: {:.1}kbps{}{} | Ctrl+C quit",
        app.mode.to_str().to_uppercase(),
        app.step_count,
        app.last_duration_ms,
        app.tx_kbps,
        app.rx_kbps,
        tools_str,
        scroll_str,
    );
    let status_widget =
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray).bg(Color::Black));
    frame.render_widget(status_widget, chunks[2]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}

fn truncate_json(v: &serde_json::Value, max: usize) -> String {
    truncate(&v.to_string(), max)
}
