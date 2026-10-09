//! Portal desktop Menzies: Tauri entry, OpenVPN + GlobalProtect, dual tunnels.
//!
//! Lifecycle: the main window close button hides to the system tray. Tray Quit
//! tears down in-app tunnels then exits. The app does not scan the OS for
//! already-running VPN processes on startup.

mod connect_queue;
mod globalprotect;
mod kill_switch;
mod openvpn;
mod paths;
mod portal;
mod tray;

use connect_queue::new_connect_queue;
use globalprotect::new_shared_gp_tunnel;
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
        .manage(new_shared_gp_tunnel())
        .manage(new_connect_queue())
        .manage(Mutex::new(HashMap::<String, (String, String)>::new()))
        .setup(|app| {
            if let Err(err) = paths::ensure_app_dirs() {
                log::warn!("could not create Menzies app dirs: {err}");
            }

            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }

            #[cfg(all(desktop, not(target_os = "android"), not(target_os = "ios")))]
            {
                match tray::setup_tray(app.handle()) {
                    Ok(icon) => {
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
            portal::connect_gp,
            globalprotect::list_gp_profiles,
            globalprotect::save_gp_profile,
            globalprotect::delete_gp_profile,
            globalprotect::rename_gp_profile,
            globalprotect::gp_connection_status,
            globalprotect::gp_logs,
            globalprotect::disconnect_gp_cmd,
            globalprotect::respond_gp_auth,
            globalprotect::pick_gp_path,
            globalprotect::has_gp_stored_credentials,
            globalprotect::clear_gp_credentials_cmd,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            eprintln!("error while running tauri application: {err}");
            std::process::exit(1);
        });
}
