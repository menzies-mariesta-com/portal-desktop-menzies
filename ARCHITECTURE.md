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
  - `profiles.json` index (OpenVPN)
  - `profiles/{id}/config.ovpn` + sidecars
  - `gp-profiles.json` index (GlobalProtect)
  - `gp-profiles/{id}/meta.json` + categorical path references
  - `runtime/*.auth` (0600)
  - `logs/`

## Settings

Nested protocol settings with `activeUiProtocol` (`openvpn` | `globalprotect`). Legacy flat settings migrate into `openvpn` on load.

## Dual tunnels

OpenVPN and GlobalProtect each have an independent tunnel slot. Both may be connected at once. Connect/disconnect jobs share a serial queue so elevate and kill-switch mutations do not race. Order follows activation order.

## IPC map

| Command                                         | Role                                    |
| ----------------------------------------------- | --------------------------------------- |
| import_ovpn_file / import_ovpn_folder           | Copy and index OpenVPN profiles         |
| list_profiles / delete_profile / rename_profile | OpenVPN profile CRUD                    |
| connect_vpn / disconnect_vpn                    | OpenVPN tunnel lifecycle                |
| connection_status / portal_logs                 | OpenVPN UI state                        |
| respond_auth_challenge                          | OpenVPN MFA / management password       |
| list/save/delete/rename_gp_profile              | GlobalProtect profile CRUD              |
| connect_gp / disconnect_gp                      | GlobalProtect via bundled gpclient      |
| gp_connection_status / gp_logs                  | GlobalProtect UI state                  |
| pick_gp_path                                    | Categorical cert/key/HIP/cookie pickers |
| respond_gp_auth                                 | GlobalProtect MFA prompt store          |
| load/save_portal_settings                       | Nested preferences                      |
| clear/has credentials (OVPN + GP)               | Keyring + session                       |

Events: `portal://status`, `portal://gp-status`, `portal://log`, `portal://gp-log`, `portal://auth-challenge`, `portal://gp-auth-challenge`, `portal://traffic`, tray UI events.

Connection log: ring buffer of the latest **20** blocks per protocol.

## OpenVPN

Source tree: `src-tauri/resources/openvpn/{platform}/openvpn`.

`PORTAL_OPENVPN_BIN` overrides for development. Connect on Linux uses `pkexec` when elevate is enabled. Management interface on localhost.

## GlobalProtect

Source tree: `src-tauri/resources/gpclient/{platform}/gpclient` and `gpauth`.

`PORTAL_GPCLIENT_BIN` / `PORTAL_GPAUTH_BIN` override for development. `scripts/fetch-gpclient.sh` copies PATH binaries or builds from `PORTAL_GPOC_SRC`. Auth modes: password, system browser SSO, embedded webview (`gpauth`), client cert, mixed. Cookie cache, HIP, gateway selection, and advanced OpenConnect flags map to `gpclient connect` args.

## Kill switch

Linux nftables table `portal_ks` via pkexec. Shared across protocols: allows loopback, established, ICMP, and VPN ifnames (`tun*`, `tap*`, `gpd*`, plus registered slots). Unregister per protocol on disconnect; clear when none remain.

## Privilege trust boundary

Webview is untrusted for secrets display only. Rust holds credentials transiently, keyring for persistence. Helper elevation is polkit (Linux), UAC/best-effort (Windows), admin/best-effort (macOS). Production never falls back to system OpenVPN or gpclient on PATH (env overrides are for development).

## Window and tray lifecycle

- Closing the main window **hides** it. Tray stays alive.
- Titlebar: dual status icons (OpenVPN + GlobalProtect); clickable when that protocol is active; focuses mode and profile.
- Tray Connect submenu lists recent OVPN and GP profiles. Disconnect/Reconnect/Quit operate on both slots as needed.
- Status badge merges phases (connected preferred).

## Bundle resources

`tauri.conf.json` maps `resources/openvpn/` and `resources/gpclient/` as directories (not flattened globs).
