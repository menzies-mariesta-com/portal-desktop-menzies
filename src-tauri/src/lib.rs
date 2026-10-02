//! Portal desktop Menzies: Tauri entry, OpenVPN profiles, and connection manager.
//!
//! Lifecycle: the main window close button hides to the system tray. Tray Quit
//! tears down the in-app tunnel then exits. The app does not scan the OS for
//! already-running OpenVPN processes on startup.

mod openvpn;
mod paths;
mod portal;
mod tray;

use openvpn::new_shared_tunnel;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(new_shared_tunnel())
        .manage(Mutex::new(HashMap::<String, (String, String)>::new()))
        .setup(|app| {
            if let Err(err) = paths::ensure_app_dirs() {
                log::warn!("could not create Menzies app dirs: {err}");
            }

            // Do not discover or reclaim foreign OpenVPN processes here.
            // Tunnel state starts Idle; only Portal-spawned children are tracked.

            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }

            #[cfg(all(desktop, not(target_os = "android"), not(target_os = "ios")))]
            {
                match tray::setup_tray(app.handle()) {
                    Ok(icon) => {
                        // Keep the tray alive for the process lifetime.
                        app.manage(icon);
                    }
                    Err(err) => {
                        log::error!("failed to create system tray: {err}");
                    }
                }
            }

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Hide to tray instead of destroying the window / quitting.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            paths::app_paths,
            portal::load_portal_settings,
            portal::save_portal_settings,
            portal::list_profiles,
            portal::import_ovpn_file,
            portal::import_ovpn_folder,
            portal::delete_profile,
            portal::rename_profile,
            portal::connection_status,
            portal::portal_logs,
            portal::connect_vpn,
            portal::disconnect_vpn,
            portal::respond_auth_challenge,
            portal::clear_profile_credentials,
            portal::has_stored_credentials,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("error while running tauri application: {err}");
            std::process::exit(1);
        });
}
