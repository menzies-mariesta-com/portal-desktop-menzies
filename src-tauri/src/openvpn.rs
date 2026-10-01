//! Bundled OpenVPN process management (one tunnel).

use crate::paths::{self, PathError};
use serde::Serialize;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const MAX_LOG_LINES: usize = 2000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnPhase {
    Idle,
    Connecting,
    Connected,
    #[allow(dead_code)]
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnStatus {
    pub phase: ConnPhase,
    pub profile_id: Option<String>,
    pub message: String,
    pub vpn_ip: Option<String>,
    pub started_at_ms: Option<u64>,
    pub management_port: Option<u16>,
}

impl Default for ConnStatus {
    fn default() -> Self {
        Self {
            phase: ConnPhase::Idle,
            profile_id: None,
            message: String::new(),
            vpn_ip: None,
            started_at_ms: None,
            management_port: None,
        }
    }
}

pub struct TunnelState {
    pub status: ConnStatus,
    pub child: Option<Child>,
    pub log_lines: Vec<String>,
    pub stop_flag: Arc<AtomicBool>,
    pub kill_switch: bool,
}

impl Default for TunnelState {
    fn default() -> Self {
        Self {
            status: ConnStatus::default(),
            child: None,
            log_lines: Vec::new(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            kill_switch: false,
        }
    }
}

pub type SharedTunnel = Arc<Mutex<TunnelState>>;

pub fn new_shared_tunnel() -> SharedTunnel {
    Arc::new(Mutex::new(TunnelState::default()))
}

fn bundled_openvpn(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(override_bin) = std::env::var("PORTAL_OPENVPN_BIN") {
        let p = PathBuf::from(override_bin);
        if p.is_file() {
            return Ok(p);
        }
        return Err("PORTAL_OPENVPN_BIN is set but is not a file".into());
    }

    let resource_root = resource_openvpn_dir(app)?;
    let candidate = openvpn_binary_in(&resource_root);

    if candidate.is_file() {
        return Ok(candidate);
    }

    Err(format!(
        "Bundled OpenVPN not found at {}. Set PORTAL_OPENVPN_BIN for development.",
        candidate.display()
    ))
}

fn openvpn_binary_in(resource_root: &Path) -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        let arch = match std::env::consts::ARCH {
            "x86_64" => "linux-x86_64",
            "aarch64" => "linux-aarch64",
            other => other,
        };
        return resource_root.join(arch).join("openvpn");
    }
    #[cfg(target_os = "windows")]
    {
        return resource_root.join("windows-x86_64").join("openvpn.exe");
    }
    #[cfg(target_os = "macos")]
    {
        let aarch = resource_root.join("macos-aarch64").join("openvpn");
        if aarch.is_file() {
            return aarch;
        }
        return resource_root.join("macos-x86_64").join("openvpn");
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        resource_root.join("openvpn")
    }
}

fn resource_openvpn_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let mut tried: Vec<PathBuf> = Vec::new();

    // Packaged / tauri-action: resolve via PathResolver (deb, AppImage, etc.)
    if let Ok(rd) = app.path().resource_dir() {
        for rel in ["resources/openvpn", "openvpn", "../resources/openvpn"] {
            let p = rd.join(rel);
            tried.push(p.clone());
            if p.is_dir() {
                return Ok(p);
            }
        }
        // Flat: resource_dir itself contains linux-x86_64/
        tried.push(rd.clone());
        if openvpn_binary_in(&rd).is_file() {
            return Ok(rd);
        }
    }

    // Dev: src-tauri/resources/openvpn (compile-time crate root)
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/openvpn");
    tried.push(manifest.clone());
    if manifest.is_dir() {
        return Ok(manifest);
    }

    // Dev / copied beside binary (tauri copies bundle.resources next to exe)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for rel in ["resources/openvpn", "../resources/openvpn"] {
                let p = dir.join(rel);
                tried.push(p.clone());
                if p.is_dir() {
                    return Ok(p);
                }
            }
        }
    }

    Err(format!(
        "OpenVPN resource directory not found. Tried: {}",
        tried
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn pick_management_port() -> u16 {
    use std::net::TcpListener;
    TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
        .unwrap_or(17505)
}

fn push_log(state: &SharedTunnel, app: &AppHandle, line: String) {
    if let Ok(mut guard) = state.lock() {
        guard.log_lines.push(line.clone());
        if guard.log_lines.len() > MAX_LOG_LINES {
            let overflow = guard.log_lines.len() - MAX_LOG_LINES;
            guard.log_lines.drain(0..overflow);
        }
    }
    let _ = app.emit("portal://log", line);
}

fn set_status(state: &SharedTunnel, app: &AppHandle, status: ConnStatus) {
    if let Ok(mut guard) = state.lock() {
        guard.status = status.clone();
    }
    let _ = app.emit("portal://status", status);
}

pub fn current_status(state: &SharedTunnel) -> ConnStatus {
    state.lock().map(|g| g.status.clone()).unwrap_or_default()
}

pub fn current_logs(state: &SharedTunnel) -> Vec<String> {
    state
        .lock()
        .map(|g| g.log_lines.clone())
        .unwrap_or_default()
}

pub fn disconnect_tunnel(state: &SharedTunnel, app: &AppHandle) -> Result<(), String> {
    let (port, kill_switch, child) = {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.stop_flag.store(true, Ordering::SeqCst);
        let child = guard.child.take();
        (guard.status.management_port, guard.kill_switch, child)
    };

    // Prefer graceful OpenVPN exit via management (works when process is elevated).
    if let Some(port) = port {
        let _ = management_signal(port, "SIGTERM");
    }

    if let Some(mut child) = child {
        let _ = child.kill();
        // Never block the UI / IPC thread on wait.
        thread::spawn(move || {
            let _ = child.wait();
        });
    }

    #[cfg(target_os = "linux")]
    if kill_switch {
        thread::spawn(|| {
            let _ = clear_kill_switch();
        });
    }

    set_status(
        state,
        app,
        ConnStatus {
            phase: ConnPhase::Idle,
            profile_id: None,
            message: "Disconnected".into(),
            vpn_ip: None,
            started_at_ms: None,
            management_port: None,
        },
    );
    Ok(())
}

fn management_signal(port: u16, signal: &str) -> Result<(), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let _ = writeln!(stream, "signal {signal}");
    let _ = stream.flush();
    Ok(())
}

pub fn send_management_auth(port: u16, password_type: &str, password: &str) -> Result<(), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    // Escape for management protocol: replace newlines
    let safe = password.replace('\n', "\\n").replace('\r', "");
    let _ = writeln!(stream, "username \"Auth\" \"user\"");
    let _ = writeln!(stream, "password \"{password_type}\" \"{safe}\"");
    let _ = stream.flush();
    Ok(())
}

#[cfg(target_os = "linux")]
fn apply_kill_switch() -> Result<(), String> {
    // Minimal outbound block except loopback and established. Requires root via pkexec.
    let script = r#"
nft delete table inet portal_ks 2>/dev/null || true
nft add table inet portal_ks
nft add chain inet portal_ks output '{ type filter hook output priority 0; policy drop; }'
nft add rule inet portal_ks output oiflo "lo" accept
nft add rule inet portal_ks output ct state established,related accept
nft add rule inet portal_ks output meta l4proto icmp accept
"#;
    let status = Command::new("pkexec")
        .args(["sh", "-c", script])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(
            "Kill switch could not be applied. Polkit authentication may have failed.".into(),
        );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn clear_kill_switch() -> Result<(), String> {
    let status = Command::new("pkexec")
        .args([
            "sh",
            "-c",
            "nft delete table inet portal_ks 2>/dev/null || true",
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Kill switch could not be cleared.".into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn apply_kill_switch() -> Result<(), String> {
    Err("Kill switch is only available on Linux in this version.".into())
}

#[cfg(not(target_os = "linux"))]
fn clear_kill_switch() -> Result<(), String> {
    Ok(())
}

pub struct ConnectArgs {
    pub profile_id: String,
    pub config_path: PathBuf,
    pub auth_user_pass_path: Option<PathBuf>,
    pub kill_switch: bool,
    pub elevate: bool,
}

pub fn connect_tunnel(
    state: SharedTunnel,
    app: AppHandle,
    args: ConnectArgs,
) -> Result<(), String> {
    {
        let guard = state.lock().map_err(|e| e.to_string())?;
        if guard.status.phase == ConnPhase::Connecting || guard.status.phase == ConnPhase::Connected
        {
            return Err(
                "A VPN tunnel is already active. Disconnect before connecting another.".into(),
            );
        }
    }

    let openvpn = bundled_openvpn(&app)?;
    let mgmt_port = pick_management_port();
    let stop_flag = Arc::new(AtomicBool::new(false));

    // Mark connecting before any elevated dialog so the UI stays responsive.
    {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.stop_flag = stop_flag.clone();
        guard.kill_switch = args.kill_switch;
        guard.log_lines.clear();
        guard.child = None;
        guard.status = ConnStatus {
            phase: ConnPhase::Connecting,
            profile_id: Some(args.profile_id.clone()),
            message: if args.elevate {
                "Waiting for authentication (polkit)...".into()
            } else {
                "Connecting".into()
            },
            vpn_ip: None,
            started_at_ms: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
            ),
            management_port: Some(mgmt_port),
        };
    }
    set_status(&state, &app, current_status(&state));

    let cmd_args: Vec<String> = {
        let mut v = vec![
            "--config".into(),
            args.config_path.to_string_lossy().into_owned(),
            "--management".into(),
            "127.0.0.1".into(),
            mgmt_port.to_string(),
            "--management-query-passwords".into(),
            "--verb".into(),
            "3".into(),
        ];
        if let Some(auth) = &args.auth_user_pass_path {
            v.push("--auth-user-pass".into());
            v.push(auth.to_string_lossy().into_owned());
        }
        v
    };

    let state_bg = state.clone();
    let app_bg = app.clone();
    let elevate = args.elevate;
    let kill_switch = args.kill_switch;

    thread::spawn(move || {
        if kill_switch {
            if let Err(err) = apply_kill_switch() {
                push_log(&state_bg, &app_bg, format!("Kill switch: {err}"));
                let mut status = current_status(&state_bg);
                status.phase = ConnPhase::Error;
                status.message = err;
                set_status(&state_bg, &app_bg, status);
                return;
            }
        }

        let child = match spawn_openvpn(&openvpn, &cmd_args, elevate) {
            Ok(c) => c,
            Err(err) => {
                let mut status = current_status(&state_bg);
                status.phase = ConnPhase::Error;
                status.message = err;
                set_status(&state_bg, &app_bg, status);
                return;
            }
        };

        {
            if let Ok(mut guard) = state_bg.lock() {
                if guard.stop_flag.load(Ordering::SeqCst) {
                    let mut child = child;
                    let _ = child.kill();
                    thread::spawn(move || {
                        let _ = child.wait();
                    });
                    return;
                }
                guard.child = Some(child);
                guard.status.message = "Connecting".into();
            }
        }
        set_status(&state_bg, &app_bg, current_status(&state_bg));

        let mut hold_released = false;
        let started = Instant::now();
        while !stop_flag.load(Ordering::SeqCst) {
            if !hold_released {
                if management_release_hold(mgmt_port).is_ok() {
                    hold_released = true;
                    push_log(&state_bg, &app_bg, "Management hold released".into());
                }
            }

            if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", mgmt_port)) {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
                if !hold_released {
                    let _ = writeln!(stream, "hold release");
                    let _ = stream.flush();
                    hold_released = true;
                }
                let _ = writeln!(stream, "state");
                let _ = stream.flush();
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                // Bound reads so a stuck socket cannot freeze this worker.
                for _ in 0..32 {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim().to_string();
                            if trimmed.is_empty() {
                                continue;
                            }
                            push_log(&state_bg, &app_bg, trimmed.clone());
                            if trimmed.contains("CONNECTED,SUCCESS")
                                || trimmed.contains("Initialization Sequence Completed")
                            {
                                let mut status = current_status(&state_bg);
                                status.phase = ConnPhase::Connected;
                                status.message = "Connected".into();
                                set_status(&state_bg, &app_bg, status);
                            }
                            if trimmed.contains("AUTH_FAILED") {
                                let mut status = current_status(&state_bg);
                                status.phase = ConnPhase::Error;
                                status.message = "Authentication failed".into();
                                set_status(&state_bg, &app_bg, status);
                            }
                            if trimmed.starts_with(">PASSWORD:") {
                                let mut status = current_status(&state_bg);
                                status.message = format!("Auth challenge: {trimmed}");
                                set_status(&state_bg, &app_bg, status);
                                let _ = app_bg.emit("portal://auth-challenge", trimmed);
                            }
                        }
                        Err(_) => break,
                    }
                    if stop_flag.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }

            let exited = {
                let mut guard = state_bg.lock().ok();
                if let Some(ref mut g) = guard {
                    if let Some(ref mut child) = g.child {
                        matches!(child.try_wait(), Ok(Some(_)))
                    } else {
                        false
                    }
                } else {
                    true
                }
            };
            if exited {
                let mut status = current_status(&state_bg);
                if status.phase == ConnPhase::Connected || status.phase == ConnPhase::Connecting {
                    status.phase = ConnPhase::Error;
                    status.message = "OpenVPN process ended".into();
                    set_status(&state_bg, &app_bg, status);
                }
                break;
            }
            if started.elapsed() > Duration::from_secs(3600 * 24) {
                break;
            }
            thread::sleep(Duration::from_millis(400));
        }
    });

    Ok(())
}

fn management_release_hold(port: u16) -> Result<(), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let _ = writeln!(stream, "hold release");
    let _ = stream.flush();
    Ok(())
}

fn spawn_openvpn(bin: &Path, args: &[String], elevate: bool) -> Result<Child, String> {
    // Never pipe stdout/stderr without a reader: a full pipe freezes OpenVPN and the UI.
    #[cfg(target_os = "linux")]
    {
        if elevate {
            let mut cmd = Command::new("pkexec");
            cmd.arg(bin);
            for a in args {
                cmd.arg(a);
            }
            return cmd
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| {
                    format!(
						"Could not start elevated OpenVPN via pkexec: {e}. Install polkit and try again."
					)
                });
        }
    }

    #[cfg(target_os = "windows")]
    {
        if elevate {
            let _ = elevate;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if elevate {
            let _ = elevate;
        }
    }

    Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Could not start bundled OpenVPN: {e}"))
}

pub fn ensure_runtime_dir() -> Result<PathBuf, PathError> {
    let dir = paths::runtime_dir()?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}
