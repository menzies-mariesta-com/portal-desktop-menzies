# Architecture

## Identity

| Item     | Value                                       |
| -------- | ------------------------------------------- |
| App      | portal-desktop-menzies                      |
| Bundle   | com.mariesta.menzies.portal-desktop-menzies |
| Dev port | 2005                                        |

## Data dirs

- Config: `{config}/menzies/com.mariesta.menzies.portal-desktop-menzies/` (`settings.json`)
- Data: `{data}/menzies/com.mariesta.menzies.portal-desktop-menzies/`
  - `profiles.json` index
  - `profiles/{id}/config.ovpn` + sidecars
  - `runtime/*.auth` (0600)
  - `logs/`

## IPC map

| Command                                         | Role                      |
| ----------------------------------------------- | ------------------------- |
| import_ovpn_file / import_ovpn_folder           | Copy and index profiles   |
| list_profiles / delete_profile / rename_profile | Profile CRUD              |
| connect_vpn / disconnect_vpn                    | Tunnel lifecycle          |
| connection_status / portal_logs                 | UI state                  |
| respond_auth_challenge                          | MFA / management password |
| load/save_portal_settings                       | Preferences               |
| clear/has credentials                           | Keyring + session         |

Events: `portal://status`, `portal://log`, `portal://auth-challenge`, `portal://traffic`, tray UI events.

Connection log: Rust `TunnelState.log_lines` is a ring buffer of the latest **20** blocks (mockup-code lines). The FE also caps at 20 when appending or reloading so an open log tab cannot grow without bound.

## OpenVPN

Source tree: `src-tauri/resources/openvpn/{platform}/openvpn` (e.g. `linux-x86_64`).

### Bundle layout (Linux deb/rpm)

`tauri.conf.json` maps directories (not map-globs), so arch subdirs are preserved:

| Source                   | Installed path                                         |
| ------------------------ | ------------------------------------------------------ |
| `resources/openvpn/`     | `$RESOURCE/openvpn/` (e.g. `/usr/lib/Portal/openvpn/`) |
| `…/linux-x86_64/openvpn` | `$RESOURCE/openvpn/linux-x86_64/openvpn`               |

`$RESOURCE` is Tauri `resource_dir()` (productName folder under `/usr/lib` for deb). Runtime resolution in `openvpn.rs` checks `resource_dir` then `openvpn/` then a flat `openvpn/openvpn` fallback for older flattened packages. `PORTAL_OPENVPN_BIN` overrides for development only.

Do not use map form `"resources/openvpn/**/*": "openvpn/"`: Tauri flattens map-globs and drops `linux-x86_64/`.

Connect on Linux uses `pkexec` when elevate is enabled. Management interface on localhost for hold release, state, and challenges.

## Privilege trust boundary

Webview is untrusted for secrets display only. Rust holds credentials transiently, keyring for persistence. Helper elevation is polkit (Linux), UAC/best-effort (Windows), admin/best-effort (macOS). Never falls back to system OpenVPN on PATH.

## One tunnel

Shared `TunnelState` mutex rejects a second connect while connecting or connected.

## Window and tray lifecycle

- Closing the main window **hides** it. The process stays alive with an always-on system tray icon.
- Tray menu: Connect (recent portals submenu), Disconnect, Reconnect, Show stats, read-only VPN IP label, Show Portal, Quit.
- Connect / Disconnect / Reconnect are status-driven (disabled, not hidden):
  - Idle or Error: Connect enabled; Disconnect and Reconnect disabled
  - Connecting or Reconnecting: Disconnect enabled; Connect and Reconnect disabled
  - Connected: Disconnect and Reconnect enabled; Connect disabled
- VPN IP is a disabled menu label (`VPN IP: {addr}` or `VPN IP: n/a`), not an action.
- Tray icon keeps the product mark and paints a corner status badge (idle slate, connecting ochre, connected green, error rose). Linux and Windows show full-color tray icons; menu item status dots depend on the desktop shell. macOS template monochrome is not used so the badge stays visible.
- Status changes rebuild enable/disable, IP text, tooltip, and badge via `tray::on_connection_status`.
- Tray Quit calls `disconnect_tunnel` for the in-app session, then `app.exit`.
- Startup does **not** scan the OS for foreign OpenVPN processes. Only Portal-spawned tunnels are tracked.
- Tray icon is kept via `app.manage(tray)` so it is not dropped at end of setup. A PNG icon is required for reliable visibility on Linux StatusNotifier.
