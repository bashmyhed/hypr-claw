use std::cmp::min;
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct ThreadRow {
    pub id: String,
    pub title: String,
    pub active: bool,
    pub archived: bool,
}

#[derive(Clone, Debug)]
pub struct TaskRow {
    pub id: String,
    pub state: String,
    pub progress_percent: u16,
    pub description: String,
}

#[derive(Clone, Debug)]
pub struct SupervisorTaskRow {
    pub id: String,
    pub state: String,
    pub class: String,
    pub prompt: String,
}

#[derive(Clone, Debug)]
pub struct TuiSnapshot {
    pub provider: String,
    pub model: String,
    pub user: String,
    pub session: String,
    pub soul: String,
    pub autonomy_mode: String,
    pub thread: String,
    pub souls_count: usize,
    pub task_counts: (usize, usize, usize, usize),
    pub supervisor_counts: (usize, usize, usize, usize, usize),
    pub supervisor_auto_run: bool,
    pub history_len: usize,
    pub facts_len: usize,
    pub approval_counts: (usize, usize),
    pub token_usage: (usize, usize, usize),
    pub plan: String,
    pub reliability: String,
    pub threads: Vec<ThreadRow>,
    pub tasks: Vec<TaskRow>,
    pub supervisor_tasks: Vec<SupervisorTaskRow>,
    pub action_feed: Vec<String>,
    pub recent_messages: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum TuiOutcome {
    Submit(String),
    Close,
    ExitApp,
    Refresh,
}

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const ACCENT: &str = "\x1b[38;5;39m";

fn paint(text: &str, style: &str) -> String {
    if std::env::var_os("NO_COLOR").is_none() {
        format!("{style}{text}{RESET}")
    } else {
        text.to_string()
    }
}

pub fn run_command_center(snapshot: &TuiSnapshot) -> io::Result<TuiOutcome> {
    render(snapshot)?;

    println!(
        "{}",
        paint(
            "Command Center Input: type command and Enter | :q|/q close | :exit exit app | :r|/r refresh",
            DIM
        )
    );
    print!("{}", paint("tui> ", ACCENT));
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let cmd = input.trim().to_string();

    if cmd.is_empty() || cmd == ":q" || cmd == "/q" {
        return Ok(TuiOutcome::Close);
    }
    if cmd == ":exit" || cmd == "/exit" {
        return Ok(TuiOutcome::ExitApp);
    }
    if cmd == ":r" || cmd == ":refresh" || cmd == "/r" || cmd == "/refresh" {
        return Ok(TuiOutcome::Refresh);
    }

    Ok(TuiOutcome::Submit(cmd))
}

fn render(snapshot: &TuiSnapshot) -> io::Result<()> {
    let (width, height) = terminal_size();
    let width = width.max(92);
    let body_lines = height.saturating_sub(14).max(14);
    let left_w = ((width as f32) * 0.64) as usize;
    let left_w = left_w.clamp(54, width.saturating_sub(32));
    let right_w = width.saturating_sub(left_w + 3);

    print!("\x1b[2J\x1b[H");

    println!("{}", border_top(width, " HYPR-CLAW COMMAND CENTER "));
    println!(
        "{}",
        row(
            width,
            &format!(
                "Provider {}  Model {}  Soul {}",
                truncate(&snapshot.provider, 18),
                truncate(&snapshot.model, 28),
                truncate(&snapshot.soul, 22)
            )
        )
    );
    println!(
        "{}",
        row(
            width,
            &format!(
                "User {}  Session {}  Thread {}",
                truncate(&snapshot.user, 26),
                truncate(&snapshot.session, 28),
                truncate(&snapshot.thread, 20)
            )
        )
    );
    println!(
        "{}",
        row(
            width,
            &format!(
                "Souls {}  Tasks t/r/d/f {}/{}/{}/{}  Tokens in/out/sess {}/{}/{}",
                snapshot.souls_count,
                snapshot.task_counts.0,
                snapshot.task_counts.1,
                snapshot.task_counts.2,
                snapshot.task_counts.3,
                snapshot.token_usage.0,
                snapshot.token_usage.1,
                snapshot.token_usage.2
            )
        )
    );
    println!(
        "{}",
        row(
            width,
            &format!(
                "Autonomy {}",
                truncate(&snapshot.autonomy_mode, width.saturating_sub(14))
            )
        )
    );
    println!(
        "{}",
        row(
            width,
            &format!(
                "Supervisor q/r/d/f/c {}/{}/{}/{}/{}  auto {}",
                snapshot.supervisor_counts.0,
                snapshot.supervisor_counts.1,
                snapshot.supervisor_counts.2,
                snapshot.supervisor_counts.3,
                snapshot.supervisor_counts.4,
                if snapshot.supervisor_auto_run {
                    "on"
                } else {
                    "off"
                }
            )
        )
    );
    println!(
        "{}",
        row(
            width,
            &format!(
                "Memory history={} facts={} approvals={}/{}  Plan {}",
                snapshot.history_len,
                snapshot.facts_len,
                snapshot.approval_counts.0,
                snapshot.approval_counts.1,
                truncate(&snapshot.plan, width.saturating_sub(56))
            )
        )
    );
    println!("{}", divider(width, " LIVE STATE "));

    let left_lines = build_left_panel(snapshot, left_w, body_lines);
    let right_lines = build_right_panel(snapshot, right_w, body_lines);

    for i in 0..body_lines {
        let left = left_lines.get(i).map(String::as_str).unwrap_or("");
        let right = right_lines.get(i).map(String::as_str).unwrap_or("");
        let left_pad = pad_plain(left, left_w);
        let right_pad = pad_plain(right, right_w);
        println!("║{}│{}║", left_pad, right_pad);
    }

    println!("{}", border_bottom(width));
    io::stdout().flush()?;
    Ok(())
}

fn truncate(value: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

fn progress_bar(percent: u16, width: usize) -> String {
    if width == 0 {
        return "[]".to_string();
    }
    let filled = ((percent as f32 / 100.0) * width as f32).round() as usize;
    let filled = filled.min(width);
    format!(
        "[{}{}]",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    )
}

fn pad_state(state: &str, width: usize) -> String {
    format!("{state:<width$}")
}

fn terminal_size() -> (usize, usize) {
    let width = std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(120);
    let height = std::env::var("LINES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(36);
    (width, height)
}

fn border_top(width: usize, title: &str) -> String {
    let mut line = String::from("╔");
    let inner = width.saturating_sub(2);
    let label = truncate(title, inner.saturating_sub(2));
    let left = (inner.saturating_sub(label.len())) / 2;
    let right = inner.saturating_sub(label.len() + left);
    line.push_str(&"═".repeat(left));
    line.push_str(&paint(&label, ACCENT));
    line.push_str(&"═".repeat(right));
    line.push('╗');
    line
}

fn border_bottom(width: usize) -> String {
    format!("╚{}╝", "═".repeat(width.saturating_sub(2)))
}

fn divider(width: usize, label: &str) -> String {
    let inner = width.saturating_sub(2);
    let label = truncate(label, inner.saturating_sub(2));
    let left = (inner.saturating_sub(label.len())) / 2;
    let right = inner.saturating_sub(label.len() + left);
    format!(
        "╠{}{}{}╣",
        "═".repeat(left),
        paint(&label, ACCENT),
        "═".repeat(right)
    )
}

fn row(width: usize, text: &str) -> String {
    format!("║{}║", pad_plain(text, width.saturating_sub(2)))
}

fn pad_plain(value: &str, width: usize) -> String {
    let clipped = truncate_visible(value, width);
    let visible = visible_len(&clipped);
    let mut out = clipped;
    if visible < width {
        out.push_str(&" ".repeat(width - visible));
    }
    out
}

fn build_left_panel(snapshot: &TuiSnapshot, width: usize, max_lines: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(max_lines);
    out.push(" Threads".to_string());
    if snapshot.threads.is_empty() {
        out.push("  no threads".to_string());
    } else {
        for thread in snapshot.threads.iter().take(6) {
            let marker = if thread.active {
                "●"
            } else if thread.archived {
                "◌"
            } else {
                "○"
            };
            let state = if thread.archived { "arch" } else { "live" };
            out.push(format!(
                "  {} {:<12} [{}] {}",
                marker,
                truncate(&thread.id, 12),
                state,
                truncate(&thread.title, width.saturating_sub(26))
            ));
        }
    }

    out.push(String::new());
    out.push(" Background Tasks".to_string());
    if snapshot.tasks.is_empty() {
        out.push("  no background tasks".to_string());
    } else {
        for task in snapshot.tasks.iter().take(8) {
            let state = render_task_state(&task.state);
            let bar_w = min(12, width.saturating_sub(42)).max(6);
            out.push(format!(
                "  {} {:>3}% {} {}",
                pad_state(&state, 12),
                task.progress_percent,
                progress_bar(task.progress_percent, bar_w),
                truncate(&task.description, width.saturating_sub(30))
            ));
        }
    }

    out.push(String::new());
    out.push(format!(
        " Supervisor Queue (auto:{})",
        if snapshot.supervisor_auto_run {
            "on"
        } else {
            "off"
        }
    ));
    if snapshot.supervisor_tasks.is_empty() {
        out.push("  no queued/running supervisor tasks".to_string());
    } else {
        for task in snapshot.supervisor_tasks.iter().take(6) {
            out.push(format!(
                "  {:<8} {:<6} {:<4} {}",
                truncate(&task.id, 8),
                truncate(&task.state, 6),
                truncate(&task.class, 4),
                truncate(&task.prompt, width.saturating_sub(25))
            ));
        }
    }

    out.push(String::new());
    out.push(" Recent Messages".to_string());
    if snapshot.recent_messages.is_empty() {
        out.push("  no messages yet".to_string());
    } else {
        for msg in snapshot.recent_messages.iter().rev().take(5).rev() {
            out.push(format!(
                "  {}",
                truncate(&sanitize_line(msg), width.saturating_sub(2))
            ));
        }
    }

    out.truncate(max_lines);
    while out.len() < max_lines {
        out.push(String::new());
    }
    out
}

fn build_right_panel(snapshot: &TuiSnapshot, width: usize, max_lines: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(max_lines);
    out.push(" Decision Feed".to_string());
    if snapshot.action_feed.is_empty() {
        out.push("  no tool calls yet".to_string());
    } else {
        for line in snapshot.action_feed.iter().rev().take(10).rev() {
            out.push(format!(
                "  {}",
                truncate(&sanitize_line(line), width.saturating_sub(2))
            ));
        }
    }

    out.push(String::new());
    out.push(" Controls".to_string());
    out.push("  :q,/q   close command center".to_string());
    out.push("  :exit   quit hypr-claw".to_string());
    out.push("  :r,/r   refresh panels".to_string());
    out.push("  autonomy <mode> switch mode".to_string());
    out.push("  type any command to execute".to_string());
    out.push(String::new());
    out.push(" Queue".to_string());
    out.push("  queue              show queue".to_string());
    out.push("  queue run          run next queued".to_string());
    out.push("  queue auto on|off  toggle auto".to_string());
    out.push(String::new());
    out.push(" Session".to_string());
    out.push(format!("  now: {}", unix_now_compact()));
    out.push(format!(
        "  rel: {}",
        truncate(&snapshot.reliability, width.saturating_sub(8))
    ));
    out.push(format!(
        "  model: {}",
        truncate(&snapshot.model, width.saturating_sub(10))
    ));
    out.push(format!(
        "  soul:  {}",
        truncate(&snapshot.soul, width.saturating_sub(10))
    ));

    out.truncate(max_lines);
    while out.len() < max_lines {
        out.push(String::new());
    }
    out
}

fn sanitize_line(value: &str) -> String {
    value
        .replace('\n', " ")
        .replace('\r', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn unix_now_compact() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => format!("unix:{}", d.as_secs()),
        Err(_) => "unix:0".to_string(),
    }
}

fn render_task_state(state: &str) -> String {
    let normalized = state.to_ascii_uppercase();
    match normalized.as_str() {
        "RUNNING" => "RUNNING".to_string(),
        "COMPLETED" => "DONE".to_string(),
        "FAILED" => "FAILED".to_string(),
        "PENDING" => "PENDING".to_string(),
        other => other.to_string(),
    }
}

fn visible_len(input: &str) -> usize {
    let mut visible = 0usize;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            let _ = chars.next();
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
            continue;
        }
        visible += 1;
    }
    visible
}

fn truncate_visible(input: &str, max_visible: usize) -> String {
    if max_visible == 0 {
        return String::new();
    }
    if visible_len(input) <= max_visible {
        return input.to_string();
    }

    let keep = max_visible.saturating_sub(3);
    let mut out = String::new();
    let mut visible = 0usize;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            out.push(ch);
            if let Some(next) = chars.next() {
                out.push(next);
            }
            for c in chars.by_ref() {
                out.push(c);
                if c == 'm' {
                    break;
                }
            }
            continue;
        }
        if visible >= keep {
            break;
        }
        out.push(ch);
        visible += 1;
    }

    out.push_str("...");
    out
}
