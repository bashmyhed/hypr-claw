use std::cmp::min;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::time::Duration;
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
    pub resources: String,
    pub background_task_id: String,
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
    pub live_mode: bool,
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
    pub action_feed_window: String,
    pub recent_messages_window: String,
    pub task_log: Vec<String>,
    pub task_log_window: String,
    pub actionable_ids: Vec<String>,
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

/// Minimum terminal width; below this we clamp to avoid broken layout.
const MIN_TERMINAL_WIDTH: usize = 60;
/// Maximum terminal width; above this we clamp to keep line length readable.
const MAX_TERMINAL_WIDTH: usize = 220;
/// Minimum lines for the body (two panels); ensures something is visible on small terminals.
const MIN_BODY_LINES: usize = 6;
/// Header rows (borders + status lines) reserved above the panel body.
const HEADER_LINES: usize = 14;
/// Minimum width for the right panel (Decision Feed, Task Log, Controls).
const MIN_RIGHT_PANEL_WIDTH: usize = 22;
/// Minimum width for the left panel (Threads, Background Tasks, Supervisor Queue, Messages).
const MIN_LEFT_PANEL_WIDTH: usize = 26;

fn paint(text: &str, style: &str) -> String {
    if std::env::var_os("NO_COLOR").is_none() {
        format!("{style}{text}{RESET}")
    } else {
        text.to_string()
    }
}

pub fn run_command_center(
    snapshot: &TuiSnapshot,
    refresh_interval: Duration,
) -> io::Result<TuiOutcome> {
    render(snapshot)?;

    println!(
        "{}",
        paint(
            &command_prompt_hint(snapshot.live_mode, refresh_interval),
            DIM
        )
    );
    print!("{}", paint("tui> ", ACCENT));
    io::stdout().flush()?;

    let cmd = if snapshot.live_mode {
        match poll_line_with_timeout(refresh_interval)? {
            Some(cmd) => cmd,
            None => return Ok(TuiOutcome::Refresh),
        }
    } else {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        input.trim().to_string()
    };

    if cmd.is_empty() {
        return Ok(TuiOutcome::Refresh);
    }
    if cmd == ":q" || cmd == "/q" {
        return Ok(TuiOutcome::Close);
    }
    if cmd == ":exit" || cmd == "/exit" {
        return Ok(TuiOutcome::ExitApp);
    }
    if cmd == ":r" || cmd == ":refresh" || cmd == "/r" || cmd == "/refresh" {
        return Ok(TuiOutcome::Refresh);
    }
    if let Some(mapped) = map_tui_shortcut(&cmd) {
        return Ok(TuiOutcome::Submit(mapped));
    }

    Ok(TuiOutcome::Submit(cmd))
}

fn map_tui_shortcut(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }

    let direct = match value {
        ":help" | "/help" => Some("/help"),
        ":status" | "/status" => Some("/status"),
        ":dashboard" | ":dash" | "/dashboard" => Some("/dashboard"),
        ":tasks" | "/tasks" => Some("/tasks"),
        ":models" | "/models" => Some("/models"),
        ":queue" | "/queue" => Some("/queue"),
        ":queue-run" | "/queue-run" => Some("/queue run"),
        ":queue-status" | "/queue-status" => Some("/queue status"),
        ":queue-clear" | "/queue-clear" => Some("/queue clear"),
        ":queue-prune" | "/queue-prune" => Some("/queue prune"),
        ":queue-stop-all" | "/queue-stop-all" => Some("/queue stop all"),
        ":queue-retry-failed" | "/queue-retry-failed" => Some("/queue retry failed"),
        ":queue-retry-completed" | "/queue-retry-completed" => Some("/queue retry completed"),
        ":queue-retry-all" | "/queue-retry-all" => Some("/queue retry all"),
        ":queue-auto-on" | "/queue-auto-on" => Some("/queue auto on"),
        ":queue-auto-off" | "/queue-auto-off" => Some("/queue auto off"),
        ":live-on" | "/live-on" => Some("/tui live on"),
        ":live-off" | "/live-off" => Some("/tui live off"),
        ":feed-up" | "/feed-up" => Some("/tui feed up"),
        ":feed-down" | "/feed-down" => Some("/tui feed down"),
        ":feed-top" | "/feed-top" => Some("/tui feed top"),
        ":feed-new" | "/feed-new" => Some("/tui feed latest"),
        ":log-up" | "/log-up" => Some("/tui log up"),
        ":log-down" | "/log-down" => Some("/tui log down"),
        ":log-top" | "/log-top" => Some("/tui log top"),
        ":log-new" | "/log-new" => Some("/tui log latest"),
        ":msg-up" | "/msg-up" => Some("/tui msgs up"),
        ":msg-down" | "/msg-down" => Some("/tui msgs down"),
        ":msg-top" | "/msg-top" => Some("/tui msgs top"),
        ":msg-new" | "/msg-new" => Some("/tui msgs latest"),
        ":view-reset" | "/view-reset" => Some("/tui view reset"),
        ":soul-list" | "/soul-list" => Some("/soul list"),
        ":capabilities" | ":caps" | "/capabilities" => Some("/capabilities"),
        ":profile" | "/profile" => Some("/profile"),
        ":scan" | "/scan" => Some("/scan"),
        ":clear" | "/clear" => Some("/clear"),
        ":interrupt" | "/interrupt" => Some("/interrupt"),
        ":repl" | "/repl" => Some("/repl"),
        _ => None,
    };
    if let Some(id) = value
        .strip_prefix(":queue-stop ")
        .or_else(|| value.strip_prefix("/queue-stop "))
        .map(str::trim)
    {
        if !id.is_empty() {
            return Some(format!("/queue stop {}", id));
        }
    }
    if let Some(id) = value
        .strip_prefix(":queue-retry ")
        .or_else(|| value.strip_prefix("/queue-retry "))
        .map(str::trim)
    {
        if !id.is_empty() {
            return Some(format!("/queue retry {}", id));
        }
    }
    if let Some(count) = value
        .strip_prefix(":queue-prune ")
        .or_else(|| value.strip_prefix("/queue-prune "))
        .map(str::trim)
    {
        if !count.is_empty() {
            return Some(format!("/queue prune {}", count));
        }
    }
    direct.map(str::to_string)
}

fn command_prompt_hint(live_mode: bool, refresh_interval: Duration) -> String {
    if live_mode {
        format!(
            "Command Center Input: live refresh={}ms | :q close | :live-off disable live | Enter keeps latest view",
            refresh_interval.as_millis()
        )
    } else {
        "Command Center Input: type command and Enter | :q|/q close | :exit exit app | :r|/r refresh"
            .to_string()
    }
}

fn poll_line_with_timeout(timeout: Duration) -> io::Result<Option<String>> {
    if !stdin_ready(timeout)? {
        return Ok(None);
    }
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(Some(input.trim().to_string()))
}

#[cfg(unix)]
fn stdin_ready(timeout: Duration) -> io::Result<bool> {
    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }

    unsafe extern "C" {
        fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
    }

    const POLLIN: i16 = 0x0001;
    let timeout_ms_u128 = timeout.as_millis().min(i32::MAX as u128);
    let timeout_ms = timeout_ms_u128 as i32;
    let mut fd = PollFd {
        fd: io::stdin().as_raw_fd(),
        events: POLLIN,
        revents: 0,
    };

    let result = unsafe { poll(&mut fd as *mut PollFd, 1, timeout_ms) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(result > 0 && (fd.revents & POLLIN) != 0)
}

#[cfg(not(unix))]
fn stdin_ready(timeout: Duration) -> io::Result<bool> {
    std::thread::sleep(timeout);
    Ok(true)
}

fn render(snapshot: &TuiSnapshot) -> io::Result<()> {
    let (width, height) = terminal_size();
    let width = width.clamp(MIN_TERMINAL_WIDTH, MAX_TERMINAL_WIDTH);
    let body_lines = height.saturating_sub(HEADER_LINES).max(MIN_BODY_LINES);

    // Stable panel split: ensure both panels have at least minimum width so alignment holds.
    let content_width = width.saturating_sub(3); // two borders and one separator
    let preferred_left = ((content_width as f32) * 0.62) as usize;
    let left_w = preferred_left.clamp(
        MIN_LEFT_PANEL_WIDTH,
        content_width.saturating_sub(MIN_RIGHT_PANEL_WIDTH),
    );
    let right_w = content_width.saturating_sub(left_w);

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
                "Autonomy {}  TUI live {}",
                truncate(&snapshot.autonomy_mode, width.saturating_sub(26)),
                if snapshot.live_mode { "on" } else { "off" }
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
                "  {:<8} {:<6} {:<9} {:<12} {}",
                truncate(&task.id, 8),
                truncate(&task.state, 6),
                truncate(&task.resources, 9),
                truncate(&task.background_task_id, 12),
                truncate(&task.prompt, width.saturating_sub(30))
            ));
        }
    }

    out.push(String::new());
    out.push(format!(
        " Recent Messages [{}]",
        snapshot.recent_messages_window
    ));
    if snapshot.recent_messages.is_empty() {
        out.push("  no messages yet".to_string());
    } else {
        for msg in snapshot.recent_messages.iter().take(8) {
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
    out.push(format!(" Decision Feed [{}]", snapshot.action_feed_window));
    if snapshot.action_feed.is_empty() {
        out.push("  no tool calls yet".to_string());
    } else {
        for line in snapshot.action_feed.iter().take(10) {
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
    out.push("  :status :dash :tasks :caps".to_string());
    out.push("  :queue :queue-status :queue-run".to_string());
    out.push("  :queue-clear :queue-prune [N]".to_string());
    out.push("  :queue-stop-all :queue-retry-failed".to_string());
    out.push("  :queue-retry-completed :queue-retry-all".to_string());
    out.push("  :queue-stop <id> :queue-retry <id>".to_string());
    out.push("  :queue-auto-on/:queue-auto-off".to_string());
    out.push("  :live-on/:live-off".to_string());
    out.push("  :feed-up/:feed-down/:feed-top/:feed-new".to_string());
    out.push("  :log-up/:log-down/:log-top/:log-new".to_string());
    out.push("  :msg-up/:msg-down/:msg-top/:msg-new".to_string());
    out.push("  :interrupt :clear :repl".to_string());
    out.push("  /dashboard runtime dashboard".to_string());
    out.push("  /models   switch/list models".to_string());
    out.push("  autonomy <mode> switch mode".to_string());
    out.push("  type any command to execute".to_string());
    out.push(String::new());
    out.push(" Actionable IDs".to_string());
    if snapshot.actionable_ids.is_empty() {
        out.push("  no actionable supervisor ids".to_string());
    } else {
        for line in snapshot.actionable_ids.iter().take(6) {
            out.push(format!(
                "  {}",
                truncate(&sanitize_line(line), width.saturating_sub(2))
            ));
        }
    }
    out.push(String::new());
    out.push(format!(" Task Log [{}]", snapshot.task_log_window));
    if snapshot.task_log.is_empty() {
        out.push("  no task events yet".to_string());
    } else {
        for line in snapshot.task_log.iter().take(8) {
            out.push(format!(
                "  {}",
                truncate(&sanitize_line(line), width.saturating_sub(2))
            ));
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_shortcuts_map_to_repl_commands() {
        assert_eq!(map_tui_shortcut(":status").as_deref(), Some("/status"));
        assert_eq!(map_tui_shortcut(":dash").as_deref(), Some("/dashboard"));
        assert_eq!(
            map_tui_shortcut(":queue-run").as_deref(),
            Some("/queue run")
        );
        assert_eq!(
            map_tui_shortcut(":queue-status").as_deref(),
            Some("/queue status")
        );
        assert_eq!(
            map_tui_shortcut(":queue-prune").as_deref(),
            Some("/queue prune")
        );
        assert_eq!(
            map_tui_shortcut(":queue-prune 64").as_deref(),
            Some("/queue prune 64")
        );
        assert_eq!(
            map_tui_shortcut(":queue-stop-all").as_deref(),
            Some("/queue stop all")
        );
        assert_eq!(
            map_tui_shortcut(":queue-retry-failed").as_deref(),
            Some("/queue retry failed")
        );
        assert_eq!(
            map_tui_shortcut(":queue-retry-completed").as_deref(),
            Some("/queue retry completed")
        );
        assert_eq!(
            map_tui_shortcut(":queue-retry-all").as_deref(),
            Some("/queue retry all")
        );
        assert_eq!(
            map_tui_shortcut(":queue-auto-off").as_deref(),
            Some("/queue auto off")
        );
        assert_eq!(
            map_tui_shortcut(":queue-stop sup-3").as_deref(),
            Some("/queue stop sup-3")
        );
        assert_eq!(
            map_tui_shortcut(":queue-retry sup-3").as_deref(),
            Some("/queue retry sup-3")
        );
        assert_eq!(
            map_tui_shortcut(":feed-up").as_deref(),
            Some("/tui feed up")
        );
        assert_eq!(
            map_tui_shortcut(":msg-new").as_deref(),
            Some("/tui msgs latest")
        );
        assert_eq!(map_tui_shortcut(":log-up").as_deref(), Some("/tui log up"));
        assert_eq!(
            map_tui_shortcut(":log-new").as_deref(),
            Some("/tui log latest")
        );
        assert_eq!(
            map_tui_shortcut(":live-on").as_deref(),
            Some("/tui live on")
        );
        assert_eq!(
            map_tui_shortcut(":live-off").as_deref(),
            Some("/tui live off")
        );
        assert_eq!(map_tui_shortcut(":caps").as_deref(), Some("/capabilities"));
        assert_eq!(map_tui_shortcut(":repl").as_deref(), Some("/repl"));
        assert_eq!(map_tui_shortcut("open firefox"), None);
    }
}
