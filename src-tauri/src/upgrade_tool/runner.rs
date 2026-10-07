use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

const EVENT_TOOL_LOG: &str = "tool-log";

#[derive(Debug, Clone, Serialize)]
pub struct LogPayload {
    pub text: String,
    pub level: String,
    #[serde(rename = "update")]
    pub in_place: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RockusbDevice {
    pub location_id: String,
    pub mode: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub version: String,
    pub platform_dir: String,
    pub tool_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    pub success: bool,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentStorageInfo {
    pub no: u32,
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DownloadRowPayload {
    pub enabled: bool,
    pub storage: String,
    pub address: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DownloadExecutePayload {
    pub rows: Vec<DownloadRowPayload>,
    pub force_by_address: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ActionParams {
    pub boot_path: Option<String>,
    pub start_sector: Option<String>,
    pub sector_count: Option<String>,
    pub output_path: Option<String>,
}

fn platform_dir_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(target_os = "windows") {
        "windows_x86-64"
    } else {
        "linux_x86-64"
    }
}

fn resolve_tool_paths(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let sub = platform_dir_name();
    let work_dir = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("bin")
            .join(sub)
    } else {
        app.path()
            .resource_dir()
            .map_err(|e| e.to_string())?
            .join("bin")
            .join(sub)
    };

    let tool_name = if cfg!(target_os = "windows") {
        "upgrade_tool.exe"
    } else {
        "upgrade_tool"
    };

    let tool_path = work_dir.join(tool_name);
    if !tool_path.exists() {
        return Err(format!("upgrade_tool not found: {}", tool_path.display()));
    }

    Ok((tool_path, work_dir))
}

pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{001b}' {
            while chars.next().is_some() {
                if chars.peek() == Some(&'m') {
                    chars.next();
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out.trim().to_string()
}

fn log_level(line: &str) -> &'static str {
    let lower = line.to_ascii_lowercase();
    if lower.contains("success") || lower.contains("成功") {
        "success"
    } else if lower.contains("error") || lower.contains("fail") || lower.contains("失败") {
        "error"
    } else if lower.contains("warn") || lower.contains("warning") {
        "info"
    } else {
        "default"
    }
}

fn is_progress_line(line: &str) -> bool {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();

    // 步骤完成行单独输出，不做原地刷新
    if lower.ends_with(" success")
        || lower.ends_with(" fail")
        || lower.ends_with(" failed")
        || lower.contains("成功")
        || lower.contains("失败")
    {
        return false;
    }
    if lower.starts_with("start to ") || lower.starts_with("begin ") {
        return false;
    }

    trimmed.contains('%')
        || trimmed.ends_with("...")
        || lower.contains("progress")
}

fn last_output_line(output: &str) -> Option<&str> {
    let trimmed = output.trim_end_matches('\n');
    if trimmed.is_empty() {
        return None;
    }
    let start = trimmed.rfind('\n').map(|p| p + 1).unwrap_or(0);
    Some(trimmed[start..].trim())
}

fn push_output_line(output: &mut String, line: &str, replace_last: bool) {
    let replace_last = replace_last
        && last_output_line(output)
            .is_some_and(is_progress_line);

    if replace_last {
        if let Some(pos) = output.rfind('\n') {
            let line_start = output[..pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
            output.truncate(line_start);
        } else {
            output.clear();
        }
    }
    output.push_str(line);
    output.push('\n');
}

fn uf_output_success(output: &str) -> bool {
    let lower = strip_ansi(output).to_ascii_lowercase();
    lower.contains("upgrade firmware ok")
        || lower.contains("upgrade firmware success")
        || lower.contains("download firmware success")
}

fn file_size_or_zero(path: &str) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Sum payload bytes for write/flash commands (WL, UF, DI, …).
fn write_payload_bytes(args: &[String]) -> u64 {
    match args.first().map(String::as_str) {
        Some("WL") => args.get(2).map(|p| file_size_or_zero(p)).unwrap_or(0),
        Some("DB") | Some("UL") | Some("UF") => {
            args.get(1).map(|p| file_size_or_zero(p)).unwrap_or(0)
        }
        Some("DI") => {
            let mut total = 0u64;
            let mut iter = args.iter().skip(1);
            while let Some(flag) = iter.next() {
                if flag.starts_with('-') {
                    if let Some(path) = iter.next() {
                        total = total.saturating_add(file_size_or_zero(path));
                    }
                }
            }
            total
        }
        _ => 0,
    }
}

/// Timeout from image size (~5 MiB/s effective transfer + 120s base, 3 min–2 h).
fn timeout_for_bytes(bytes: u64) -> Duration {
    const MIN_SECS: u64 = 180;
    const MAX_SECS: u64 = 7200;
    const BASE_SECS: u64 = 120;
    const BYTES_PER_SEC: u64 = 5 * 1024 * 1024;

    if bytes == 0 {
        return Duration::from_secs(MIN_SECS);
    }

    let transfer_secs = bytes.div_ceil(BYTES_PER_SEC);
    let total = BASE_SECS.saturating_add(transfer_secs).clamp(MIN_SECS, MAX_SECS);
    Duration::from_secs(total)
}

fn command_timeout(args: &[String]) -> Duration {
    let write_bytes = write_payload_bytes(args);
    if write_bytes > 0 {
        return timeout_for_bytes(write_bytes);
    }

    match args.first().map(String::as_str) {
        Some("UF") | Some("DI") => Duration::from_secs(900),
        Some("DB") | Some("UL") | Some("EF") => Duration::from_secs(300),
        _ => Duration::from_secs(180),
    }
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> Result<std::process::ExitStatus, String> {
    let start = Instant::now();
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => return Ok(status),
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Operation timed out after {} seconds; re-enter Maskrom and check the USB connection, then retry",
                    timeout.as_secs()
                ));
            }
            None => thread::sleep(Duration::from_millis(200)),
        }
    }
}

fn emit_log(app: &AppHandle, text: &str, in_place: bool) {
    if text.is_empty() {
        return;
    }
    let _ = app.emit(
        EVENT_TOOL_LOG,
        LogPayload {
            text: text.to_string(),
            level: log_level(text).to_string(),
            in_place,
        },
    );
}

/// 模拟终端单行缓冲，正确处理 `\r` 与 ANSI 光标移动，避免进度行被截断。
struct TerminalLineBuffer {
    chars: Vec<char>,
    cursor: usize,
    parse: AnsiParseState,
}

enum AnsiParseState {
    Normal,
    Escape,
    Csi { params: String },
}

impl TerminalLineBuffer {
    fn new() -> Self {
        Self {
            chars: Vec::new(),
            cursor: 0,
            parse: AnsiParseState::Normal,
        }
    }

    fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
        self.parse = AnsiParseState::Normal;
    }

    fn line(&self) -> String {
        self.chars.iter().collect()
    }

    fn carriage_return(&mut self) {
        self.cursor = 0;
    }

    fn clear_line(&mut self) {
        self.chars.clear();
        self.cursor = 0;
    }

    fn clear_to_eol(&mut self) {
        self.chars.truncate(self.cursor);
    }

    fn set_cursor_col_1based(&mut self, col: usize) {
        let idx = col.saturating_sub(1);
        self.cursor = idx;
        while self.chars.len() < idx {
            self.chars.push(' ');
        }
    }

    fn cursor_forward(&mut self, n: usize) {
        self.cursor += n;
        while self.chars.len() < self.cursor {
            self.chars.push(' ');
        }
    }

    fn put_char(&mut self, ch: char) {
        if ch == '\t' {
            let next_tab = (self.cursor + 8) & !7;
            while self.chars.len() < next_tab {
                self.chars.push(' ');
            }
            self.cursor = next_tab;
            return;
        }
        if self.cursor < self.chars.len() {
            self.chars[self.cursor] = ch;
        } else if self.cursor == self.chars.len() {
            self.chars.push(ch);
        } else {
            while self.chars.len() < self.cursor {
                self.chars.push(' ');
            }
            self.chars.push(ch);
        }
        self.cursor += 1;
    }

    fn feed_byte(&mut self, byte: u8) {
        match &mut self.parse {
            AnsiParseState::Normal => match byte {
                b'\r' => self.carriage_return(),
                b'\n' => {}
                0x1b => self.parse = AnsiParseState::Escape,
                0x09 | 0x20..=0x7e => self.put_char(byte as char),
                _ => {}
            },
            AnsiParseState::Escape => {
                if byte == b'[' {
                    self.parse = AnsiParseState::Csi {
                        params: String::new(),
                    };
                } else {
                    self.parse = AnsiParseState::Normal;
                }
            }
            AnsiParseState::Csi { params } => {
                if byte.is_ascii_digit() || byte == b';' {
                    params.push(byte as char);
                } else {
                    let params_copy = params.clone();
                    self.parse = AnsiParseState::Normal;
                    self.dispatch_csi(&params_copy, byte);
                }
            }
        }
    }

    fn dispatch_csi(&mut self, params: &str, cmd: u8) {
        let nums: Vec<u32> = if params.is_empty() {
            vec![0]
        } else {
            params
                .split(';')
                .map(|s| s.parse().unwrap_or(0))
                .collect()
        };

        match cmd {
            b'G' => self.set_cursor_col_1based(*nums.first().unwrap_or(&1) as usize),
            b'C' => self.cursor_forward(*nums.first().unwrap_or(&1) as usize),
            b'K' => match nums.first().copied().unwrap_or(0) {
                1 => {
                    let tail: String = self.chars[self.cursor..].iter().collect();
                    self.chars.truncate(self.cursor);
                    self.chars.splice(0..0, tail.chars());
                }
                2 => self.clear_line(),
                _ => self.clear_to_eol(),
            },
            b'H' | b'f' => {
                if let Some(&col) = nums.get(1) {
                    self.set_cursor_col_1based(col as usize);
                } else if let Some(&col) = nums.first() {
                    self.set_cursor_col_1based(col as usize);
                }
            }
            b'm' => {}
            _ => {}
        }
    }

    #[cfg(test)]
    fn feed_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if byte == b'\n' {
                continue;
            }
            self.feed_byte(byte);
        }
    }
}

struct StreamLineReader {
    line: TerminalLineBuffer,
    output: String,
    app: AppHandle,
    silent: bool,
    last_progress_emitted: String,
}

impl StreamLineReader {
    fn new(app: AppHandle, silent: bool) -> Self {
        Self {
            line: TerminalLineBuffer::new(),
            output: String::new(),
            app,
            silent,
            last_progress_emitted: String::new(),
        }
    }

    fn emit_line(&mut self, text: &str, in_place: bool) {
        let line = strip_ansi(text);
        if line.is_empty() {
            return;
        }

        let progress = is_progress_line(&line);
        if in_place && progress {
            if line == self.last_progress_emitted {
                return;
            }
            self.last_progress_emitted = line.clone();
            push_output_line(&mut self.output, &line, true);
            if !self.silent {
                emit_log(&self.app, &line, true);
            }
            return;
        }

        self.last_progress_emitted.clear();
        push_output_line(&mut self.output, &line, false);
        if !self.silent {
            emit_log(&self.app, &line, false);
        }
    }

    fn on_newline(&mut self) {
        let text = self.line.line();
        if is_progress_line(&text) && text == self.last_progress_emitted {
            self.last_progress_emitted.clear();
            self.line.clear();
            return;
        }
        self.emit_line(&text, false);
        self.line.clear();
    }

    fn on_progress_tick(&mut self) {
        let text = self.line.line();
        if is_progress_line(&text) {
            self.emit_line(&text, true);
        }
    }

    fn feed_byte(&mut self, byte: u8) {
        if byte == b'\n' {
            self.on_newline();
            return;
        }
        self.line.feed_byte(byte);
        if is_progress_line(&self.line.line()) {
            self.on_progress_tick();
        }
    }

    fn finish(mut self) -> String {
        let tail = self.line.line();
        if !tail.is_empty() {
            let in_place = is_progress_line(&tail);
            self.emit_line(&tail, in_place);
        }
        self.output
    }
}

fn read_tool_stream(
    mut stream: impl Read + Send + 'static,
    app: AppHandle,
    silent: bool,
) -> Result<String, String> {
    let mut reader = StreamLineReader::new(app, silent);
    let mut buf = [0u8; 8192];

    loop {
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        for &byte in &buf[..n] {
            reader.feed_byte(byte);
        }
    }

    Ok(reader.finish())
}

fn output_has_error(output: &str) -> bool {
    for line in output.lines() {
        let line = strip_ansi(line);
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.contains("fail")
            || lower.contains("error")
            || lower.contains("失败")
            || lower.contains("invalid argument")
        {
            return true;
        }
    }
    false
}

fn command_matches_success(args: &[String], output: &str, exit_ok: bool) -> bool {
    match args.first().map(|s| s.as_str()) {
        Some("UF") => {
            if uf_output_success(output) {
                return true;
            }
            exit_ok && !output_has_error(output)
        }
        Some("DB") | Some("UL") => exit_ok && !output_has_error(output),
        Some("DI") => {
            if !exit_ok || output_has_error(output) {
                return false;
            }
            let lower = strip_ansi(output).to_ascii_lowercase();
            lower.contains("success") || lower.contains("成功")
        }
        _ => exit_ok && !output_has_error(output),
    }
}

#[cfg(target_os = "linux")]
fn shell_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "./-_:@/".contains(c))
    {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

#[cfg(target_os = "linux")]
fn shell_join(program: &Path, args: &[String]) -> String {
    let mut line = shell_quote(&program.display().to_string());
    for arg in args {
        line.push(' ');
        line.push_str(&shell_quote(arg));
    }
    line
}

fn tool_argv(device_id: Option<&str>, args: &[String]) -> Vec<String> {
    let mut tool_args = Vec::with_capacity(args.len() + 2);
    if let Some(id) = device_id.filter(|id| !id.is_empty()) {
        tool_args.push(String::from("-s"));
        tool_args.push(id.to_string());
    }
    tool_args.extend(args.iter().cloned());
    tool_args
}

fn device_arg_for_tool(state: &State<'_, AppState>, selected: Option<String>) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        // The Windows UI selection is a SetupAPI device-interface path for CreateFileW.
        // `upgrade_tool -s` expects its own numeric LocationID, so passing this path would be
        // invalid. RockUSB actions use the driver transport; remaining tool actions rescan.
        let _ = (state, selected);
        return None;
    }

    #[cfg(not(target_os = "windows"))]
    pick_device_arg(
        state
            .last_devices
            .lock()
            .map(|devices| devices.len())
            .unwrap_or(0),
        selected,
    )
}

fn pick_device_arg(device_count: usize, selected: Option<String>) -> Option<String> {
    if device_count <= 1 {
        None
    } else {
        selected
    }
}

#[cfg(windows)]
fn apply_windows_hidden(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

enum SpawnedTool {
    Pipe {
        child: Child,
        stdout: std::process::ChildStdout,
        stderr: Option<std::process::ChildStderr>,
    },
}

/// Unix 用 script PTY 实时输出；Windows 用 pipe（ConPTY 会丢失输出且中文路径易失败）。
fn spawn_tool_child(
    tool_path: &Path,
    work_dir: &Path,
    device_id: Option<&str>,
    args: &[String],
) -> Result<SpawnedTool, String> {
    let tool_args = tool_argv(device_id, args);

    #[cfg(unix)]
    if let Ok(spawned) = try_spawn_with_script(tool_path, work_dir, &tool_args) {
        return Ok(spawned);
    }

    spawn_tool_pipe(tool_path, work_dir, &tool_args)
}

#[cfg(unix)]
fn try_spawn_with_script(
    tool_path: &Path,
    work_dir: &Path,
    tool_args: &[String],
) -> Result<SpawnedTool, String> {
    let mut cmd = Command::new("script");
    cmd.current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "macos")]
    {
        cmd.arg("-F").arg("-q").arg("/dev/null").arg(tool_path);
        cmd.args(tool_args);
    }

    #[cfg(target_os = "linux")]
    {
        cmd.arg("-q")
            .arg("-f")
            .arg("-c")
            .arg(shell_join(tool_path, tool_args))
            .arg("/dev/null");
    }

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("Failed to read stdout")?;
    let stderr = child.stderr.take();
    Ok(SpawnedTool::Pipe {
        child,
        stdout,
        stderr,
    })
}

fn spawn_tool_pipe(
    tool_path: &Path,
    work_dir: &Path,
    tool_args: &[String],
) -> Result<SpawnedTool, String> {
    let mut cmd = Command::new(tool_path);
    cmd.current_dir(work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(tool_args);

    #[cfg(windows)]
    apply_windows_hidden(&mut cmd);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start upgrade_tool: {e}"))?;
    let stdout = child.stdout.take().ok_or("Failed to read stdout")?;
    let stderr = child.stderr.take();
    Ok(SpawnedTool::Pipe {
        child,
        stdout,
        stderr,
    })
}

fn run_tool_sync(
    app: &AppHandle,
    tool_path: &Path,
    work_dir: &Path,
    device_id: Option<&str>,
    args: &[String],
    silent: bool,
) -> Result<CommandResult, String> {
    let tool_args = tool_argv(device_id, args);
    if !silent {
        emit_log(app, &format!("> upgrade_tool {}", tool_args.join(" ")), false);
    }

    match spawn_tool_child(tool_path, work_dir, device_id, args)? {
        SpawnedTool::Pipe {
            mut child,
            stdout,
            stderr,
        } => {
            let app_out = app.clone();
            let stdout_handle = thread::spawn(move || read_tool_stream(stdout, app_out, silent));

            let stderr_handle = if let Some(stderr) = stderr {
                let app_err = app.clone();
                Some(thread::spawn(move || read_tool_stream(stderr, app_err, silent)))
            } else {
                None
            };

            let timeout = command_timeout(args);
            let status = match wait_for_child(&mut child, timeout) {
                Ok(status) => status,
                Err(err) => {
                    let _ = stdout_handle.join();
                    if let Some(handle) = stderr_handle {
                        let _ = handle.join();
                    }
                    return Err(err);
                }
            };

            let mut output = stdout_handle
                .join()
                .map_err(|_| "stdout reader thread panicked".to_string())??;
            if let Some(handle) = stderr_handle {
                output.push_str(&handle.join().map_err(|_| "stderr reader thread panicked".to_string())??);
            }

            Ok(CommandResult {
                success: command_matches_success(args, &output, status.success()),
                output,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        command_matches_success, is_progress_line, timeout_for_bytes, write_payload_bytes,
        TerminalLineBuffer,
    };
    use std::fs;
    use std::time::Duration;

    #[test]
    fn timeout_scales_with_file_size() {
        let dir = std::env::temp_dir().join(format!("rkdevtool-timeout-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("image.img");
        fs::write(&path, vec![0u8; 1024 * 1024]).unwrap();

        let args = vec![
            "WL".into(),
            "0x0".into(),
            path.display().to_string(),
        ];
        let bytes = write_payload_bytes(&args);
        assert_eq!(bytes, 1024 * 1024);
        assert_eq!(timeout_for_bytes(bytes), Duration::from_secs(180));

        let large = 2 * 1024 * 1024 * 1024u64;
        assert_eq!(timeout_for_bytes(large), Duration::from_secs(530));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn di_timeout_sums_partition_paths() {
        let dir = std::env::temp_dir().join(format!("rkdevtool-di-timeout-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let boot = dir.join("boot.img");
        let root = dir.join("root.img");
        fs::write(&boot, vec![0u8; 512 * 1024]).unwrap();
        fs::write(&root, vec![0u8; 1024 * 1024]).unwrap();

        let args = vec![
            "DI".into(),
            "-boot".into(),
            boot.display().to_string(),
            "-root".into(),
            root.display().to_string(),
        ];
        assert_eq!(write_payload_bytes(&args), 512 * 1024 + 1024 * 1024);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn terminal_buffer_full_progress_rewrite() {
        let mut buf = TerminalLineBuffer::new();
        for chunk in b"\rDownload Image... (10%)\rDownload Image... (100%)" {
            buf.feed_byte(*chunk);
        }
        assert_eq!(buf.line(), "Download Image... (100%)");
    }

    #[test]
    fn terminal_buffer_cursor_partial_overwrite() {
        let mut buf = TerminalLineBuffer::new();
        buf.feed_bytes(b"Download Image... (10%)");
        buf.feed_bytes(b"\r");
        buf.feed_bytes(b"\x1b[12G");
        buf.feed_bytes(b"age... (100%)");
        assert_eq!(buf.line(), "Download Image... (100%)");
    }

    #[test]
    fn progress_line_excludes_step_success() {
        assert!(!is_progress_line("Download Boot Success"));
        assert!(is_progress_line("Download Image... (100%)"));
    }

    #[test]
    fn db_success_on_exit_zero_without_error() {
        let args = vec!["DB".into(), "/path/download.bin".into()];
        let output = "Download boot...\n";
        assert!(command_matches_success(&args, output, true));
    }

    #[test]
    fn db_fails_on_error_in_output() {
        let args = vec!["DB".into(), "/path/download.bin".into()];
        let output = "Download Boot Fail\n";
        assert!(!command_matches_success(&args, output, true));
    }

    #[test]
    fn pick_device_arg_omits_selector_for_single_device() {
        assert_eq!(super::pick_device_arg(1, Some("1113113".into())), None);
        assert_eq!(
            super::pick_device_arg(2, Some("1113113".into())),
            Some("1113113".into())
        );
    }

    #[test]
    fn db_fails_on_nonzero_exit() {
        let args = vec!["DB".into(), "/path/download.bin".into()];
        let output = "Download boot...\n";
        assert!(!command_matches_success(&args, output, false));
    }

    #[test]
    fn db_note_without_error_is_success() {
        let args = vec!["DB".into(), "/path/download.bin".into()];
        let output = "Download boot...\nNote: please check ddr, please reset device\n";
        assert!(command_matches_success(&args, output, true));
    }

    #[test]
    fn uf_requires_ok_message() {
        let args = vec!["UF".into(), "/path/download.bin".into()];
        let output = "Loading firmware...\nftruncate: Invalid argument\n";
        assert!(!command_matches_success(&args, output, true));
    }

    #[test]
    fn uf_success_with_carriage_only_output() {
        let args = vec!["UF".into(), "/path/update.img".into()];
        let mut output = String::new();
        super::push_output_line(&mut output, "Download firmware 10%", true);
        super::push_output_line(&mut output, "Download firmware 50%", true);
        super::push_output_line(&mut output, "Upgrade firmware ok.", true);
        assert!(command_matches_success(&args, &output, true));
    }

    #[test]
    fn uf_success_even_on_nonzero_exit_after_ok_message() {
        let args = vec!["UF".into(), "/path/update.img".into()];
        let output = "Upgrade firmware ok.\n";
        assert!(command_matches_success(&args, output, false));
    }

    #[test]
    fn progress_line_detection() {
        assert!(super::is_progress_line("Download Firmware Progress... (45%)"));
        assert!(super::is_progress_line("Loading firmware..."));
        assert!(!super::is_progress_line("Download Boot Success"));
        assert!(!super::is_progress_line("Wait For Maskrom Success"));
        assert!(!super::is_progress_line("Start to upgrade firmware..."));
        assert!(!super::is_progress_line("Upgrade firmware ok."));
    }

    #[test]
    fn uf_step_lines_preserved_in_output() {
        let args = vec!["UF".into(), "/path/update.img".into()];
        let mut output = String::new();
        super::push_output_line(&mut output, "Download Boot Success", false);
        super::push_output_line(&mut output, "Download Firmware Progress... (50%)", true);
        super::push_output_line(&mut output, "Download Firmware Progress... (100%)", true);
        super::push_output_line(&mut output, "Upgrade firmware ok.", false);
        assert!(output.contains("Download Boot Success"));
        assert!(command_matches_success(&args, &output, true));
    }
}

fn ensure_not_busy(state: &State<'_, AppState>) -> Result<(), String> {
    let busy = state.busy.lock().map_err(|e| e.to_string())?;
    if *busy {
        return Err("Another task is already running; please wait".to_string());
    }
    Ok(())
}

fn set_busy(state: &State<'_, AppState>, busy: bool) -> Result<(), String> {
    *state.busy.lock().map_err(|e| e.to_string())? = busy;
    Ok(())
}

async fn with_tool<F>(app: AppHandle, state: State<'_, AppState>, f: F) -> Result<CommandResult, String>
where
    F: FnOnce(&AppHandle, &Path, &Path, Option<&str>) -> Result<CommandResult, String> + Send + 'static,
{
    ensure_not_busy(&state)?;
    set_busy(&state, true)?;

    let device = state
        .selected_device
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let device_arg = device_arg_for_tool(&state, device);

    let result = tauri::async_runtime::spawn_blocking(move || {
        let (tool_path, work_dir) = resolve_tool_paths(&app)?;
        f(&app, &tool_path, &work_dir, device_arg.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?;

    let _ = set_busy(&state, false);
    result
}

#[tauri::command]
pub async fn get_tool_info(app: AppHandle) -> Result<ToolInfo, String> {
    let (tool_path, work_dir) = resolve_tool_paths(&app)?;
    let version = std::fs::read_to_string(work_dir.join("revision.txt"))
        .ok()
        .and_then(|text| text.lines().next().map(str::trim).map(String::from))
        .unwrap_or_else(|| "unknown".to_string());

    Ok(ToolInfo {
        version,
        platform_dir: platform_dir_name().to_string(),
        tool_path: tool_path.display().to_string(),
    })
}

#[tauri::command]
pub async fn list_devices(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<RockusbDevice>, String> {
    if *state.busy.lock().map_err(|e| e.to_string())? {
        return crate::devices::cached_devices(state.inner());
    }

    crate::devices::resync_devices(&app, state.inner()).await
}

#[tauri::command]
pub fn select_device(state: State<'_, AppState>, location_id: Option<String>) -> Result<(), String> {
    *state.selected_device.lock().map_err(|e| e.to_string())? = location_id;
    Ok(())
}

#[tauri::command]
pub async fn download_execute(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: DownloadExecutePayload,
) -> Result<(), String> {
    crate::device_ops::download_execute(app, state, payload).await
}

fn action_label_en(action: &str) -> &'static str {
    match action {
        "read-flash-id" => "Read Flash ID",
        "read-flash-info" => "Read Flash info",
        "read-chip-info" => "Read chip info",
        "read-capability" => "Read capability",
        "test-device" => "Test device",
        "reboot-device" => "Reboot device",
        "enter-maskrom" => "Enter Maskrom",
        "switch-storage" => "Switch storage",
        "get-current-storage" => "Get current storage",
        "clear-serial" => "Clear serial",
        "detect-secure-mode" => "Detect secure mode",
        "export-serial-log" => "Export serial log",
        "erase-sector" => "Erase sector",
        "erase-all" => "Erase all",
        "switch-usb3" => "Switch USB3",
        _ => "Action",
    }
}

fn action_to_args(action: &str, params: &ActionParams) -> Result<Vec<String>, String> {
    Ok(match action {
        "read-flash-id" => vec!["RID".into()],
        "read-flash-info" => vec!["RFI".into()],
        "read-chip-info" => vec!["RCI".into()],
        "read-capability" => vec!["RCB".into()],
        "test-device" => vec!["TD".into()],
        "reboot-device" => vec!["RD".into()],
        "enter-maskrom" => vec!["RD".into(), "3".into()],
        "switch-storage" => {
            let index = params
                .start_sector
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "1".into());
            vec!["SSD".into(), index]
        }
        "clear-serial" => vec!["SN".into(), String::new()],
        "detect-secure-mode" => vec!["RSM".into()],
        "export-serial-log" => {
            let path = params
                .output_path
                .clone()
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| "serial.log".into());
            vec!["RCL".into(), path]
        }
        "erase-sector" => {
            let start = params.start_sector.clone().unwrap_or_else(|| "0".into());
            let count = params.sector_count.clone().unwrap_or_else(|| "1".into());
            vec!["EL".into(), start, count]
        }
        "erase-all" => {
            let loader = params
                .boot_path
                .clone()
                .filter(|p| !p.is_empty())
                .ok_or("Erase all requires a Boot/Loader path")?;
            vec!["EF".into(), loader]
        }
        "switch-usb3" => vec!["SSD".into()],
        _ => return Err(format!("Unsupported action: {}", action_label_en(action))),
    })
}

#[tauri::command]
pub async fn run_action(
    app: AppHandle,
    state: State<'_, AppState>,
    action: String,
    params: Option<ActionParams>,
) -> Result<String, String> {
    let params = params.unwrap_or(ActionParams {
        boot_path: None,
        start_sector: None,
        sector_count: None,
        output_path: None,
    });

    if let Some(output) = crate::device_ops::try_run_action(
        app.clone(),
        state.clone(),
        &action,
        params.start_sector.as_deref(),
        params.sector_count.as_deref(),
        params.output_path.as_deref(),
    )
    .await?
    {
        return Ok(output);
    }

    let args = action_to_args(&action, &params)?;
    let result = with_tool(app, state, move |app, tool, dir, device| {
        run_tool_sync(app, tool, dir, device, &args, false)
    })
    .await?;

    if !result.success {
        return Err(format!("{} failed", action_label_en(&action)));
    }
    Ok(result.output)
}

#[tauri::command]
pub fn is_tool_busy(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(*state.busy.lock().map_err(|e| e.to_string())?)
}
