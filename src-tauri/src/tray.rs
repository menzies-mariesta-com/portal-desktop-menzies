//! System tray: always-on icon, status-driven connect actions, read-only VPN IP.
//!
//! Intentionally does **not** scan the OS for foreign OpenVPN processes on startup.
//!
//! ## Status mapping (Connect / Disconnect / Reconnect)
//!
//! | Phase | Connect | Disconnect | Reconnect |
//! |-------|---------|------------|-----------|
//! | Idle / Error | enabled | disabled | disabled |
//! | Connecting / Reconnecting | disabled | enabled | disabled |
//! | Connected | disabled | enabled | enabled |
//!
//! ## Status color (tray icon)
//!
//! The tray uses the product PNG with a corner status badge (Linux StatusNotifier
//! and Windows show full-color icons; macOS template mode is not used so the badge
//! stays visible). Menu item icons use the same status color dots where the DE
//! paints custom menu icons (support varies by Linux shell).

use crate::openvpn::{self, ConnPhase, ConnStatus, SharedTunnel};
use crate::portal;
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{
    IconMenuItem, IconMenuItemBuilder, Menu, MenuBuilder, MenuItem, MenuItemBuilder,
    PredefinedMenuItem, Submenu, SubmenuBuilder,
};
use tauri::tray::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const RECENT_LIMIT: usize = 8;
const CONNECT_PREFIX: &str = "connect:";

/// Wash-aligned semantic colors for tray status (RGB).
const COLOR_IDLE: (u8, u8, u8) = (0x64, 0x74, 0x8B);
const COLOR_CONNECTING: (u8, u8, u8) = (0xB8, 0x75, 0x24);
const COLOR_CONNECTED: (u8, u8, u8) = (0x2F, 0x6F, 0x4E);
const COLOR_ERROR: (u8, u8, u8) = (0xA3, 0x3A, 0x32);

struct TrayMenuParts {
    tray: TrayIcon,
    connect_submenu: Submenu<tauri::Wry>,
    disconnect: IconMenuItem<tauri::Wry>,
    reconnect: IconMenuItem<tauri::Wry>,
    show_stats: MenuItem<tauri::Wry>,
    ip_label: MenuItem<tauri::Wry>,
    base_icon: Image<'static>,
}

type TrayMenuState = Mutex<Option<TrayMenuParts>>;

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.unminimize();
    }
}

fn load_base_icon(app: &AppHandle) -> Option<Image<'static>> {
    if let Some(icon) = app.default_window_icon() {
        return Some(icon.clone().to_owned());
    }
    Image::from_bytes(include_bytes!("../icons/32x32.png")).ok()
}

fn phase_color(phase: &ConnPhase) -> (u8, u8, u8) {
    match phase {
        ConnPhase::Connected => COLOR_CONNECTED,
        ConnPhase::Connecting | ConnPhase::Reconnecting => COLOR_CONNECTING,
        ConnPhase::Error => COLOR_ERROR,
        ConnPhase::Idle => COLOR_IDLE,
    }
}

fn action_flags(phase: &ConnPhase) -> (bool, bool, bool) {
    // (connect_enabled, disconnect_enabled, reconnect_enabled)
    match phase {
        ConnPhase::Connected => (false, true, true),
        ConnPhase::Connecting | ConnPhase::Reconnecting => (false, true, false),
        ConnPhase::Idle | ConnPhase::Error => (true, false, false),
    }
}

fn vpn_ip_label(status: &ConnStatus) -> String {
    match status.vpn_ip.as_deref() {
        Some(ip) if !ip.is_empty() => format!("VPN IP: {ip}"),
        _ => "VPN IP: n/a".to_string(),
    }
}

fn status_dot(rgb: (u8, u8, u8), size: u32) -> Image<'static> {
    let (r, g, b) = rgb;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let cx = (size as f32 - 1.0) / 2.0;
    let cy = cx;
    let radius = size as f32 * 0.38;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let i = ((y * size + x) * 4) as usize;
            if dist <= radius {
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            }
        }
    }
    Image::new_owned(rgba, size, size)
}

fn icon_with_status_badge(base: &Image<'static>, rgb: (u8, u8, u8)) -> Image<'static> {
    let w = base.width();
    let h = base.height();
    let mut rgba = base.rgba().to_vec();
    let (cr, cg, cb) = rgb;
    let badge_r = (w.min(h) as f32 * 0.18).max(3.0);
    let cx = w as f32 - badge_r - 1.0;
    let cy = h as f32 - badge_r - 1.0;
    let ring = 1.5_f32;

    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let i = ((y * w + x) * 4) as usize;
            if dist <= badge_r - ring {
                rgba[i] = cr;
                rgba[i + 1] = cg;
                rgba[i + 2] = cb;
                rgba[i + 3] = 255;
            } else if dist <= badge_r {
                // Light paper ring so the badge reads on busy water-block art.
                rgba[i] = 0xF7;
                rgba[i + 1] = 0xF4;
                rgba[i + 2] = 0xEF;
                rgba[i + 3] = 255;
            }
        }
    }
    Image::new_owned(rgba, w, h)
}

fn build_connect_submenu(
    app: &AppHandle,
    connect_enabled: bool,
) -> tauri::Result<Submenu<tauri::Wry>> {
    let mut builder = SubmenuBuilder::with_id(app, "connect", "Connect").enabled(connect_enabled);
    let recent = portal::recent_profiles(RECENT_LIMIT);
    if recent.is_empty() {
        let empty = MenuItemBuilder::with_id("connect:none", "No recent portals")
            .enabled(false)
            .build(app)?;
        builder = builder.item(&empty);
    } else {
        for profile in recent {
            let id = format!("{CONNECT_PREFIX}{}", profile.id);
            let item = MenuItemBuilder::with_id(id, &profile.name)
                .enabled(connect_enabled)
                .build(app)?;
            builder = builder.item(&item);
        }
    }
    builder.build()
}

fn refresh_connect_submenu(app: &AppHandle, connect_enabled: bool) {
    let tray_state = app.state::<TrayMenuState>();
    let Ok(state) = tray_state.lock() else {
        return;
    };
    let Some(parts) = state.as_ref() else {
        return;
    };
    let submenu = &parts.connect_submenu;
    let _ = submenu.set_enabled(connect_enabled);
    if let Ok(existing) = submenu.items() {
        for item in existing {
            let _ = submenu.remove(&item);
        }
    }
    let recent = portal::recent_profiles(RECENT_LIMIT);
    if recent.is_empty() {
        if let Ok(empty) = MenuItemBuilder::with_id("connect:none", "No recent portals")
            .enabled(false)
            .build(app)
        {
            let _ = submenu.append(&empty);
        }
        return;
    }
    for profile in recent {
        let id = format!("{CONNECT_PREFIX}{}", profile.id);
        if let Ok(item) = MenuItemBuilder::with_id(id, &profile.name)
            .enabled(connect_enabled)
            .build(app)
        {
            let _ = submenu.append(&item);
        }
    }
}

fn apply_status_to_parts(parts: &TrayMenuParts, status: &ConnStatus) {
    let (connect_on, disconnect_on, reconnect_on) = action_flags(&status.phase);
    let color = phase_color(&status.phase);
    let stats_on = matches!(
        status.phase,
        ConnPhase::Connected | ConnPhase::Connecting | ConnPhase::Reconnecting
    );
    let _ = parts.connect_submenu.set_enabled(connect_on);
    let _ = parts.disconnect.set_enabled(disconnect_on);
    let _ = parts.reconnect.set_enabled(reconnect_on);
    let _ = parts.show_stats.set_enabled(stats_on);
    let _ = parts.ip_label.set_text(vpn_ip_label(status));
    let _ = parts.disconnect.set_icon(Some(status_dot(color, 16)));
    let _ = parts.reconnect.set_icon(Some(status_dot(color, 16)));
    let badged = icon_with_status_badge(&parts.base_icon, color);
    let _ = parts.tray.set_icon(Some(badged));
    let tip = match status.phase {
        ConnPhase::Connected => match status.vpn_ip.as_deref() {
            Some(ip) if !ip.is_empty() => format!("Portal: connected ({ip})"),
            _ => "Portal: connected".to_string(),
        },
        ConnPhase::Connecting => "Portal: connecting".to_string(),
        ConnPhase::Reconnecting => "Portal: reconnecting".to_string(),
        ConnPhase::Error => "Portal: error".to_string(),
        ConnPhase::Idle => "Portal: disconnected".to_string(),
    };
    let _ = parts.tray.set_tooltip(Some(tip));
}

fn build_tray_menu(
    app: &AppHandle,
    status: &ConnStatus,
) -> tauri::Result<(
    Menu<tauri::Wry>,
    Submenu<tauri::Wry>,
    IconMenuItem<tauri::Wry>,
    IconMenuItem<tauri::Wry>,
    MenuItem<tauri::Wry>,
    MenuItem<tauri::Wry>,
)> {
    let (connect_on, disconnect_on, reconnect_on) = action_flags(&status.phase);
    let color = phase_color(&status.phase);
    let connect = build_connect_submenu(app, connect_on)?;
    let disconnect = IconMenuItemBuilder::with_id("disconnect", "Disconnect")
        .enabled(disconnect_on)
        .icon(status_dot(color, 16))
        .build(app)?;
    let reconnect = IconMenuItemBuilder::with_id("reconnect", "Reconnect")
        .enabled(reconnect_on)
        .icon(status_dot(color, 16))
        .build(app)?;
    let show_stats = MenuItemBuilder::with_id("show_stats", "Show stats")
        .enabled(matches!(
            status.phase,
            ConnPhase::Connected | ConnPhase::Connecting | ConnPhase::Reconnecting
        ))
        .build(app)?;
    let ip_label = MenuItemBuilder::with_id("vpn_ip", vpn_ip_label(status))
        .enabled(false)
        .build(app)?;
    let show = MenuItemBuilder::with_id("show", "Show Portal").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let menu = MenuBuilder::new(app)
        .item(&connect)
        .item(&disconnect)
        .item(&reconnect)
        .item(&sep1)
        .item(&show_stats)
        .item(&ip_label)
        .item(&show)
        .item(&sep2)
        .item(&quit)
        .build()?;

    Ok((menu, connect, disconnect, reconnect, show_stats, ip_label))
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "show" => show_main(app),
        "disconnect" => {
            if let Err(err) = portal::tray_disconnect(app) {
                log::warn!("tray disconnect failed: {err}");
                let _ = app.emit("portal://tray-toast", format!("Disconnect failed: {err}"));
            }
            refresh_for_current_status(app);
        }
        "reconnect" => {
            if let Err(err) = portal::tray_reconnect(app) {
                log::warn!("tray reconnect failed: {err}");
                show_main(app);
                let _ = app.emit("portal://tray-toast", err);
            } else {
                refresh_for_current_status(app);
            }
        }
        "show_stats" => {
            show_main(app);
            let _ = app.emit("portal://tray-show-stats", ());
        }
        "vpn_ip" => {
            // Read-only label; clicks should not fire when disabled.
        }
        "quit" => {
            let _ = portal::tray_disconnect(app);
            app.exit(0);
        }
        other if other.starts_with(CONNECT_PREFIX) => {
            let profile_id = &other[CONNECT_PREFIX.len()..];
            if profile_id.is_empty() || profile_id == "none" {
                return;
            }
            match portal::tray_connect(app, profile_id) {
                Ok(()) => refresh_for_current_status(app),
                Err(err) => {
                    log::warn!("tray connect failed: {err}");
                    show_main(app);
                    if err.contains("Username and password")
                        || err.contains("passphrase")
                        || err.contains("required")
                    {
                        let _ = app.emit("portal://tray-needs-auth", profile_id.to_string());
                    } else {
                        let _ = app.emit("portal://tray-toast", err);
                    }
                    refresh_for_current_status(app);
                }
            }
        }
        _ => {}
    }
}

/// Create the always-on tray. Keep the returned [`TrayIcon`] managed
/// so the icon is not destroyed when setup returns.
pub fn setup_tray(app: &AppHandle) -> tauri::Result<TrayIcon> {
    app.manage(TrayMenuState::new(None));

    let status = openvpn::current_status(&app.state::<SharedTunnel>());
    let Some(base_icon) = load_base_icon(app) else {
        log::warn!("portal tray: no icon available; tray may be invisible on Linux");
        // Still build a minimal tray without a custom image.
        let (menu, connect_submenu, disconnect, reconnect, show_stats, ip_label) =
            build_tray_menu(app, &status)?;
        let tray = TrayIconBuilder::with_id("portal-main")
            .menu(&menu)
            .tooltip("Portal")
            .show_menu_on_left_click(true)
            .on_menu_event(|app, event| {
                handle_menu_event(app, event.id.as_ref());
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } = event
                {
                    show_main(tray.app_handle());
                }
            })
            .build(app)?;
        // Placeholder 1x1 so TrayMenuParts can store a base; badge updates no-op visually.
        let base_icon = Image::new_owned(vec![0, 0, 0, 0], 1, 1);
        if let Ok(mut guard) = app.state::<TrayMenuState>().lock() {
            *guard = Some(TrayMenuParts {
                tray: tray.clone(),
                connect_submenu,
                disconnect,
                reconnect,
                show_stats,
                ip_label,
                base_icon,
            });
        }
        apply_status_ui(app, &status);
        return Ok(tray);
    };

    let (menu, connect_submenu, disconnect, reconnect, show_stats, ip_label) =
        build_tray_menu(app, &status)?;
    let initial_icon = icon_with_status_badge(&base_icon, phase_color(&status.phase));

    let tray = TrayIconBuilder::with_id("portal-main")
        .menu(&menu)
        .icon(initial_icon)
        .tooltip("Portal")
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id.as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    if let Ok(mut guard) = app.state::<TrayMenuState>().lock() {
        *guard = Some(TrayMenuParts {
            tray: tray.clone(),
            connect_submenu,
            disconnect,
            reconnect,
            show_stats,
            ip_label,
            base_icon,
        });
    }
    apply_status_ui(app, &status);

    Ok(tray)
}

fn apply_status_ui(app: &AppHandle, status: &ConnStatus) {
    let (connect_on, _, _) = action_flags(&status.phase);
    refresh_connect_submenu(app, connect_on);
    let tray_state = app.state::<TrayMenuState>();
    let Ok(state) = tray_state.lock() else {
        return;
    };
    let Some(parts) = state.as_ref() else {
        return;
    };
    apply_status_to_parts(parts, status);
}

/// Refresh tray enable/disable, IP label, and status badge from live tunnel state.
pub fn refresh_for_current_status(app: &AppHandle) {
    let status = openvpn::current_status(&app.state::<SharedTunnel>());
    apply_status_ui(app, &status);
}

/// Called whenever connection status changes (from OpenVPN manager).
pub fn on_connection_status(app: &AppHandle, status: &ConnStatus) {
    apply_status_ui(app, status);
}

/// Refresh the Connect submenu after profile import/delete/connect.
pub fn refresh_recent_menu(app: &AppHandle) {
    refresh_for_current_status(app);
}
