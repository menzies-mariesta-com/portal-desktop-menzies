# Threat model

## Assets

- Imported private keys, certificates, TLS keys
- VPN usernames and passwords (keyring or session)
- Runtime auth files under data/runtime
- Privileged OpenVPN process and optional nftables kill switch

## Trust boundaries

| Boundary          | Notes                                                             |
| ----------------- | ----------------------------------------------------------------- |
| Webview           | UI only; no direct FS secrets                                     |
| Rust / Tauri      | Profile store, spawn, keyring                                     |
| Polkit / OS admin | TUN device, kill switch                                           |
| Network           | OpenVPN peer; optional public IP probe is opt-in and not required |

## Capabilities

Dialog (import), process exit/restart, updater, window chrome. No shell allowlist for arbitrary commands. Connect uses fixed bundled OpenVPN path plus pkexec.

## Update channel

Tauri updater from GitHub releases; pubkey in `tauri.conf.json`.

## Residual risks

- Polkit policy for packaged helper path should be installed with deb for smoother UX
- Kill switch can disrupt connectivity if not cleared on crash (cleared on disconnect and best-effort on next launch recommend)
- macOS Network Extension not shipped in v0.1; elevation may fail on locked-down Macs
