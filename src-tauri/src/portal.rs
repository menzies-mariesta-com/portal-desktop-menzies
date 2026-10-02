//! Portal profiles, settings, credentials, and VPN commands.

use crate::openvpn::{
    self, connect_tunnel, disconnect_tunnel, send_management_auth, ConnStatus, ConnectArgs,
    SharedTunnel,
};
use crate::paths::{self, PathError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

const SETTINGS_FILE: &str = "settings.json";
const PROFILES_INDEX: &str = "profiles.json";

#[derive(Debug, thiserror::Error)]
pub enum PortalError {
    #[error(transparent)]
    Path(#[from] PathError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid json")]
    InvalidJson,
    #[allow(dead_code)]
    #[error("profile not found")]
    NotFound,
    #[error("{0}")]
    Msg(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortalDiskSettings {
    pub onboarded: bool,
    pub auto_connect_profile_id: Option<String>,
    pub reconnect_on_drop: bool,
    pub kill_switch: bool,
    pub credential_store: String,
    pub elevate_on_connect: bool,
    pub tray_enabled: bool,
}

impl Default for PortalDiskSettings {
    fn default() -> Self {
        Self {
            onboarded: false,
            auto_connect_profile_id: None,
            reconnect_on_drop: false,
            kill_switch: false,
            credential_store: "keyring".into(),
            elevate_on_connect: true,
            tray_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub needs_user_pass: bool,
    pub needs_key_passphrase: bool,
    pub last_used_ms: Option<u64>,
    pub created_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ProfilesFile {
    profiles: Vec<ProfileSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub profile_id: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub key_passphrase: Option<String>,
    pub remember: bool,
    pub store: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponseRequest {
    pub password_type: String,
    pub password: String,
}

pub type SessionSecrets = Mutex<HashMap<String, (String, String)>>;

fn settings_path() -> Result<PathBuf, PortalError> {
    Ok(paths::config_dir()?.join(SETTINGS_FILE))
}

fn profiles_index_path() -> Result<PathBuf, PortalError> {
    Ok(paths::data_dir()?.join(PROFILES_INDEX))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn load_settings_disk() -> Result<PortalDiskSettings, PortalError> {
    paths::ensure_app_dirs()?;
    let path = settings_path()?;
    if !path.exists() {
        return Ok(PortalDiskSettings::default());
    }
    let raw = fs::read_to_string(&path)?;
    serde_json::from_str(&raw).map_err(|_| PortalError::InvalidJson)
}

fn save_settings_disk(settings: &PortalDiskSettings) -> Result<(), PortalError> {
    paths::ensure_app_dirs()?;
    let path = settings_path()?;
    let raw = serde_json::to_string_pretty(settings).map_err(|_| PortalError::InvalidJson)?;
    fs::write(path, raw)?;
    Ok(())
}

fn load_profiles_disk() -> Result<ProfilesFile, PortalError> {
    paths::ensure_app_dirs()?;
    let path = profiles_index_path()?;
    if !path.exists() {
        return Ok(ProfilesFile::default());
    }
    let raw = fs::read_to_string(&path)?;
    serde_json::from_str(&raw).map_err(|_| PortalError::InvalidJson)
}

fn save_profiles_disk(file: &ProfilesFile) -> Result<(), PortalError> {
    paths::ensure_app_dirs()?;
    let path = profiles_index_path()?;
    let raw = serde_json::to_string_pretty(file).map_err(|_| PortalError::InvalidJson)?;
    fs::write(path, raw)?;
    Ok(())
}

fn profile_dir(id: &str) -> Result<PathBuf, PortalError> {
    Ok(paths::profiles_dir()?.join(id))
}

fn ovpn_needs_user_pass(content: &str) -> bool {
    content
        .lines()
        .any(|l| l.trim().starts_with("auth-user-pass"))
}

fn ovpn_needs_key_pass(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("proc-type: 4,encrypted") || lower.contains("dek-info:")
}

fn rewrite_ovpn_paths(content: &str, base: &Path) -> String {
    let mut out = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        let mut rewritten = line.to_string();
        for key in ["ca", "cert", "key", "tls-auth", "tls-crypt", "pkcs12", "dh"] {
            if let Some(rest) = trimmed.strip_prefix(key) {
                let rest = rest.trim();
                if rest.is_empty() || rest.starts_with('[') {
                    break;
                }
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if let Some(first) = parts.first() {
                    if !first.starts_with('/') && !Path::new(first).is_absolute() {
                        let abs = base.join(first);
                        if abs.exists() {
                            let mut rebuilt = format!("{key} {}", abs.display());
                            for extra in parts.iter().skip(1) {
                                rebuilt.push(' ');
                                rebuilt.push_str(extra);
                            }
                            rewritten = rebuilt;
                        }
                    }
                }
                break;
            }
        }
        out.push_str(&rewritten);
        out.push('\n');
    }
    out
}

fn copy_sidecar_files(
    content: &str,
    source_dir: &Path,
    dest_dir: &Path,
) -> Result<(), PortalError> {
    for line in content.lines() {
        let trimmed = line.trim();
        for key in ["ca", "cert", "key", "tls-auth", "tls-crypt", "pkcs12", "dh"] {
            if let Some(rest) = trimmed.strip_prefix(key) {
                let rest = rest.trim();
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if let Some(first) = parts.first() {
                    if first.starts_with('[') {
                        continue;
                    }
                    let src = if Path::new(first).is_absolute() {
                        PathBuf::from(first)
                    } else {
                        source_dir.join(first)
                    };
                    if src.is_file() {
                        let name = src
                            .file_name()
                            .map(|s| s.to_os_string())
                            .unwrap_or_else(|| std::ffi::OsString::from("sidecar"));
                        let dest = dest_dir.join(&name);
                        fs::copy(&src, &dest)?;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(0o600));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn import_one_ovpn(path: &Path) -> Result<ProfileSummary, PortalError> {
    if !path.is_file() {
        return Err(PortalError::Msg(format!("Not a file: {}", path.display())));
    }
    let content = fs::read_to_string(path)?;
    let id = Uuid::new_v4().to_string();
    let dir = profile_dir(&id)?;
    fs::create_dir_all(&dir)?;
    let source_dir = path.parent().unwrap_or_else(|| Path::new("."));
    copy_sidecar_files(&content, source_dir, &dir)?;
    let rewritten = rewrite_ovpn_paths(&content, &dir);
    let cfg_path = dir.join("config.ovpn");
    fs::write(&cfg_path, &rewritten)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&cfg_path, fs::Permissions::from_mode(0o600));
    }
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Profile")
        .to_string();
    Ok(ProfileSummary {
        id,
        name,
        source_path: path.to_string_lossy().into_owned(),
        needs_user_pass: ovpn_needs_user_pass(&content),
        needs_key_passphrase: ovpn_needs_key_pass(&content),
        last_used_ms: None,
        created_ms: now_ms(),
    })
}

fn keyring_entry(profile_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("com.mariesta.menzies.portal-desktop-menzies", profile_id)
        .map_err(|e| e.to_string())
}

fn store_credentials(
    profile_id: &str,
    username: &str,
    password: &str,
    store: &str,
    session: &SessionSecrets,
) -> Result<(), String> {
    if store == "session" {
        let mut map = session.lock().map_err(|e| e.to_string())?;
        map.insert(
            profile_id.to_string(),
            (username.to_string(), password.to_string()),
        );
        return Ok(());
    }
    let entry = keyring_entry(profile_id)?;
    let payload = serde_json::json!({ "username": username, "password": password });
    entry
        .set_password(&payload.to_string())
        .map_err(|e| e.to_string())
}

fn load_credentials(
    profile_id: &str,
    session: &SessionSecrets,
) -> Result<Option<(String, String)>, String> {
    {
        let map = session.lock().map_err(|e| e.to_string())?;
        if let Some(pair) = map.get(profile_id) {
            return Ok(Some(pair.clone()));
        }
    }
    let entry = keyring_entry(profile_id)?;
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
        Err(e) => Err(e.to_string()),
    }
}

fn clear_credentials(profile_id: &str, session: &SessionSecrets) -> Result<(), String> {
    if let Ok(mut map) = session.lock() {
        map.remove(profile_id);
    }
    if let Ok(entry) = keyring_entry(profile_id) {
        let _ = entry.delete_credential();
    }
    Ok(())
}

#[tauri::command]
pub fn load_portal_settings() -> Result<PortalDiskSettings, String> {
    load_settings_disk().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_portal_settings(settings: PortalDiskSettings) -> Result<(), String> {
    save_settings_disk(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    load_profiles_disk()
        .map(|f| f.profiles)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_ovpn_file(app: AppHandle, path: String) -> Result<ProfileSummary, String> {
    let summary = import_one_ovpn(Path::new(&path)).map_err(|e| e.to_string())?;
    let mut file = load_profiles_disk().map_err(|e| e.to_string())?;
    file.profiles.push(summary.clone());
    save_profiles_disk(&file).map_err(|e| e.to_string())?;
    crate::tray::refresh_recent_menu(&app);
    Ok(summary)
}

#[tauri::command]
pub fn import_ovpn_folder(app: AppHandle, path: String) -> Result<Vec<ProfileSummary>, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err("Path is not a directory".into());
    }
    let mut imported = Vec::new();
    let walker = walkdir_ovpn(&root);
    let mut file = load_profiles_disk().map_err(|e| e.to_string())?;
    for ovpn in walker {
        match import_one_ovpn(&ovpn) {
            Ok(summary) => {
                file.profiles.push(summary.clone());
                imported.push(summary);
            }
            Err(err) => log::warn!("skip {}: {err}", ovpn.display()),
        }
    }
    save_profiles_disk(&file).map_err(|e| e.to_string())?;
    crate::tray::refresh_recent_menu(&app);
    Ok(imported)
}

fn walkdir_ovpn(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ovpn"))
            {
                out.push(path);
            }
        }
    }
    walk(root, &mut out);
    out.sort();
    out
}

#[tauri::command]
pub fn delete_profile(
    app: AppHandle,
    id: String,
    session: State<'_, SessionSecrets>,
) -> Result<(), String> {
    let mut file = load_profiles_disk().map_err(|e| e.to_string())?;
    file.profiles.retain(|p| p.id != id);
    save_profiles_disk(&file).map_err(|e| e.to_string())?;
    let dir = profile_dir(&id).map_err(|e| e.to_string())?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    let _ = clear_credentials(&id, &session);
    crate::tray::refresh_recent_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn rename_profile(app: AppHandle, id: String, name: String) -> Result<(), String> {
    let mut file = load_profiles_disk().map_err(|e| e.to_string())?;
    let Some(profile) = file.profiles.iter_mut().find(|p| p.id == id) else {
        return Err("Profile not found".into());
    };
    profile.name = name;
    save_profiles_disk(&file).map_err(|e| e.to_string())?;
    crate::tray::refresh_recent_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn connection_status(tunnel: State<'_, SharedTunnel>) -> Result<ConnStatus, String> {
    Ok(openvpn::current_status(&tunnel))
}

#[tauri::command]
pub fn portal_logs(tunnel: State<'_, SharedTunnel>) -> Result<Vec<String>, String> {
    Ok(openvpn::current_logs(&tunnel))
}

#[tauri::command]
pub async fn disconnect_vpn(app: AppHandle, tunnel: State<'_, SharedTunnel>) -> Result<(), String> {
    let state = tunnel.inner().clone();
    tauri::async_runtime::spawn_blocking(move || disconnect_tunnel(&state, &app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn respond_auth_challenge(
    tunnel: State<'_, SharedTunnel>,
    req: AuthResponseRequest,
) -> Result<(), String> {
    let port = {
        let status = openvpn::current_status(&tunnel);
        status
            .management_port
            .ok_or_else(|| "No active management port".to_string())?
    };
    send_management_auth(port, &req.password_type, &req.password)
}

#[tauri::command]
pub fn connect_vpn(
    app: AppHandle,
    tunnel: State<'_, SharedTunnel>,
    session: State<'_, SessionSecrets>,
    req: ConnectRequest,
) -> Result<(), String> {
    let settings = load_settings_disk().map_err(|e| e.to_string())?;
    let mut file = load_profiles_disk().map_err(|e| e.to_string())?;
    let profile = file
        .profiles
        .iter_mut()
        .find(|p| p.id == req.profile_id)
        .ok_or_else(|| "Profile not found".to_string())?;

    let cfg = profile_dir(&req.profile_id)
        .map_err(|e| e.to_string())?
        .join("config.ovpn");
    if !cfg.is_file() {
        return Err("Profile config missing".into());
    }

    let mut username = req.username.clone().unwrap_or_default();
    let mut password = req.password.clone().unwrap_or_default();
    if (username.is_empty() || password.is_empty()) && profile.needs_user_pass {
        if let Some((u, p)) = load_credentials(&req.profile_id, &session)? {
            if username.is_empty() {
                username = u;
            }
            if password.is_empty() {
                password = p;
            }
        }
    }

    let auth_path = if profile.needs_user_pass {
        if username.is_empty() || password.is_empty() {
            return Err("Username and password are required for this profile".into());
        }
        let store = req
            .store
            .clone()
            .unwrap_or_else(|| settings.credential_store.clone());
        if req.remember {
            store_credentials(&req.profile_id, &username, &password, &store, &session)?;
        }
        let runtime = openvpn::ensure_runtime_dir().map_err(|e| e.to_string())?;
        let auth = runtime.join(format!("{}.auth", req.profile_id));
        fs::write(&auth, format!("{username}\n{password}\n")).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&auth, fs::Permissions::from_mode(0o600));
        }
        Some(auth)
    } else {
        None
    };

    if let Some(pass) = &req.key_passphrase {
        if !pass.is_empty() {
            let mut map = session.lock().map_err(|e| e.to_string())?;
            map.insert(
                format!("{}::key", req.profile_id),
                (String::new(), pass.clone()),
            );
        }
    }

    profile.last_used_ms = Some(now_ms());
    let profile_id = profile.id.clone();
    save_profiles_disk(&file).map_err(|e| e.to_string())?;

    // Returns after spawning a worker; pkexec / kill-switch never block the UI thread.
    connect_tunnel(
        tunnel.inner().clone(),
        app.clone(),
        ConnectArgs {
            profile_id,
            config_path: cfg,
            auth_user_pass_path: auth_path,
            kill_switch: settings.kill_switch,
            elevate: settings.elevate_on_connect,
        },
    )?;
    crate::tray::refresh_recent_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn clear_profile_credentials(
    id: String,
    session: State<'_, SessionSecrets>,
) -> Result<(), String> {
    clear_credentials(&id, &session)
}

#[tauri::command]
pub fn has_stored_credentials(
    id: String,
    session: State<'_, SessionSecrets>,
) -> Result<bool, String> {
    Ok(load_credentials(&id, &session)?.is_some())
}

/// Recent profiles for the tray Connect submenu (most recently used first).
pub fn recent_profiles(limit: usize) -> Vec<ProfileSummary> {
    let mut profiles = load_profiles_disk().map(|f| f.profiles).unwrap_or_default();
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

pub fn tray_disconnect(app: &AppHandle) -> Result<(), String> {
    let tunnel = app.state::<SharedTunnel>().inner().clone();
    disconnect_tunnel(&tunnel, app)
}

pub fn tray_connect(app: &AppHandle, profile_id: &str) -> Result<(), String> {
    let tunnel = app.state::<SharedTunnel>();
    let session = app.state::<SessionSecrets>();
    let req = ConnectRequest {
        profile_id: profile_id.to_string(),
        username: None,
        password: None,
        key_passphrase: None,
        remember: false,
        store: None,
    };
    connect_vpn(app.clone(), tunnel, session, req)?;
    crate::tray::refresh_recent_menu(app);
    Ok(())
}

pub fn tray_reconnect(app: &AppHandle) -> Result<(), String> {
    let tunnel = app.state::<SharedTunnel>().inner().clone();
    let status = openvpn::current_status(&tunnel);
    let profile_id = status
        .profile_id
        .clone()
        .ok_or_else(|| "No active portal to reconnect".to_string())?;
    disconnect_tunnel(&tunnel, app)?;
    tray_connect(app, &profile_id)
}
