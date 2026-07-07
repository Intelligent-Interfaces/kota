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

#[derive(Clone, Copy, PartialEq, Debug)]
enum ArtMode {
    Cat,
    Plasma,
    Lizard,
    Clouds,
}

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
    art_mode: Option<ArtMode>,
    frame_count: u32,
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
                (LineKind::System, "                       )/_".into()),
                (LineKind::System, "             _.--..---\"-,--c_".into()),
                (LineKind::System, "        \\L..'           ._O__)_".into()),
                (
                    LineKind::System,
                    ",-.     _.+  _  \\..--( /           🦎".into(),
                ),
                (LineKind::System, "  `\\.-''__.-' \\ (     \\_".into()),
                (LineKind::System, "    `'''       `\\__   /\\".into()),
                (LineKind::System, "                ')".into()),
                (LineKind::System, "".into()),
                (LineKind::System, "             _  __      _".into()),
                (LineKind::System, "            | |/ /___  | |_  __ _".into()),
                (
                    LineKind::System,
                    "            | ' // _ \\ | __|/ _` |".into(),
                ),
                (
                    LineKind::System,
                    "            | . \\ (_) || |_| (_| |".into(),
                ),
                (
                    LineKind::System,
                    "            |_|\\_\\___/  \\__|\\__,_|".into(),
                ),
                (
                    LineKind::System,
                    "            [  COMPUTING ASSIST  ]".into(),
                ),
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
            art_mode: None,
            frame_count: 0,
        }
    }

    fn push_line(&mut self, kind: LineKind, text: String) {
        self.output.push((kind, text));
        // Don't reset scroll — if the user has scrolled up to read history,
        // keep them there. They can PageDown back to the live tail.
    }

    fn handle_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::UserMessage { text, source } => {
                if source == "system" {
                    if text.starts_with("SYSTEM: Mode changed to ") {
                        let mode_str = text.trim_start_matches("SYSTEM: Mode changed to ").trim();
                        self.mode = AgentMode::from_str(mode_str);
                    }
                    let content = text.trim_start_matches("SYSTEM:").trim();
                    for line in content.lines() {
                        self.push_line(LineKind::System, format!("  {}", line));
                    }
                } else if source == "remote" {
                    self.busy = true; // Block local input while agent is busy processing remote message
                    self.push_line(LineKind::User, format!("📱 [Remote]: {}", text));
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
            AgentEvent::StartArt { mode } => {
                self.art_mode = match mode.as_str() {
                    "cat" => Some(ArtMode::Cat),
                    "plasma" => Some(ArtMode::Plasma),
                    "lizard" => Some(ArtMode::Lizard),
                    "clouds" => Some(ArtMode::Clouds),
                    _ => None,
                };
                self.frame_count = 0;
            }
        }
    }
}

pub async fn run(
    mut rx: broadcast::Receiver<AgentEvent>,
    input_tx: mpsc::UnboundedSender<(String, String)>,
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

        // Increment frame count for animations
        app.frame_count = app.frame_count.wrapping_add(1);

        // Poll for keyboard/mouse events
        if event::poll(std::time::Duration::from_millis(16))? {
            let ev = event::read()?;
            if app.art_mode.is_some() {
                // Any input event (key press, mouse click) exits art mode
                if let Event::Key(_) | Event::Mouse(_) = ev {
                    app.art_mode = None;
                }
            } else {
                match ev {
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

                            // Ctrl+G = jump to bottom (live-follow)
                            (KeyModifiers::CONTROL, KeyCode::Char('g')) => {
                                app.scroll = 0;
                            }

                            (_, KeyCode::Enter) if !app.input.is_empty() && !app.busy => {
                                let input = app.input.clone();
                                app.input.clear();
                                app.busy = true;
                                app.thinking_buf.clear();
                                let _ = input_tx.send((input, "terminal".to_string()));
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

fn draw_art(art_mode: ArtMode, frame_count: u32, width: u16, height: u16) -> Vec<Line<'static>> {
    let w = width as usize;
    let h = height as usize;
    let mut lines = Vec::new();

    match art_mode {
        ArtMode::Lizard => {
            let phase = (frame_count / 15) % 4;
            let eyes = if phase == 1 || phase == 3 {
                "(--)..(--)"
            } else {
                "()..()"
            };
            let tongue = if phase == 1 { "==" } else { "__" };

            // Jonathon R. Oglesbee lizard with wiggling tail and blinking eyes
            let tail1 = if phase.is_multiple_of(2) {
                " / /"
            } else {
                " \\ \\"
            };
            let tail2 = if phase.is_multiple_of(2) {
                "( ("
            } else {
                ") )"
            };
            let tail3 = if phase.is_multiple_of(2) {
                " \\ \\"
            } else {
                " / /"
            };
            let tail4 = if phase.is_multiple_of(2) {
                ") )"
            } else {
                "( ("
            };

            let raw_lines = vec![
                "              ____...---...___".to_string(),
                "___.....---\"\"\"        .       \"\"--..____".to_string(),
                "     .                  .            .".to_string(),
                " .             _.--._       /|".to_string(),
                format!("        .    .'{}`.    {}", eyes, tail1),
                format!("            ( `-.__{}__.-' )  {}    .", tongue, tail2),
                format!("    .         \\        /    {}", tail3),
                format!("        .      \\      /      {}        .", tail4),
                "            .' -.__.- `.-.-'_.'".to_string(),
                " .        .'  /-____-\\\\  `.-'       .".to_string(),
                "          \\  /-.____.-\\  /-.".to_string(),
                "           \\ \\`-.__.-'/ /\\|\\|".to_string(),
                "          .'  `.    .'  `.".to_string(),
                "          |/\\/\\|    |/\\/\\|".to_string(),
            ];

            let top_pad = h.saturating_sub(raw_lines.len()) / 2;
            for _ in 0..top_pad {
                lines.push(Line::raw(""));
            }
            for line in raw_lines {
                let left_pad = w.saturating_sub(line.chars().count()) / 2;
                let padded = format!("{:left_pad$}{}", "", line, left_pad = left_pad);
                lines.push(Line::styled(padded, Style::default().fg(Color::Green)));
            }
        }
        ArtMode::Cat => {
            let phase = (frame_count / 12) % 4;
            let eyes = if phase.is_multiple_of(2) {
                "■-■"
            } else {
                "☼.☼"
            };
            let tail = if phase.is_multiple_of(2) { "~" } else { "≈" };

            let w_offset = (frame_count / 6) as usize;
            let mut wave_line1 = String::new();
            let mut wave_line2 = String::new();
            let mut wave_line3 = String::new();
            for i in 0..w {
                wave_line1.push(if (i + w_offset) % 8 < 4 { '~' } else { ' ' });
                wave_line2.push(if (i + w_offset / 2) % 12 < 6 {
                    '≈'
                } else {
                    ' '
                });
                wave_line3.push(if (i + w_offset * 2) % 6 < 3 { '^' } else { 'v' });
            }

            let raw_lines = vec![
                "          _.._  .---.".to_string(),
                "         ( `  \\'     `".to_string(),
                "          |  /  \\  \\  \\".to_string(),
                "          |__\\__/__/__/_".to_string(),
                "                |".to_string(),
                format!(
                    "                |   |\\_/|   {}",
                    wave_line1.chars().take(15).collect::<String>()
                ),
                format!(
                    "                |  =({})={}  _.._{}",
                    eyes,
                    tail,
                    wave_line2.chars().take(8).collect::<String>()
                ),
                "               /| \\  _  /   /     \\".to_string(),
                wave_line3,
            ];

            let top_pad = h.saturating_sub(raw_lines.len()) / 2;
            for _ in 0..top_pad {
                lines.push(Line::raw(""));
            }
            for (idx, line) in raw_lines.into_iter().enumerate() {
                let style = if idx < 5 {
                    Style::default().fg(Color::Yellow)
                } else if idx < 8 {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::Blue)
                };
                if idx == 8 {
                    lines.push(Line::styled(line, style));
                } else {
                    let left_pad = w.saturating_sub(line.chars().count()) / 2;
                    let padded = format!("{:left_pad$}{}", "", line, left_pad = left_pad);
                    lines.push(Line::styled(padded, style));
                }
            }
        }
        ArtMode::Plasma => {
            for y in 0..h {
                let mut row = String::new();
                for x in 0..w {
                    let x_f = x as f32;
                    let y_f = y as f32;
                    let t_f = frame_count as f32;

                    let val = (x_f / 6.0 + t_f / 8.0).sin()
                        + (y_f / 3.0 - t_f / 12.0).cos()
                        + ((x_f + y_f) / 10.0 + t_f / 15.0).sin();

                    let idx = (((val + 3.0) / 6.0) * 9.0) as i32;
                    let idx = idx.clamp(0, 9) as usize;
                    let chars = b" .:-=+*#%@";
                    row.push(chars[idx] as char);
                }
                lines.push(Line::styled(row, Style::default().fg(Color::Magenta)));
            }
        }
        ArtMode::Clouds => {
            for y in 0..h {
                let mut spans = Vec::new();
                let mut current_segment = String::new();
                let mut current_style = Style::default();
                let mut last_type = 0; // 0: sky, 1: cloud edge, 2: cloud body

                for x in 0..w {
                    let h_f = h as f32;
                    let x_f = x as f32;
                    let y_f = y as f32;
                    let t_f = frame_count as f32;

                    // Band 1 (High, slow, small clouds)
                    let center1 = h_f * 0.3;
                    let thick1 = (h_f * 0.12).max(1.0);
                    let wave1 =
                        (x_f / 8.0 + t_f / 30.0).sin() * 0.5 + (x_f / 4.0 - t_f / 18.0).cos() * 0.2;
                    let y_dist1 = y_f - center1;
                    let falloff1 = if y_dist1 > 0.0 {
                        y_dist1 / (thick1 * 0.4) // flat bottom (rapid falloff)
                    } else {
                        y_dist1.abs() / thick1 // puffy top (gradual falloff)
                    };
                    let density1 = wave1 + 0.15 - falloff1;

                    // Band 2 (Low, fast, large clouds)
                    let center2 = h_f * 0.65;
                    let thick2 = (h_f * 0.20).max(1.0);
                    let wave2 = (x_f / 15.0 + t_f / 14.0).sin() * 0.6
                        + (x_f / 7.0 - t_f / 9.0).cos() * 0.25;
                    let y_dist2 = y_f - center2;
                    let falloff2 = if y_dist2 > 0.0 {
                        y_dist2 / (thick2 * 0.4) // flat bottom (rapid falloff)
                    } else {
                        y_dist2.abs() / thick2 // puffy top (gradual falloff)
                    };
                    let density2 = wave2 + 0.15 - falloff2;

                    let density = density1.max(density2);

                    let (char_val, char_type, style) = if density > 0.3 {
                        ('@', 2, Style::default().fg(Color::White).bold())
                    } else if density > 0.1 {
                        ('O', 2, Style::default().fg(Color::White).bold())
                    } else if density > -0.05 {
                        ('o', 2, Style::default().fg(Color::White))
                    } else if density > -0.2 {
                        ('~', 1, Style::default().fg(Color::Gray))
                    } else if density > -0.35 {
                        ('-', 1, Style::default().fg(Color::DarkGray))
                    } else {
                        // Twinkling star field in the sky
                        let star =
                            (x as i32 * 17 + y as i32 * 31 + (frame_count / 12) as i32) % 120;
                        if star == 0 {
                            ('*', 0, Style::default().fg(Color::Yellow))
                        } else if star == 1 {
                            ('.', 0, Style::default().fg(Color::DarkGray))
                        } else {
                            (' ', 0, Style::default().fg(Color::Black))
                        }
                    };

                    if char_type != last_type && !current_segment.is_empty() {
                        spans.push(Span::styled(current_segment.clone(), current_style));
                        current_segment.clear();
                    }
                    current_segment.push(char_val);
                    current_style = style;
                    last_type = char_type;
                }
                if !current_segment.is_empty() {
                    spans.push(Span::styled(current_segment, current_style));
                }
                lines.push(Line::from(spans));
            }
        }
    }
    lines
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

    let inner_width = chunks[0].width.saturating_sub(2).max(1);
    let inner_height = chunks[0].height.saturating_sub(2);

    // Output
    let output_lines: Vec<Line> = if let Some(art) = app.art_mode {
        draw_art(art, app.frame_count, inner_width, inner_height)
    } else {
        app.output
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
            .collect()
    };

    // Output Border Style
    let output_style = if app.busy {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Cyan)
    };

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

    let (scroll_y, wrap_widget) = if app.art_mode.is_some() {
        (0, false)
    } else {
        (actual_scroll, true)
    };

    let mut output_widget = Paragraph::new(output_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(output_style)
            .title(Span::styled(
                " kota ",
                Style::default().bold().fg(Color::Cyan),
            )),
    );
    if wrap_widget {
        output_widget = output_widget.wrap(Wrap { trim: false });
    }
    let output_widget = output_widget.scroll((scroll_y, 0));
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
        " | ↑ scrolled (PgDn/Ctrl+G to tail)".to_string()
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
