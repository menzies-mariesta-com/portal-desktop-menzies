//! Portal desktop Menzies: Tauri entry, OpenVPN profiles, and connection manager.

mod openvpn;
mod paths;
mod portal;

use openvpn::new_shared_tunnel;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

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

            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }

            #[cfg(all(desktop, not(target_os = "android"), not(target_os = "ios")))]
            {
                use tauri::menu::{MenuBuilder, MenuItemBuilder};
                use tauri::tray::TrayIconBuilder;
                let settings = portal::load_portal_settings().unwrap_or_default();
                if settings.tray_enabled {
                    let show = MenuItemBuilder::with_id("show", "Show Portal").build(app)?;
                    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
                    let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;
                    let _tray = TrayIconBuilder::new()
                        .menu(&menu)
                        .tooltip("Portal")
                        .on_menu_event(|app, event| match event.id.as_ref() {
                            "show" => {
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                            "quit" => {
                                app.exit(0);
                            }
                            _ => {}
                        })
                        .build(app)?;
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
