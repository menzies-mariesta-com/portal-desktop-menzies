# Threat model

## Assets

- Imported private keys, certificates, TLS keys (OpenVPN sidecars)
- GlobalProtect client certificate and key path references
- VPN usernames and passwords (keyring or session; GP keys namespaced `gp::`)
- Portal SSO cookies (optional cookie cache files)
- Runtime auth files under data/runtime
- Privileged OpenVPN / gpclient processes and shared nftables kill switch

## Trust boundaries

| Boundary          | Notes                                                    |
| ----------------- | -------------------------------------------------------- |
| Webview           | UI only; no direct FS secrets                            |
| Rust / Tauri      | Profile store, spawn gpclient/gpauth/openvpn, keyring    |
| Polkit / OS admin | TUN device, kill switch                                  |
| System browser    | SSO redirects for GlobalProtect; open only for auth URLs |
| Network           | OpenVPN peer / GlobalProtect portal and gateway          |

## Capabilities

Dialog (import / path pick), process exit/restart, updater, window chrome, system tray. No shell allowlist for arbitrary commands. Connect uses fixed bundled OpenVPN and gpclient/gpauth paths plus optional pkexec. Env overrides (`PORTAL_OPENVPN_BIN`, `PORTAL_GPCLIENT_BIN`, `PORTAL_GPAUTH_BIN`) are for development only.

Window close hides to tray; tray Quit tears down both Portal-owned tunnels then exits. Startup does not scan for foreign VPN processes.

## Dual tunnel risks

- Kill switch allowlist must cover both tun interfaces; misconfiguration can blackhole traffic
- Serial connect queue reduces elevate races but long polkit prompts delay the second protocol
- Cookie cache files on disk are sensitive; protect with user permissions

## Update channel

Tauri updater from GitHub releases; pubkey in `tauri.conf.json`.

## Residual risks

- Polkit policy for packaged helper path should be installed with deb for smoother UX
- Kill switch can disrupt connectivity if not cleared on crash (cleared on disconnect; dual unregister)
- macOS / Windows GlobalProtect binaries are best-effort placeholders until release assets ship
- Embedded webview SSO depends on gpauth WebKit availability on the host
- Ignoring TLS errors (optional setting) weakens portal authenticity; default off
