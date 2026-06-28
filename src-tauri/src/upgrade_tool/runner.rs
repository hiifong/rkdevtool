use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

const EVENT_TOOL_LOG: &str = "tool-log";
const EVENT_DEVICES: &str = "devices-updated";

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
        return Err(format!("找不到 upgrade_tool: {}", tool_path.display()));
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

fn command_timeout(args: &[String]) -> Duration {
    match args.first().map(|s| s.as_str()) {
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
                    "操作超时（{} 秒），请重新进入 Maskrom 并检查 USB 连接后重试",
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

fn devices_to_snapshot(devices: &[RockusbDevice]) -> Vec<crate::state::DeviceSnapshot> {
    devices
        .iter()
        .map(|d| (d.location_id.clone(), d.mode.clone(), d.label.clone()))
        .collect()
}

fn snapshot_to_devices(snapshots: &[crate::state::DeviceSnapshot]) -> Vec<RockusbDevice> {
    snapshots
        .iter()
        .map(|(location_id, mode, label)| RockusbDevice {
            location_id: location_id.clone(),
            mode: mode.clone(),
            label: label.clone(),
        })
        .collect()
}

fn store_devices(state: &State<'_, AppState>, devices: &[RockusbDevice]) -> Result<(), String> {
    *state.last_devices.lock().map_err(|e| e.to_string())? = devices_to_snapshot(devices);
    Ok(())
}

fn cached_devices(state: &State<'_, AppState>) -> Result<Vec<RockusbDevice>, String> {
    let cache = state.last_devices.lock().map_err(|e| e.to_string())?;
    Ok(snapshot_to_devices(&cache))
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

fn tool_error_summary(output: &str) -> String {
    let mut last = String::from("未知错误");
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
            || lower.contains("请检查")
        {
            last = line;
        }
    }
    last
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
    if let Some(id) = device_id {
        tool_args.push(String::from("-s"));
        tool_args.push(id.to_string());
    }
    tool_args.extend(args.iter().cloned());
    tool_args
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
    #[cfg(windows)]
    Pty {
        child: Box<dyn portable_pty::Child + Send + Sync>,
        reader: Box<dyn Read + Send>,
    },
}

#[cfg(windows)]
fn wait_for_pty_child(
    child: &mut dyn portable_pty::Child,
    timeout: Duration,
) -> Result<bool, String> {
    let start = Instant::now();
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => return Ok(status.success()),
            None if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "操作超时（{} 秒），请重新进入 Maskrom 并检查 USB 连接后重试",
                    timeout.as_secs()
                ));
            }
            None => thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// 通过 PTY 启动 upgrade_tool，避免 pipe 模式下 stdout 全缓冲导致日志延迟。
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

    #[cfg(windows)]
    if let Ok(spawned) = try_spawn_with_pty(tool_path, work_dir, &tool_args) {
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
    let stdout = child.stdout.take().ok_or("无法读取 stdout")?;
    let stderr = child.stderr.take();
    Ok(SpawnedTool::Pipe {
        child,
        stdout,
        stderr,
    })
}

#[cfg(windows)]
fn try_spawn_with_pty(
    tool_path: &Path,
    work_dir: &Path,
    tool_args: &[String],
) -> Result<SpawnedTool, String> {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;

    let mut cmd = CommandBuilder::new(tool_path.display().to_string());
    cmd.cwd(work_dir.display().to_string());
    for arg in tool_args {
        cmd.arg(arg);
    }

    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);

    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;

    Ok(SpawnedTool::Pty {
        child,
        reader: Box::new(reader),
    })
}

fn spawn_tool_pipe(
    tool_path: &Path,
    work_dir: &Path,
    tool_args: &[String],
) -> Result<SpawnedTool, String> {
    let mut cmd = Command::new(tool_path);
    cmd.current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(tool_args);

    #[cfg(windows)]
    apply_windows_hidden(&mut cmd);

    let mut child = cmd.spawn()
        .map_err(|e| format!("启动 upgrade_tool 失败: {e}"))?;
    let stdout = child.stdout.take().ok_or("无法读取 stdout")?;
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
    let joined = args.join(" ");
    if !silent {
        emit_log(app, &format!("> upgrade_tool {joined}"), false);
    }

    match spawn_tool_child(tool_path, work_dir, device_id, args)? {
        #[cfg(windows)]
        SpawnedTool::Pty { mut child, reader } => {
            let app_out = app.clone();
            let stdout_handle =
                thread::spawn(move || read_tool_stream(reader, app_out, silent));

            let timeout = command_timeout(args);
            let exit_ok = match wait_for_pty_child(child.as_mut(), timeout) {
                Ok(ok) => ok,
                Err(err) => {
                    let _ = stdout_handle.join();
                    return Err(err);
                }
            };

            let output = stdout_handle
                .join()
                .map_err(|_| "读取 stdout 线程异常".to_string())??;

            return Ok(CommandResult {
                success: command_matches_success(args, &output, exit_ok),
                output,
            });
        }
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
                .map_err(|_| "读取 stdout 线程异常".to_string())??;
            if let Some(handle) = stderr_handle {
                output.push_str(&handle.join().map_err(|_| "读取 stderr 线程异常".to_string())??);
            }

            Ok(CommandResult {
                success: command_matches_success(args, &output, status.success()),
                output,
            })
        }
    }
}

fn parse_field(line: &str, keys: &[&str]) -> Option<String> {
    for key in keys {
        for prefix in [*key, &key.to_ascii_lowercase()] {
            let marker = format!("{prefix}=");
            if let Some(idx) = line.find(&marker) {
                let rest = &line[idx + marker.len()..];
                let value: String = rest
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != ',')
                    .collect();
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
    }
    None
}

fn run_ld_sync(tool_path: &Path, work_dir: &Path) -> Result<String, String> {
    let mut cmd = Command::new(tool_path);
    cmd.current_dir(work_dir).arg("LD");
    #[cfg(windows)]
    apply_windows_hidden(&mut cmd);

    let output = cmd
        .output()
        .map_err(|e| format!("启动 upgrade_tool 失败: {e}"))?;

    let mut combined = String::new();
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    let stdout = strip_ansi(&String::from_utf8_lossy(&output.stdout));

    if !stderr.is_empty() {
        combined.push_str(&stderr);
        if !stderr.ends_with('\n') {
            combined.push('\n');
        }
    }
    combined.push_str(&stdout);
    Ok(combined)
}

fn parse_devices_from_blob(output: &str) -> Vec<RockusbDevice> {
    let text = strip_ansi(output);
    let lower = text.to_ascii_lowercase();
    let mut devices: Vec<RockusbDevice> = Vec::new();
    let mut start = 0;

    while let Some(rel) = lower[start..].find("locationid=") {
        let idx = start + rel;
        let slice = &text[idx..];
        if let Some(id) = parse_field(slice, &["LocationID", "LocationId"]) {
            let mode = parse_field(slice, &["Mode"]).unwrap_or_else(|| "UNKNOWN".to_string());
            if !devices.iter().any(|d| d.location_id == id) {
                devices.push(RockusbDevice {
                    label: format!("{id} : {}", mode.to_ascii_uppercase()),
                    location_id: id,
                    mode,
                });
            }
        }
        start = idx + 1;
    }

    devices
}

fn parse_devices_by_lines(output: &str) -> Vec<RockusbDevice> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let line = strip_ansi(line);
        if line.is_empty() {
            continue;
        }

        let lower = line.to_ascii_lowercase();
        if lower.contains("no found") || lower.contains("not found") {
            continue;
        }

        if !lower.contains("locationid=") && !lower.contains("devno=") {
            if !(line.contains(':') && (lower.contains("maskrom") || lower.contains("loader"))) {
                continue;
            }
        }

        if let Some(id) = parse_field(&line, &["LocationID", "LocationId"]) {
            let mode_str = parse_field(&line, &["Mode"]).unwrap_or_else(|| "UNKNOWN".to_string());
            devices.push(RockusbDevice {
                label: format!("{id} : {}", mode_str.to_ascii_uppercase()),
                location_id: id,
                mode: mode_str,
            });
            continue;
        }

        if line.contains(':') && (lower.contains("maskrom") || lower.contains("loader")) {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                let id = parts[0].trim();
                let mode_part = parts[1].trim();
                if !id.is_empty() {
                    devices.push(RockusbDevice {
                        label: format!("{id} : {mode_part}"),
                        location_id: id.to_string(),
                        mode: mode_part.to_string(),
                    });
                }
            }
        }
    }
    devices
}

fn parse_devices(output: &str) -> Vec<RockusbDevice> {
    let devices = parse_devices_by_lines(output);
    if !devices.is_empty() {
        return devices;
    }
    parse_devices_from_blob(output)
}

#[cfg(test)]
mod tests {
    use super::{command_matches_success, is_progress_line, parse_devices, TerminalLineBuffer};

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

    #[test]
    fn parse_ld_comma_separated_line() {
        let output = "List of rockusb connected(1)\n\
            DevNo=1 Vid=0x2207,Pid=0x110c,LocationID=24113  Mode=Maskrom    SerialNo=\n";
        let devices = parse_devices(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].location_id, "24113");
        assert_eq!(devices[0].mode, "Maskrom");
    }

    #[test]
    fn parse_ld_serial_no_suffix() {
        let output = "Using /path/config.ini\nList of rockusb connected(1)\n\
            DevNo=1 Vid=0x2207,Pid=0x110c,LocationID=24113 Mode=Maskrom SerialNo=rockchip\n";
        let devices = parse_devices(output);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].location_id, "24113");
        assert_eq!(devices[0].mode, "Maskrom");
    }

    #[test]
    fn parse_ld_no_devices() {
        let output = "List of rockusb connected(0)\n";
        assert!(parse_devices(output).is_empty());
    }
}

fn ensure_not_busy(state: &State<'_, AppState>) -> Result<(), String> {
    let busy = state.busy.lock().map_err(|e| e.to_string())?;
    if *busy {
        return Err("已有任务正在执行，请稍候".to_string());
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

    let result = tauri::async_runtime::spawn_blocking(move || {
        let (tool_path, work_dir) = resolve_tool_paths(&app)?;
        f(&app, &tool_path, &work_dir, device.as_deref())
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
        return cached_devices(&state);
    }

    let (tool_path, work_dir) = resolve_tool_paths(&app)?;
    let output = tauri::async_runtime::spawn_blocking(move || run_ld_sync(&tool_path, &work_dir))
        .await
        .map_err(|e| e.to_string())??;

    let devices = parse_devices(&output);
    store_devices(&state, &devices)?;

    {
        let mut selected = state.selected_device.lock().map_err(|e| e.to_string())?;
        if devices.is_empty() {
            *selected = None;
        } else if !devices
            .iter()
            .any(|d| selected.as_deref() == Some(d.location_id.as_str()))
        {
            *selected = Some(devices[0].location_id.clone());
        }
    }

    let _ = app.emit(EVENT_DEVICES, &devices);
    Ok(devices)
}

#[tauri::command]
pub fn select_device(state: State<'_, AppState>, location_id: Option<String>) -> Result<(), String> {
    *state.selected_device.lock().map_err(|e| e.to_string())? = location_id;
    Ok(())
}

#[tauri::command]
pub async fn partition_list(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let result = with_tool(app, state, |app, tool, dir, device| {
        run_tool_sync(app, tool, dir, device, &[String::from("PL")], false)
    })
    .await?;

    if !result.success {
        return Err("读取分区表失败".to_string());
    }
    Ok(result.output)
}

#[tauri::command]
pub async fn upgrade_firmware(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    no_reset: Option<bool>,
) -> Result<(), String> {
    let mut args = vec![String::from("UF"), path];
    if no_reset.unwrap_or(false) {
        args.push(String::from("-noreset"));
    }

    let result = with_tool(app, state, move |app, tool, dir, device| {
        run_tool_sync(app, tool, dir, device, &args, false)
    })
    .await?;

    if !result.success {
        let detail = if output_has_error(&result.output) {
            tool_error_summary(&result.output)
        } else {
            "未检测到 Upgrade firmware ok / Success".to_string()
        };
        let hint = if detail.to_ascii_lowercase().contains("read chip info fail") {
            "（Maskrom 下需先完成 Boot 下载；若仍失败，请确认 Mac 版 upgrade_tool v2.13 是否支持该芯片，或改用 Linux v2.44+ 工具）"
        } else if detail.contains("ftruncate") {
            "（请确认选择的是 update.img 固件包，不要用 download.bin / Loader 文件）"
        } else if detail == "未检测到 Upgrade firmware ok / Success" {
            "（若设备无响应，请重新进入 Maskrom 后重试）"
        } else {
            ""
        };
        return Err(format!("升级固件失败: {detail}{hint}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn download_boot(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let args = vec![String::from("DB"), path];
    let result = with_tool(app, state, move |app, tool, dir, device| {
        run_tool_sync(app, tool, dir, device, &args, false)
    })
    .await?;

    if !result.success {
        let detail = if output_has_error(&result.output) {
            tool_error_summary(&result.output)
        } else {
            "upgrade_tool 返回非零退出码".to_string()
        };
        let hint = if detail.to_ascii_lowercase().contains("ddr") || detail.contains("请检查") {
            "（请检查 DDR/主控与 USB 连接，重新进入 Maskrom 后重试）"
        } else {
            "（请使用 Loader 文件如 MiniLoaderAll.bin；download.bin 需确认是否为正确 Loader）"
        };
        return Err(format!("下载 Boot 失败: {detail}。{hint}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn read_chip_info(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let result = with_tool(app, state, |app, tool, dir, device| {
        run_tool_sync(app, tool, dir, device, &[String::from("RCI")], false)
    })
    .await?;

    Ok(result.output)
}

#[tauri::command]
pub async fn download_execute(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: DownloadExecutePayload,
) -> Result<(), String> {
    let rows: Vec<_> = payload
        .rows
        .into_iter()
        .filter(|r| r.enabled && !r.path.trim().is_empty())
        .collect();

    if rows.is_empty() {
        return Err("请至少启用一行并填写镜像路径".to_string());
    }

    let force = payload.force_by_address;
    let result = with_tool(app, state, move |app, tool, dir, device| {
        if force {
            for row in &rows {
                let addr = row.address.trim();
                if addr.is_empty() {
                    return Err(format!("按地址写入需要填写地址: {}", row.name));
                }
                let args = vec![String::from("WL"), addr.to_string(), row.path.clone()];
                let r = run_tool_sync(app, tool, dir, device, &args, false)?;
                if !r.success {
                    return Err(format!("写入失败: {}", row.name));
                }
            }
            return Ok(CommandResult {
                success: true,
                output: String::new(),
            });
        }

        for row in &rows {
            if row.name.eq_ignore_ascii_case("loader") {
                let mut args = vec![String::from("UL"), row.path.clone()];
                if !row.storage.trim().is_empty() {
                    args.push(row.storage.trim().to_uppercase());
                }
                let r = run_tool_sync(app, tool, dir, device, &args, false)?;
                if !r.success {
                    return Err("烧写 Loader 失败".to_string());
                }
            }
        }

        let mut di_args = vec![String::from("DI")];
        for row in &rows {
            if row.name.eq_ignore_ascii_case("loader") {
                continue;
            }
            if row.name.trim().is_empty() {
                continue;
            }
            di_args.push(format!("-{}", row.name.trim()));
            di_args.push(row.path.clone());
        }

        if di_args.len() > 1 {
            let r = run_tool_sync(app, tool, dir, device, &di_args, false)?;
            if !r.success {
                return Err("烧录镜像失败".to_string());
            }
        }

        Ok(CommandResult {
            success: true,
            output: String::new(),
        })
    })
    .await?;

    if !result.success {
        return Err("执行失败".to_string());
    }
    Ok(())
}

fn action_to_args(action: &str, params: &ActionParams) -> Result<Vec<String>, String> {
    Ok(match action {
        "读取FlashID" => vec!["RID".into()],
        "读取Flash信息" => vec!["RFI".into()],
        "读取Chip信息" => vec!["RCI".into()],
        "读取Capability" => vec!["RCB".into()],
        "测试设备" => vec!["TD".into()],
        "重启设备" => vec!["RD".into()],
        "进入Maskrom" => vec!["RD".into(), "3".into()],
        "切换存储" | "获取当前存储" => {
            let index = params
                .start_sector
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "1".into());
            vec!["SSD".into(), index]
        }
        "清空序列号" => vec!["SN".into(), String::new()],
        "检测安全模式" => vec!["RSM".into()],
        "导出串口日志" => {
            let path = params
                .output_path
                .clone()
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| "serial.log".into());
            vec!["RCL".into(), path]
        }
        "擦除扇区" => {
            let start = params.start_sector.clone().unwrap_or_else(|| "0".into());
            let count = params.sector_count.clone().unwrap_or_else(|| "1".into());
            vec!["EL".into(), start, count]
        }
        "擦除所有" => {
            let loader = params
                .boot_path
                .clone()
                .filter(|p| !p.is_empty())
                .ok_or("擦除所有需要填写 Boot/Loader 路径")?;
            vec!["EF".into(), loader]
        }
        "切换USB3" => vec!["SSD".into()],
        _ => return Err(format!("暂不支持的操作: {action}")),
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

    let args = action_to_args(&action, &params)?;
    let result = with_tool(app, state, move |app, tool, dir, device| {
        run_tool_sync(app, tool, dir, device, &args, false)
    })
    .await?;

    if !result.success {
        return Err(format!("{action} 失败"));
    }
    Ok(result.output)
}

#[tauri::command]
pub fn is_tool_busy(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(*state.busy.lock().map_err(|e| e.to_string())?)
}
