//! GlobalProtect tunnel via bundled gpclient / gpauth.

use crate::kill_switch;
use crate::openvpn::{ConnPhase, ConnStatus};
use crate::paths::{self, PathError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const GP_INDEX: &str = "gp-profiles.json";
const MAX_LOG_BLOCKS: usize = 20;

#[derive(Debug, thiserror::Error)]
pub enum GpError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid json")]
    InvalidJson,
    #[allow(dead_code)]
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpHip {
    pub enabled: bool,
    pub script_path: Option<String>,
    pub hip_user: Option<String>,
}

impl Default for GpHip {
    fn default() -> Self {
        Self {
            enabled: false,
            script_path: None,
            hip_user: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpAdvanced {
    pub mtu: Option<u32>,
    pub no_dtls: bool,
    pub disable_ipv6: bool,
    pub os: Option<String>,
    pub client_version: Option<String>,
    pub interface: Option<String>,
    pub reconnect_timeout: u32,
    pub no_xmlpost: bool,
    pub csc: String,
    pub local_hostname: Option<String>,
    pub dpd_interval: Option<u32>,
    pub clean_cookie: bool,
    pub cookie_cache_path: Option<String>,
    pub script: Option<String>,
    pub script_tun: bool,
}

impl Default for GpAdvanced {
    fn default() -> Self {
        Self {
            mtu: None,
            no_dtls: false,
            disable_ipv6: false,
            os: None,
            client_version: None,
            interface: None,
            reconnect_timeout: 300,
            no_xmlpost: false,
            csc: "auto".into(),
            local_hostname: None,
            dpd_interval: None,
            clean_cookie: false,
            cookie_cache_path: None,
            script: None,
            script_tun: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpPaths {
    pub certificate: Option<String>,
    pub ssl_key: Option<String>,
    pub hip_script: Option<String>,
    pub cookie_cache: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpProfileIcon {
    #[serde(default = "default_icon_kind")]
    pub kind: String,
    #[serde(default = "default_icon_builtin")]
    pub builtin: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub image_path: Option<String>,
}

fn default_icon_kind() -> String {
    "builtin".into()
}

fn default_icon_builtin() -> String {
    "globe".into()
}

impl Default for GpProfileIcon {
    fn default() -> Self {
        Self {
            kind: default_icon_kind(),
            builtin: default_icon_builtin(),
            color: String::new(),
            image_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpProfile {
    pub id: String,
    pub name: String,
    pub portal: String,
    pub gateway: Option<String>,
    pub auto_gateway: bool,
    pub as_gateway: bool,
    pub username: Option<String>,
    pub auth_mode: String,
    pub browser: Option<String>,
    pub certificate_path: Option<String>,
    pub ssl_key_path: Option<String>,
    pub hip: GpHip,
    pub advanced: GpAdvanced,
    pub paths: GpPaths,
    #[serde(default)]
    pub icon: GpProfileIcon,
    pub last_used_ms: Option<u64>,
    pub created_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GpProfilesFile {
    profiles: Vec<GpProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpConnectRequest {
    pub profile_id: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub key_passphrase: Option<String>,
    pub remember: bool,
    pub store: Option<String>,
    pub gateway: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpAuthResponse {
    pub password_type: String,
    pub password: String,
}

pub struct GpTunnelState {
    pub status: ConnStatus,
    pub child: Option<Child>,
    pub log_lines: Vec<String>,
    pub stop_flag: Arc<AtomicBool>,
    pub kill_switch: bool,
    /// True when the active tunnel was started via pkexec / elevated privileges.
    pub elevated: bool,
    /// Bumped on each connect/disconnect so stale worker threads cannot overwrite status.
    pub session_gen: u64,
    pub pending_auth: Option<(String, String)>,
}

impl Default for GpTunnelState {
    fn default() -> Self {
        Self {
            status: ConnStatus {
                protocol: "globalprotect".into(),
                ..ConnStatus::default()
            },
            child: None,
            log_lines: Vec::new(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            kill_switch: false,
            elevated: false,
            session_gen: 0,
            pending_auth: None,
        }
    }
}

pub type SharedGpTunnel = Arc<Mutex<GpTunnelState>>;

pub fn new_shared_gp_tunnel() -> SharedGpTunnel {
    Arc::new(Mutex::new(GpTunnelState::default()))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn gp_index_path() -> Result<PathBuf, GpError> {
    Ok(paths::data_dir()?.join(GP_INDEX))
}

fn gp_profile_dir(id: &str) -> Result<PathBuf, GpError> {
    Ok(paths::gp_profiles_dir()?.join(id))
}

pub fn load_gp_profiles_disk() -> Result<Vec<GpProfile>, GpError> {
    paths::ensure_app_dirs()?;
    let path = gp_index_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path)?;
    let file: GpProfilesFile = serde_json::from_str(&raw).map_err(|_| GpError::InvalidJson)?;
    Ok(file.profiles)
}

pub fn save_gp_profiles_disk_pub(profiles: &[GpProfile]) -> Result<(), GpError> {
    save_gp_profiles_disk(profiles)
}

fn save_gp_profiles_disk(profiles: &[GpProfile]) -> Result<(), GpError> {
    paths::ensure_app_dirs()?;
    let path = gp_index_path()?;
    let file = GpProfilesFile {
        profiles: profiles.to_vec(),
    };
    let raw = serde_json::to_string_pretty(&file).map_err(|_| GpError::InvalidJson)?;
    fs::write(path, raw)?;
    Ok(())
}

fn bundled_bin(app: &AppHandle, name: &str, env_key: &str) -> Result<PathBuf, String> {
    if let Ok(override_bin) = std::env::var(env_key) {
        let p = PathBuf::from(override_bin);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("{env_key} is set but is not a file"));
    }
    let root = resource_gp_dir(app)?;
    let candidate = root.join(name);
    #[cfg(windows)]
    let candidate = root.join(format!("{name}.exe"));
    if candidate.is_file() {
        return Ok(candidate);
    }
    Err(format!(
        "Bundled {name} not found at {}. Set {env_key} for development.",
        candidate.display()
    ))
}

fn resource_gp_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let resource = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("gpclient");
    let arch = gp_arch_subdir();
    let nested = resource.join(arch);
    if nested.is_dir() {
        return Ok(nested);
    }
    if resource.is_dir() {
        return Ok(resource);
    }
    Ok(nested)
}

fn gp_arch_subdir() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos-x86_64"
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        "windows-x86_64"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        "linux-x86_64"
    }
}

fn set_gp_status(state: &SharedGpTunnel, app: &AppHandle, status: ConnStatus) {
    if let Ok(mut guard) = state.lock() {
        guard.status = status.clone();
    }
    let _ = app.emit("portal://gp-status", &status);
    crate::tray::on_connection_status(app, &status);
}

fn push_gp_log(state: &SharedGpTunnel, app: &AppHandle, line: String) {
    if let Ok(mut guard) = state.lock() {
        guard.log_lines.push(line.clone());
        let excess = guard.log_lines.len().saturating_sub(MAX_LOG_BLOCKS);
        if excess > 0 {
            guard.log_lines.drain(0..excess);
        }
    }
    let _ = app.emit("portal://gp-log", line);
}

pub fn current_gp_status(state: &SharedGpTunnel) -> ConnStatus {
    state
        .lock()
        .map(|g| g.status.clone())
        .unwrap_or_else(|_| ConnStatus {
            protocol: "globalprotect".into(),
            ..ConnStatus::default()
        })
}

pub fn current_gp_logs(state: &SharedGpTunnel) -> Vec<String> {
    state
        .lock()
        .map(|g| g.log_lines.clone())
        .unwrap_or_default()
}

pub fn recent_gp_profiles(limit: usize) -> Vec<GpProfile> {
    let mut profiles = load_gp_profiles_disk().unwrap_or_default();
    profiles.sort_by(|a, b| {
        b.last_used_ms
            .unwrap_or(0)
            .cmp(&a.last_used_ms.unwrap_or(0))
            .then_with(|| b.created_ms.cmp(&a.created_ms))
            .then_with(|| a.name.cmp(&b.name))
    });
    profiles.truncate(limit);
    profiles
}

fn keyring_gp_entry(profile_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(
        "com.mariesta.menzies.portal-desktop-menzies",
        &format!("gp::{profile_id}"),
    )
    .map_err(|e| crate::portal::format_keyring_error(&e))
}

fn store_gp_credentials_in_session(
    profile_id: &str,
    username: &str,
    password: &str,
    session: &crate::portal::SessionSecrets,
) -> Result<(), String> {
    let mut map = session.lock().map_err(|e| e.to_string())?;
    map.insert(
        format!("gp::{profile_id}"),
        (username.to_string(), password.to_string()),
    );
    Ok(())
}

pub fn store_gp_credentials(
    profile_id: &str,
    username: &str,
    password: &str,
    store: &str,
    session: &crate::portal::SessionSecrets,
) -> Result<(), String> {
    if store == "session" {
        return store_gp_credentials_in_session(profile_id, username, password, session);
    }
    let entry = keyring_gp_entry(profile_id)?;
    let payload = serde_json::json!({ "username": username, "password": password });
    match entry.set_password(&payload.to_string()) {
        Ok(()) => Ok(()),
        Err(e) => {
            log::warn!(
                "GP keyring store failed; keeping credentials in session only: {}",
                e
            );
            store_gp_credentials_in_session(profile_id, username, password, session)
        }
    }
}

pub fn load_gp_credentials(
    profile_id: &str,
    session: &crate::portal::SessionSecrets,
) -> Result<Option<(String, String)>, String> {
    {
        let map = session.lock().map_err(|e| e.to_string())?;
        if let Some(pair) = map.get(&format!("gp::{profile_id}")) {
            return Ok(Some(pair.clone()));
        }
    }
    let entry = match keyring_gp_entry(profile_id) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("GP keyring entry unavailable: {e}");
            return Ok(None);
        }
    };
    match entry.get_password() {
        Ok(raw) => {
            let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let u = v
                .get("username")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let p = v
                .get("password")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Some((u, p)))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => {
            // Locked / timed-out Secret Service must not block connect. Prompt instead.
            log::warn!("GP keyring read failed: {e}");
            Ok(None)
        }
    }
}

pub fn clear_gp_credentials(
    profile_id: &str,
    session: &crate::portal::SessionSecrets,
) -> Result<(), String> {
    if let Ok(mut map) = session.lock() {
        map.remove(&format!("gp::{profile_id}"));
        map.remove(&format!("gp::{profile_id}::key"));
    }
    if let Ok(entry) = keyring_gp_entry(profile_id) {
        let _ = entry.delete_credential();
    }
    Ok(())
}

fn build_gpclient_args(
    profile: &GpProfile,
    username: &str,
    gateway_override: Option<&str>,
    cookie_cache: bool,
    ignore_tls: bool,
    fix_openssl: bool,
    default_os: &str,
    password_via_stdin: bool,
    cookie_via_stdin: bool,
) -> Vec<String> {
    let mut args = vec!["connect".into(), profile.portal.clone()];

    if fix_openssl {
        args.insert(0, "--fix-openssl".into());
    }
    if ignore_tls {
        args.insert(0, "--ignore-tls-errors".into());
    }

    let gw = gateway_override
        .map(str::to_string)
        .or_else(|| profile.gateway.clone());
    // --as-gateway treats SERVER as a gateway. Do not combine with portal auto-gateway.
    if profile.as_gateway {
        args.push("--as-gateway".into());
    } else if profile.auto_gateway {
        args.push("--auto-gateway".into());
    } else if let Some(g) = gw {
        if !g.is_empty() {
            args.push("--gateway".into());
            args.push(g);
        }
    }
    if !username.is_empty() {
        args.push("--user".into());
        args.push(username.to_string());
    }
    if password_via_stdin {
        args.push("--passwd-on-stdin".into());
        // Stale portal cookies after disconnect cause reconnect auth-failed while a
        // fresh password is available on stdin. Always re-auth with that password.
        if !profile.advanced.clean_cookie {
            args.push("--clean".into());
        }
    }
    if cookie_via_stdin {
        args.push("--cookie-on-stdin".into());
    }

    let cert = profile
        .paths
        .certificate
        .clone()
        .or_else(|| profile.certificate_path.clone());
    if let Some(c) = cert {
        if !c.is_empty() {
            args.push("--certificate".into());
            args.push(c);
        }
    }
    let key = profile
        .paths
        .ssl_key
        .clone()
        .or_else(|| profile.ssl_key_path.clone());
    if let Some(k) = key {
        if !k.is_empty() {
            args.push("--sslkey".into());
            args.push(k);
        }
    }

    if profile.hip.enabled {
        let script = profile
            .hip
            .script_path
            .clone()
            .or_else(|| profile.paths.hip_script.clone());
        match script {
            Some(s) if !s.is_empty() => {
                args.push("--hip".into());
                args.push(s);
            }
            _ => {
                args.push("--hip".into());
            }
        }
        if let Some(u) = &profile.hip.hip_user {
            if !u.is_empty() {
                args.push("--hip-user".into());
                args.push(u.clone());
            }
        }
    }

    let adv = &profile.advanced;
    if let Some(mtu) = adv.mtu {
        args.push("--mtu".into());
        args.push(mtu.to_string());
    }
    if adv.no_dtls {
        args.push("--no-dtls".into());
    }
    if adv.disable_ipv6 {
        args.push("--disable-ipv6".into());
    }
    let os = adv
        .os
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_os.to_string());
    if !os.is_empty() {
        args.push("--os".into());
        args.push(os);
    }
    if let Some(cv) = &adv.client_version {
        if !cv.is_empty() {
            args.push("--client-version".into());
            args.push(cv.clone());
        }
    }
    if let Some(iface) = &adv.interface {
        if !iface.is_empty() {
            args.push("--interface".into());
            args.push(iface.clone());
        }
    }
    args.push("--reconnect-timeout".into());
    args.push(adv.reconnect_timeout.to_string());
    if adv.no_xmlpost {
        args.push("--no-xmlpost".into());
    }
    if !adv.csc.is_empty() && adv.csc != "auto" {
        args.push("--csc".into());
        args.push(adv.csc.clone());
    }
    if let Some(h) = &adv.local_hostname {
        if !h.is_empty() {
            args.push("--local-hostname".into());
            args.push(h.clone());
        }
    }
    if let Some(dpd) = adv.dpd_interval {
        args.push("--force-dpd".into());
        args.push(dpd.to_string());
    }
    if let Some(script) = &adv.script {
        if !script.is_empty() {
            args.push("--script".into());
            args.push(script.clone());
        }
    }
    if adv.script_tun {
        args.push("--script-tun".into());
    }

    if cookie_cache {
        let cache_path = adv
            .cookie_cache_path
            .clone()
            .or_else(|| profile.paths.cookie_cache.clone())
            .filter(|p| !p.is_empty())
            .or_else(|| {
                // Per-profile cache only. A bare --cookie-cache is shared across
                // accounts and can make one login look "special" vs another.
                gp_profile_dir(&profile.id).ok().map(|d| {
                    let _ = fs::create_dir_all(&d);
                    d.join("portal-cookie.json").to_string_lossy().into_owned()
                })
            });
        if let Some(path) = cache_path {
            args.push("--cookie-cache".into());
            args.push(path);
        }
    }
    if adv.clean_cookie {
        args.push("--clean".into());
    }

    let auth = profile.auth_mode.as_str();
    if auth == "browser" || auth == "mixed" {
        let browser = profile
            .browser
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "auto".into());
        args.push("--browser".into());
        args.push(browser);
    }

    args
}

fn run_gpauth_cookie(
    app: &AppHandle,
    profile: &GpProfile,
    browser_mode: &str,
    ignore_tls: bool,
    fix_openssl: bool,
) -> Result<String, String> {
    let gpauth = bundled_bin(app, "gpauth", "PORTAL_GPAUTH_BIN")?;
    let mut cmd = Command::new(&gpauth);
    if fix_openssl {
        cmd.arg("--fix-openssl");
    }
    if ignore_tls {
        cmd.arg("--ignore-tls-errors");
    }
    cmd.arg(&profile.portal);
    if browser_mode == "embedded" || browser_mode == "webview" {
        // Default gpauth webview (no --browser).
    } else {
        cmd.arg("--browser");
        if browser_mode == "auto" {
            // flag alone = auto
        } else {
            cmd.arg(browser_mode);
        }
    }
    if let Some(cert) = profile
        .paths
        .certificate
        .as_ref()
        .or(profile.certificate_path.as_ref())
    {
        if !cert.is_empty() {
            cmd.arg("--certificate").arg(cert);
        }
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = cmd.output().map_err(|e| format!("gpauth failed: {e}"))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gpauth exited with error: {err}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn gpclient_connect_pids(gpclient: &Path) -> Vec<u32> {
    let Ok(output) = Command::new("ps").args(["-eo", "pid=,args="]).output() else {
        return Vec::new();
    };
    let hay = gpclient.display().to_string();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if !line.contains(&hay) || !line.contains(" connect ") {
                return None;
            }
            line.split_whitespace()
                .next()
                .and_then(|p| p.parse::<u32>().ok())
        })
        .collect()
}

/// Run a command with a hard wall clock limit so disconnect never hangs the UI
/// (pkexec polkit prompts and stuck gpclient disconnect are the usual causes).
fn run_with_timeout(mut cmd: Command, timeout: Duration) {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else {
        return;
    };
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(100)),
            Err(_) => return,
        }
    }
}

fn run_gpclient_disconnect_user(gpclient: &Path) {
    let mut cmd = Command::new(gpclient);
    cmd.arg("disconnect");
    run_with_timeout(cmd, Duration::from_secs(3));
}

fn run_gpclient_disconnect_elevated(gpclient: &Path) {
    #[cfg(target_os = "linux")]
    {
        let mut cmd = Command::new("pkexec");
        cmd.arg(gpclient).arg("disconnect");
        // Polkit may wait for a password; do not block the disconnect IPC forever.
        run_with_timeout(cmd, Duration::from_secs(12));
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = gpclient;
    }
}

fn wait_for_gpclient_exit(gpclient: &Path, timeout: Duration) {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if gpclient_connect_pids(gpclient).is_empty() {
            return;
        }
        thread::sleep(Duration::from_millis(150));
    }
}

fn wait_child_brief(mut child: Child, timeout: Duration) {
    let _ = child.kill();
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                // Detach: do not block disconnect if pkexec/gpclient ignores signals.
                thread::spawn(move || {
                    let _ = child.wait();
                });
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => return,
        }
    }
}

pub fn disconnect_gp(state: &SharedGpTunnel, app: &AppHandle) -> Result<(), String> {
    let (kill_switch, child, elevated) = {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.stop_flag.store(true, Ordering::SeqCst);
        guard.session_gen = guard.session_gen.wrapping_add(1);
        let child = guard.child.take();
        let elevated = guard.elevated;
        guard.elevated = false;
        (guard.kill_switch, child, elevated)
    };

    // Flip UI to Idle first so Disconnect never looks stuck.
    set_gp_status(
        state,
        app,
        ConnStatus {
            phase: ConnPhase::Idle,
            profile_id: None,
            message: "Disconnected".into(),
            vpn_ip: None,
            started_at_ms: None,
            management_port: None,
            protocol: "globalprotect".into(),
            interface_name: None,
        },
    );

    if let Some(child) = child {
        wait_child_brief(child, Duration::from_millis(800));
    }

    let gpclient = match bundled_bin(app, "gpclient", "PORTAL_GPCLIENT_BIN") {
        Ok(p) => p,
        Err(_) => {
            if kill_switch {
                let _ = kill_switch::unregister_interface("globalprotect");
            }
            return Ok(());
        }
    };

    // Fast path: user-level disconnect can signal root gpclient on Linux.
    run_gpclient_disconnect_user(&gpclient);
    wait_for_gpclient_exit(&gpclient, Duration::from_secs(2));

    let still_running = !gpclient_connect_pids(&gpclient).is_empty();
    if still_running && elevated {
        // Finish elevated cleanup off the IPC path so polkit cannot freeze the button.
        let gp = gpclient.clone();
        thread::spawn(move || {
            run_gpclient_disconnect_elevated(&gp);
            wait_for_gpclient_exit(&gp, Duration::from_secs(3));
            if !gpclient_connect_pids(&gp).is_empty() {
                run_gpclient_disconnect_user(&gp);
            }
        });
    } else if still_running {
        // Unelevated leftovers: one more quick pass, then give up without hanging.
        run_gpclient_disconnect_user(&gpclient);
        wait_for_gpclient_exit(&gpclient, Duration::from_secs(1));
    }

    if kill_switch {
        let _ = kill_switch::unregister_interface("globalprotect");
    }

    Ok(())
}

pub struct GpConnectArgs {
    pub profile: GpProfile,
    pub username: String,
    pub password: String,
    pub key_passphrase: Option<String>,
    pub kill_switch: bool,
    pub elevate: bool,
    pub cookie_cache: bool,
    pub ignore_tls: bool,
    pub fix_openssl: bool,
    pub default_os: String,
    pub default_browser: String,
    pub gateway_override: Option<String>,
}

pub fn connect_gp_tunnel(
    state: SharedGpTunnel,
    app: AppHandle,
    args: GpConnectArgs,
) -> Result<(), String> {
    {
        let guard = state.lock().map_err(|e| e.to_string())?;
        if guard.status.phase == ConnPhase::Connecting || guard.status.phase == ConnPhase::Connected
        {
            return Err("A GlobalProtect tunnel is already active".into());
        }
    }

    let gpclient = bundled_bin(&app, "gpclient", "PORTAL_GPCLIENT_BIN")?;
    let stop_flag = Arc::new(AtomicBool::new(false));
    let auth_mode = args.profile.auth_mode.clone();
    let want_browser = matches!(auth_mode.as_str(), "browser" | "webview" | "mixed");
    let mut use_browser = want_browser;
    let mut use_password = matches!(auth_mode.as_str(), "password" | "mixed" | "cert")
        || (!want_browser && !args.password.is_empty());

    let mut cookie_stdin = None;
    if want_browser && (auth_mode == "webview" || auth_mode == "browser" || auth_mode == "mixed") {
        let mode = if auth_mode == "webview" {
            "embedded"
        } else {
            args.profile
                .browser
                .as_deref()
                .unwrap_or(args.default_browser.as_str())
        };
        match run_gpauth_cookie(&app, &args.profile, mode, args.ignore_tls, args.fix_openssl) {
            Ok(cookie) => cookie_stdin = Some(cookie),
            Err(err) => {
                let lower = err.to_lowercase();
                // Many portals advertise saml-default-browser but still return a password form.
                // Falling through to elevated gpclient --browser then fails with "not a TTY".
                if lower.contains("non-saml") || lower.contains("not a saml") {
                    push_gp_log(
                        &state,
                        &app,
                        format!(
                            "gpauth: {err}. Portal is not using browser SSO; using username and password instead."
                        ),
                    );
                    use_browser = false;
                    use_password = true;
                    if args.password.is_empty() {
                        set_gp_status(
                            &state,
                            &app,
                            ConnStatus {
                                phase: ConnPhase::Error,
                                profile_id: Some(args.profile.id.clone()),
                                message: "This portal does not offer Microsoft browser SSO. Switch Authentication to Username and password, then connect with your VPN account password (it may differ from your Outlook password).".into(),
                                vpn_ip: None,
                                started_at_ms: None,
                                management_port: None,
                                protocol: "globalprotect".into(),
                                interface_name: None,
                            },
                        );
                        return Err(
                            "Portal does not offer browser SSO; use Username and password".into(),
                        );
                    }
                } else {
                    push_gp_log(
                        &state,
                        &app,
                        format!("gpauth: {err}; trying gpclient browser"),
                    );
                }
            }
        }
    }

    let cookie_via_stdin = cookie_stdin.is_some();
    let password_via_stdin = use_password && !cookie_via_stdin && !args.password.is_empty();

    // Do not pass --browser on an elevated (pkexec) process: the SSO window cannot open as root.
    let mut profile_for_args = args.profile.clone();
    if !use_browser {
        if profile_for_args.auth_mode == "browser" || profile_for_args.auth_mode == "webview" {
            profile_for_args.auth_mode = "password".into();
        }
    }

    let mut cli_args = build_gpclient_args(
        &profile_for_args,
        &args.username,
        args.gateway_override.as_deref(),
        args.cookie_cache,
        args.ignore_tls,
        args.fix_openssl,
        &args.default_os,
        password_via_stdin,
        cookie_via_stdin,
    );

    // Unelevated browser fallback only when gpauth did not already prove non-SAML.
    if use_browser && !cookie_via_stdin && !cli_args.iter().any(|a| a == "--browser") {
        cli_args.push("--browser".into());
        let b = args
            .profile
            .browser
            .clone()
            .unwrap_or_else(|| args.default_browser.clone());
        if b != "auto" {
            cli_args.push(b);
        }
    }

    // Browser SSO must not run under pkexec (no user display). Password tunnels may elevate.
    let elevate = args.elevate && !(use_browser && !cookie_via_stdin);

    let has_client_cert = profile_for_args
        .paths
        .certificate
        .as_ref()
        .or(profile_for_args.certificate_path.as_ref())
        .is_some_and(|c| !c.is_empty());
    if has_client_cert {
        if let Some(pass) = &args.key_passphrase {
            if !pass.is_empty() {
                cli_args.push("--key-password".into());
                cli_args.push(pass.clone());
            }
        }
    }

    // Ensure no leftover connect process still owns tun before starting again.
    if let Ok(existing) = bundled_bin(&app, "gpclient", "PORTAL_GPCLIENT_BIN") {
        if !gpclient_connect_pids(&existing).is_empty() {
            push_gp_log(
                &state,
                &app,
                "Previous GlobalProtect process still running; disconnecting before reconnect"
                    .into(),
            );
            run_gpclient_disconnect_user(&existing);
            wait_for_gpclient_exit(&existing, Duration::from_secs(2));
            if !gpclient_connect_pids(&existing).is_empty() {
                run_gpclient_disconnect_elevated(&existing);
                wait_for_gpclient_exit(&existing, Duration::from_secs(3));
            }
        }
    }

    let session_gen = {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.stop_flag = stop_flag.clone();
        guard.kill_switch = args.kill_switch;
        guard.elevated = elevate;
        guard.session_gen = guard.session_gen.wrapping_add(1);
        guard.log_lines.clear();
        guard.child = None;
        guard.status = ConnStatus {
            phase: ConnPhase::Connecting,
            profile_id: Some(args.profile.id.clone()),
            message: "Connecting GlobalProtect".into(),
            vpn_ip: None,
            started_at_ms: Some(now_ms()),
            management_port: None,
            protocol: "globalprotect".into(),
            interface_name: args.profile.advanced.interface.clone(),
        };
        guard.session_gen
    };
    set_gp_status(&state, &app, current_gp_status(&state));

    let state_bg = state.clone();
    let app_bg = app.clone();
    let kill_switch = args.kill_switch;
    let password = args.password.clone();
    let cookie_data = cookie_stdin.clone();
    let iface = args
        .profile
        .advanced
        .interface
        .clone()
        .unwrap_or_else(|| "tun0".into());

    if use_password && !password_via_stdin && !cookie_via_stdin {
        set_gp_status(
            &state,
            &app,
            ConnStatus {
                phase: ConnPhase::Error,
                profile_id: Some(args.profile.id.clone()),
                message: "Username and password are required. This portal uses portal login, not Microsoft browser SSO.".into(),
                vpn_ip: None,
                started_at_ms: None,
                management_port: None,
                protocol: "globalprotect".into(),
                interface_name: None,
            },
        );
        return Err("Username and password are required for this portal".into());
    }

    thread::spawn(move || {
        if kill_switch {
            if let Err(err) = kill_switch::register_interface("globalprotect") {
                push_gp_log(&state_bg, &app_bg, format!("Kill switch: {err}"));
                let mut status = current_gp_status(&state_bg);
                status.phase = ConnPhase::Error;
                status.message = err;
                status.started_at_ms = None;
                set_gp_status(&state_bg, &app_bg, status);
                return;
            }
        }

        let mut cmd = if elevate {
            #[cfg(target_os = "linux")]
            {
                let mut c = Command::new("pkexec");
                c.arg(&gpclient);
                c
            }
            #[cfg(not(target_os = "linux"))]
            {
                Command::new(&gpclient)
            }
        } else {
            Command::new(&gpclient)
        };

        for a in &cli_args {
            cmd.arg(a);
        }
        push_gp_log(
            &state_bg,
            &app_bg,
            format!(
                "Starting: {}{} {}",
                if elevate { "pkexec " } else { "" },
                gpclient.display(),
                cli_args.join(" ")
            ),
        );
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let err = format!("Could not start gpclient: {e}");
                push_gp_log(&state_bg, &app_bg, err.clone());
                let mut status = current_gp_status(&state_bg);
                status.phase = ConnPhase::Error;
                status.message = err;
                status.started_at_ms = None;
                set_gp_status(&state_bg, &app_bg, status);
                return;
            }
        };

        if let Some(mut stdin) = child.stdin.take() {
            if let Some(cookie) = &cookie_data {
                let _ = write!(stdin, "{cookie}");
            } else if password_via_stdin {
                let _ = writeln!(stdin, "{password}");
            }
            let _ = stdin.flush();
        }

        {
            if let Ok(mut guard) = state_bg.lock() {
                if guard.stop_flag.load(Ordering::SeqCst) {
                    let _ = child.kill();
                    return;
                }
                guard.child = Some(child);
            }
        }

        // Re-borrow child for IO: take stdout/stderr from stored child.
        let (stdout, stderr) = {
            let mut guard = match state_bg.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let child = match guard.child.as_mut() {
                Some(c) => c,
                None => return,
            };
            (child.stdout.take(), child.stderr.take())
        };

        let state_out = state_bg.clone();
        let app_out = app_bg.clone();
        let iface_out = iface.clone();
        if let Some(out) = stdout {
            thread::spawn(move || {
                let reader = BufReader::new(out);
                for line in reader.lines().flatten() {
                    handle_gp_output_line(&state_out, &app_out, &line, &iface_out);
                }
            });
        }
        let state_err = state_bg.clone();
        let app_err = app_bg.clone();
        let iface_err = iface.clone();
        if let Some(err) = stderr {
            thread::spawn(move || {
                let reader = BufReader::new(err);
                for line in reader.lines().flatten() {
                    handle_gp_output_line(&state_err, &app_err, &line, &iface_err);
                }
            });
        }

        // Wait for process end.
        let status_code = {
            loop {
                if stop_flag.load(Ordering::SeqCst) {
                    break None;
                }
                let mut guard = match state_bg.lock() {
                    Ok(g) => g,
                    Err(_) => break None,
                };
                if let Some(child) = guard.child.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => break Some(status),
                        Ok(None) => {}
                        Err(_) => break None,
                    }
                } else {
                    break None;
                }
                drop(guard);
                thread::sleep(Duration::from_millis(400));
            }
        };

        if stop_flag.load(Ordering::SeqCst) {
            return;
        }

        // Another connect/disconnect replaced this session; do not overwrite its status.
        {
            let guard = match state_bg.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            if guard.session_gen != session_gen {
                return;
            }
        }

        let phase = current_gp_status(&state_bg).phase;
        if phase == ConnPhase::Connected {
            // unexpected drop
            let mut status = current_gp_status(&state_bg);
            status.phase = ConnPhase::Error;
            status.message = "GlobalProtect tunnel ended unexpectedly".into();
            status.started_at_ms = None;
            if let Ok(mut guard) = state_bg.lock() {
                guard.elevated = false;
            }
            set_gp_status(&state_bg, &app_bg, status);
        } else if phase == ConnPhase::Connecting {
            let mut status = current_gp_status(&state_bg);
            status.phase = ConnPhase::Error;
            let prior = status.message.trim().to_string();
            let prior_is_detail = !prior.is_empty()
                && prior != "Connecting GlobalProtect"
                && !prior.starts_with("Starting:");
            let log_hint = {
                let guard = state_bg.lock().ok();
                guard.and_then(|g| {
                    g.log_lines
                        .iter()
                        .rev()
                        .find(|l| {
                            let t = l.to_lowercase();
                            t.contains("auth-failed")
                                || t.contains("non-saml")
                                || t.contains("not a tty")
                                || t.contains("portal config error")
                                || t.contains("gateway login")
                                || t.contains("device or resource busy")
                        })
                        .cloned()
                })
            };
            status.message = if prior_is_detail {
                prior
            } else if let Some(hint) = log_hint {
                let lower = hint.to_lowercase();
                if lower.contains("auth-failed") {
                    format!(
                        "{hint}. The portal rejected this username or password. Outlook / Microsoft 365 password is not always the VPN password. Certificates are not required."
                    )
                } else if lower.contains("non-saml") {
                    format!("{hint}. This portal uses Username and password, not browser SSO.")
                } else if lower.contains("busy") {
                    format!(
                        "{hint}. A previous tunnel may still be active. Disconnect, wait a moment, then connect again."
                    )
                } else {
                    hint
                }
            } else {
                match status_code {
                    Some(s) if !s.success() => format!(
                        "gpclient exited: {s}. If this followed a reconnect, disconnect fully first. Use Username and password; Ignore TLS errors if the portal uses a private certificate."
                    ),
                    Some(_) => "GlobalProtect connect ended before the tunnel was ready".into(),
                    None => "GlobalProtect connect failed".into(),
                }
            };
            status.started_at_ms = None;
            if let Ok(mut guard) = state_bg.lock() {
                guard.elevated = false;
            }
            set_gp_status(&state_bg, &app_bg, status);
        }

        if kill_switch {
            let _ = kill_switch::unregister_interface("globalprotect");
        }
    });

    Ok(())
}

fn handle_gp_output_line(state: &SharedGpTunnel, app: &AppHandle, line: &str, iface: &str) {
    push_gp_log(state, app, line.to_string());
    let lower = line.to_lowercase();

    if lower.contains("password:")
        || lower.contains("challenge")
        || lower.contains("enter tokencode")
        || lower.contains("verification code")
    {
        let _ = app.emit(
            "portal://gp-auth-challenge",
            serde_json::json!({
                "prompt": line,
                "passwordType": "Auth"
            }),
        );
    }

    if lower.contains("auth-failed")
        || lower.contains("invalid username or password")
        || lower.contains("portal config error")
        || lower.contains("gateway login error")
        || lower.contains("authentication failed")
    {
        let mut status = current_gp_status(state);
        if status.phase == ConnPhase::Connecting {
            status.message = line.trim().to_string();
            set_gp_status(state, app, status);
        }
    }

    if lower.contains("connected")
        || lower.contains("esp session established")
        || lower.contains("tunnel connected")
        || lower.contains("connection established")
    {
        let mut status = current_gp_status(state);
        status.phase = ConnPhase::Connected;
        status.message = "Connected".into();
        status.interface_name = Some(iface.to_string());
        if status.vpn_ip.is_none() {
            if let Some(ip) = extract_ipv4(line) {
                status.vpn_ip = Some(ip);
            }
        }
        set_gp_status(state, app, status);
        let _ = app.emit(
            "portal://traffic",
            serde_json::json!({
                "protocol": "globalprotect",
                "downBps": 0,
                "upBps": 0,
                "bytesIn": 0,
                "bytesOut": 0
            }),
        );
    }

    if let Some(ip) = extract_ipv4(line) {
        if lower.contains("ip") || lower.contains("address") || lower.contains("inet") {
            let mut status = current_gp_status(state);
            status.vpn_ip = Some(ip);
            set_gp_status(state, app, status);
        }
    }
}

fn extract_ipv4(line: &str) -> Option<String> {
    for part in line.split(|c: char| !c.is_ascii_digit() && c != '.') {
        let octets: Vec<&str> = part.split('.').collect();
        if octets.len() == 4 && octets.iter().all(|o| o.parse::<u8>().is_ok()) {
            if part.starts_with("127.") || part.starts_with("0.") {
                continue;
            }
            return Some(part.to_string());
        }
    }
    None
}

pub fn store_gp_auth_response(state: &SharedGpTunnel, req: &GpAuthResponse) -> Result<(), String> {
    // Store for potential stdin injection on next prompt; gpclient MFA often via browser.
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.pending_auth = Some((req.password_type.clone(), req.password.clone()));
    Ok(())
}

// --- Tauri commands ---

#[tauri::command]
pub fn list_gp_profiles() -> Result<Vec<GpProfile>, String> {
    load_gp_profiles_disk().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_gp_profile(profile: GpProfile) -> Result<GpProfile, String> {
    let mut profiles = load_gp_profiles_disk().map_err(|e| e.to_string())?;
    let mut saved = profile;
    if saved.id.is_empty() {
        saved.id = Uuid::new_v4().to_string();
        saved.created_ms = now_ms();
        let dir = gp_profile_dir(&saved.id).map_err(|e| e.to_string())?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        profiles.push(saved.clone());
    } else if let Some(existing) = profiles.iter_mut().find(|p| p.id == saved.id) {
        saved.created_ms = existing.created_ms;
        *existing = saved.clone();
    } else {
        let dir = gp_profile_dir(&saved.id).map_err(|e| e.to_string())?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        if saved.created_ms == 0 {
            saved.created_ms = now_ms();
        }
        profiles.push(saved.clone());
    }
    // Persist categorical path meta beside profile.
    let meta_dir = gp_profile_dir(&saved.id).map_err(|e| e.to_string())?;
    let meta_path = meta_dir.join("meta.json");
    let raw = serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())?;
    fs::write(meta_path, raw).map_err(|e| e.to_string())?;
    save_gp_profiles_disk(&profiles).map_err(|e| e.to_string())?;
    Ok(saved)
}

#[tauri::command]
pub fn delete_gp_profile(
    id: String,
    session: tauri::State<'_, crate::portal::SessionSecrets>,
) -> Result<(), String> {
    let mut profiles = load_gp_profiles_disk().map_err(|e| e.to_string())?;
    profiles.retain(|p| p.id != id);
    save_gp_profiles_disk(&profiles).map_err(|e| e.to_string())?;
    let dir = gp_profile_dir(&id).map_err(|e| e.to_string())?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let _ = clear_gp_credentials(&id, &session);
    Ok(())
}

#[tauri::command]
pub fn rename_gp_profile(id: String, name: String) -> Result<(), String> {
    let mut profiles = load_gp_profiles_disk().map_err(|e| e.to_string())?;
    let Some(profile) = profiles.iter_mut().find(|p| p.id == id) else {
        return Err("Profile not found".into());
    };
    profile.name = name;
    save_gp_profiles_disk(&profiles).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn gp_connection_status(
    tunnel: tauri::State<'_, SharedGpTunnel>,
) -> Result<ConnStatus, String> {
    Ok(current_gp_status(&tunnel))
}

#[tauri::command]
pub fn gp_logs(tunnel: tauri::State<'_, SharedGpTunnel>) -> Result<Vec<String>, String> {
    Ok(current_gp_logs(&tunnel))
}

#[tauri::command(rename = "disconnect_gp")]
pub async fn disconnect_gp_cmd(
    app: AppHandle,
    tunnel: tauri::State<'_, SharedGpTunnel>,
) -> Result<(), String> {
    let state = tunnel.inner().clone();
    tauri::async_runtime::spawn_blocking(move || disconnect_gp(&state, &app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn respond_gp_auth(
    tunnel: tauri::State<'_, SharedGpTunnel>,
    req: GpAuthResponse,
) -> Result<(), String> {
    store_gp_auth_response(&tunnel, &req)
}

/// Legacy IPC. Prefer the frontend `@tauri-apps/plugin-dialog` `open()` path.
/// Kept async so a blocking dialog never runs on the webview main thread (Linux freeze/crash).
#[tauri::command]
pub async fn pick_gp_path(app: AppHandle, category: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let title = match category.as_str() {
        "certificate" => "Select client certificate",
        "sslKey" => "Select SSL private key",
        "hipScript" => "Select HIP script",
        "cookieCache" => "Select cookie cache file",
        "iconImage" => "Select profile icon image",
        _ => "Select file",
    };
    let mut builder = app.dialog().file().set_title(title);
    if category == "iconImage" {
        builder = builder.add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "svg"]);
    } else if category == "certificate" || category == "sslKey" {
        builder = builder.add_filter(
            "Certificates and keys",
            &["pem", "crt", "cer", "key", "p12", "pfx", "pkcs12"],
        );
    }
    let picked = builder.blocking_pick_file();
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn has_gp_stored_credentials(
    id: String,
    session: tauri::State<'_, crate::portal::SessionSecrets>,
) -> Result<bool, String> {
    Ok(load_gp_credentials(&id, &session)?.is_some())
}

#[tauri::command(rename = "clear_gp_credentials")]
pub fn clear_gp_credentials_cmd(
    id: String,
    session: tauri::State<'_, crate::portal::SessionSecrets>,
) -> Result<(), String> {
    clear_gp_credentials(&id, &session)
}

#[allow(dead_code)]
fn _path_exists_check(p: &Path) -> bool {
    p.exists()
}
