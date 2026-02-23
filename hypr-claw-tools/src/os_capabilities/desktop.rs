//! Desktop operations - launching apps, browser ops, and GUI automation.

use super::{OsError, OsResult};
use serde_json::Value;
use tokio::process::Command;
use tokio::time::{sleep, Duration};

fn validate_app_name(app: &str) -> OsResult<()> {
    if app.trim().is_empty() {
        return Err(OsError::InvalidArgument("app cannot be empty".to_string()));
    }
    if !app
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(OsError::InvalidArgument(
            "app contains invalid characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_arg(arg: &str) -> OsResult<()> {
    if arg.contains('\0') || arg.contains('\n') {
        return Err(OsError::InvalidArgument(
            "argument contains invalid control characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_text(text: &str) -> OsResult<()> {
    if text.contains('\0') {
        return Err(OsError::InvalidArgument(
            "text contains null byte".to_string(),
        ));
    }
    Ok(())
}

fn validate_key_token(key: &str) -> OsResult<()> {
    if key.trim().is_empty() {
        return Err(OsError::InvalidArgument("key cannot be empty".to_string()));
    }
    if !key
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '+' | ' '))
    {
        return Err(OsError::InvalidArgument(
            "key contains invalid characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_coordinate(value: i32, label: &str) -> OsResult<()> {
    if value < 0 {
        return Err(OsError::InvalidArgument(format!(
            "{label} must be >= 0, got {value}"
        )));
    }
    Ok(())
}

fn validate_url(url: &str) -> OsResult<()> {
    if url.trim().is_empty() {
        return Err(OsError::InvalidArgument("url cannot be empty".to_string()));
    }
    let lower = url.to_lowercase();
    if !(lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("file://"))
    {
        return Err(OsError::InvalidArgument(
            "url must start with http://, https://, mailto:, or file://".to_string(),
        ));
    }
    Ok(())
}

fn encode_query(query: &str) -> String {
    query
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn command_exists(command: &str) -> bool {
    Command::new("which")
        .arg(command)
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

async fn run_checked(command: &str, args: &[&str]) -> OsResult<()> {
    let output = Command::new(command).args(args).output().await?;
    if output.status.success() {
        return Ok(());
    }
    Err(OsError::OperationFailed(
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

async fn run_output(command: &str, args: &[&str]) -> OsResult<String> {
    let output = Command::new(command).args(args).output().await?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    Err(OsError::OperationFailed(
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Launch an app directly.
pub async fn launch_app(app: &str, args: &[String]) -> OsResult<u32> {
    validate_app_name(app)?;
    for arg in args {
        validate_arg(arg)?;
    }

    let child = Command::new(app).args(args).spawn().map_err(OsError::Io)?;
    Ok(child.id().unwrap_or_default())
}

/// Open a URL with xdg-open.
pub async fn open_url(url: &str) -> OsResult<()> {
    validate_url(url)?;
    Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map_err(OsError::Io)?;
    Ok(())
}

/// Search the web with a selected search engine.
pub async fn search_web(query: &str, engine: Option<&str>) -> OsResult<String> {
    let encoded = encode_query(query);
    let target = match engine.unwrap_or("duckduckgo").to_lowercase().as_str() {
        "google" => format!("https://www.google.com/search?q={encoded}"),
        "bing" => format!("https://www.bing.com/search?q={encoded}"),
        _ => format!("https://duckduckgo.com/?q={encoded}"),
    };
    open_url(&target).await?;
    Ok(target)
}

/// Open Gmail in default browser.
pub async fn open_gmail() -> OsResult<()> {
    open_url("https://mail.google.com").await
}

/// Type text into the currently focused window.
pub async fn type_text(text: &str) -> OsResult<()> {
    validate_text(text)?;
    if command_exists("wtype").await {
        return run_checked("wtype", &[text]).await;
    }
    if command_exists("ydotool").await {
        return run_checked("ydotool", &["type", text]).await;
    }
    Err(OsError::OperationFailed(
        "No text input backend found (install 'wtype' or 'ydotool')".to_string(),
    ))
}

/// Press a single key in the focused window.
pub async fn key_press(key: &str) -> OsResult<()> {
    validate_key_token(key)?;
    if !command_exists("wtype").await {
        return Err(OsError::OperationFailed(
            "wtype not found for key presses".to_string(),
        ));
    }
    run_checked("wtype", &["-k", key]).await
}

/// Press a key combination (modifiers + key), e.g. ctrl+l.
pub async fn key_combo(keys: &[String]) -> OsResult<()> {
    if keys.len() < 2 {
        return Err(OsError::InvalidArgument(
            "key_combo requires at least one modifier and one key".to_string(),
        ));
    }
    if !command_exists("wtype").await {
        return Err(OsError::OperationFailed(
            "wtype not found for key combos".to_string(),
        ));
    }

    for key in keys {
        validate_key_token(key)?;
    }

    let main_key = keys.last().cloned().unwrap_or_default();
    let modifiers = &keys[..keys.len() - 1];

    let mut args: Vec<String> = Vec::new();
    for modifier in modifiers {
        args.push("-M".to_string());
        args.push(modifier.to_string());
    }
    args.push("-k".to_string());
    args.push(main_key);
    for modifier in modifiers.iter().rev() {
        args.push("-m".to_string());
        args.push(modifier.to_string());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_checked("wtype", &arg_refs).await
}

fn parse_mouse_button(button: &str) -> OsResult<&'static str> {
    match button.to_lowercase().as_str() {
        "left" => Ok("1"),
        "middle" => Ok("2"),
        "right" => Ok("3"),
        other => Err(OsError::InvalidArgument(format!(
            "unsupported mouse button: {other}"
        ))),
    }
}

/// Click mouse button in current cursor position.
pub async fn mouse_click(button: &str) -> OsResult<()> {
    let code = parse_mouse_button(button)?;
    if command_exists("ydotool").await {
        return run_checked("ydotool", &["click", code]).await;
    }
    if command_exists("wlrctl").await {
        return run_checked("wlrctl", &["pointer", "click", button]).await;
    }
    Err(OsError::OperationFailed(
        "No click backend found (install 'ydotool' or 'wlrctl')".to_string(),
    ))
}

/// Move cursor to absolute coordinate.
pub async fn mouse_move_absolute(x: i32, y: i32) -> OsResult<()> {
    validate_coordinate(x, "x")?;
    validate_coordinate(y, "y")?;

    let xs = x.to_string();
    let ys = y.to_string();

    if command_exists("wlrctl").await {
        return run_checked("wlrctl", &["pointer", "move", &xs, &ys]).await;
    }
    if command_exists("ydotool").await {
        // ydotool mousemove supports absolute mode on newer versions.
        return run_checked("ydotool", &["mousemove", "--absolute", &xs, &ys]).await;
    }
    Err(OsError::OperationFailed(
        "No mouse move backend found (install 'wlrctl' or 'ydotool')".to_string(),
    ))
}

/// Click at absolute coordinate.
pub async fn click_at(x: i32, y: i32, button: &str) -> OsResult<()> {
    mouse_move_absolute(x, y).await?;
    // Small delay lets compositor settle cursor move before click.
    sleep(Duration::from_millis(30)).await;
    mouse_click(button).await
}

/// Capture current screen to file and return saved path.
pub async fn capture_screen(path: Option<&str>) -> OsResult<String> {
    let target = path
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("/tmp/hypr-claw-shot-{}.png", chrono::Utc::now().timestamp()));

    if command_exists("grim").await {
        run_checked("grim", &[&target]).await?;
        return Ok(target);
    }
    if command_exists("hyprshot").await {
        // hyprshot -m output prints path, but we pass explicit output path.
        run_checked("hyprshot", &["-m", "output", "-o", &target]).await?;
        return Ok(target);
    }
    Err(OsError::OperationFailed(
        "No screenshot backend found (install 'grim' or 'hyprshot')".to_string(),
    ))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OcrMatch {
    pub text: String,
    pub confidence: f32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub center_x: i32,
    pub center_y: i32,
}

fn parse_tesseract_tsv(tsv: &str) -> Vec<OcrMatch> {
    let mut matches = Vec::new();
    for (idx, line) in tsv.lines().enumerate() {
        if idx == 0 {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 12 {
            continue;
        }
        let raw_text = cols[11].trim();
        if raw_text.is_empty() {
            continue;
        }
        let conf = cols[10].parse::<f32>().unwrap_or(-1.0);
        if conf < 0.0 {
            continue;
        }
        let x = cols[6].parse::<i32>().unwrap_or(-1);
        let y = cols[7].parse::<i32>().unwrap_or(-1);
        let width = cols[8].parse::<i32>().unwrap_or(0);
        let height = cols[9].parse::<i32>().unwrap_or(0);
        if x < 0 || y < 0 || width <= 0 || height <= 0 {
            continue;
        }
        matches.push(OcrMatch {
            text: raw_text.to_string(),
            confidence: conf,
            x,
            y,
            width,
            height,
            center_x: x + width / 2,
            center_y: y + height / 2,
        });
    }
    matches
}

/// OCR current screen and return recognized words with boxes.
pub async fn ocr_screen(
    path: Option<&str>,
    lang: Option<&str>,
) -> OsResult<(String, Vec<OcrMatch>)> {
    let image_path = if let Some(path) = path {
        path.to_string()
    } else {
        capture_screen(None).await?
    };

    if !command_exists("tesseract").await {
        return Err(OsError::OperationFailed(
            "tesseract not found (install 'tesseract' package)".to_string(),
        ));
    }

    let mut args = vec![image_path.as_str(), "stdout", "tsv"];
    if let Some(lang) = lang {
        args.push("-l");
        args.push(lang);
    }
    let tsv = run_output("tesseract", &args).await?;
    let words = parse_tesseract_tsv(&tsv);
    let full_text = words
        .iter()
        .map(|m| m.text.clone())
        .collect::<Vec<String>>()
        .join(" ");
    Ok((full_text, words))
}

/// Find text matches from OCR output.
pub async fn find_text(
    query: &str,
    case_sensitive: bool,
    limit: usize,
    lang: Option<&str>,
) -> OsResult<Vec<OcrMatch>> {
    if query.trim().is_empty() {
        return Err(OsError::InvalidArgument(
            "query cannot be empty".to_string(),
        ));
    }

    let (_, words) = ocr_screen(None, lang).await?;
    let mut found = Vec::new();
    for word in words {
        let hit = if case_sensitive {
            word.text.contains(query)
        } else {
            word.text.to_lowercase().contains(&query.to_lowercase())
        };
        if hit {
            found.push(word);
            if limit > 0 && found.len() >= limit {
                break;
            }
        }
    }
    Ok(found)
}

/// Find text on screen and click its center.
pub async fn click_text(
    query: &str,
    occurrence: usize,
    button: &str,
    case_sensitive: bool,
    lang: Option<&str>,
) -> OsResult<OcrMatch> {
    let matches = find_text(query, case_sensitive, occurrence + 1, lang).await?;
    if matches.len() <= occurrence {
        return Err(OsError::NotFound(format!(
            "text '{}' not found on screen",
            query
        )));
    }
    let target = matches[occurrence].clone();
    click_at(target.center_x, target.center_y, button).await?;
    Ok(target)
}

/// Return current active window metadata from Hyprland.
pub async fn active_window() -> OsResult<Value> {
    let output = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output()
        .await?;
    if !output.status.success() {
        return Err(OsError::OperationFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    let json: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| OsError::OperationFailed(e.to_string()))?;
    Ok(json)
}

/// List Hyprland windows (clients) metadata.
pub async fn list_windows(limit: usize) -> OsResult<Vec<Value>> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .await?;
    if !output.status.success() {
        return Err(OsError::OperationFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    let mut json: Vec<Value> = serde_json::from_slice(&output.stdout)
        .map_err(|e| OsError::OperationFailed(e.to_string()))?;
    if limit > 0 && json.len() > limit {
        json.truncate(limit);
    }
    Ok(json)
}
