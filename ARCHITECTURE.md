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

Events: `portal://status`, `portal://log`, `portal://auth-challenge`.

## OpenVPN

Bundled binary from `src-tauri/resources/openvpn/{platform}/`. Connect on Linux uses `pkexec` when elevate is enabled. Management interface on localhost for hold release, state, and challenges.

## Privilege trust boundary

Webview is untrusted for secrets display only. Rust holds credentials transiently, keyring for persistence. Helper elevation is polkit (Linux), UAC/best-effort (Windows), admin/best-effort (macOS). Never falls back to system OpenVPN on PATH.

## One tunnel

Shared `TunnelState` mutex rejects a second connect while connecting or connected.
