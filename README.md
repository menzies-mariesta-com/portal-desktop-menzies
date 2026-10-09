# Portal desktop Menzies

Linux-first dual-mode VPN client for Menzies OS: **OpenVPN** and **GlobalProtect** (via bundled `gpclient` / `gpauth`). Both tunnels can run at once.

## Requirements

- Node 22+
- Rust toolchain (Tauri 2)
- Linux: polkit (`pkexec`) for elevated connect
- Bundled OpenVPN under `src-tauri/resources/openvpn/` (see `scripts/fetch-openvpn.sh`)
- Bundled GlobalProtect CLI under `src-tauri/resources/gpclient/` (see `scripts/fetch-gpclient.sh`)

The app does **not** install packages on the user system. Production builds use bundled binaries only. Dev overrides: `PORTAL_OPENVPN_BIN`, `PORTAL_GPCLIENT_BIN`, `PORTAL_GPAUTH_BIN`.

## Develop

```bash
npm install
bash scripts/fetch-openvpn.sh   # if needed
bash scripts/fetch-gpclient.sh  # copies gpclient/gpauth from PATH when present
npm run tauri dev
```

Dev server: **http://localhost:2005**

Optional: set `PORTAL_GPOC_SRC` to a [GlobalProtect-openconnect](https://github.com/yuezk/GlobalProtect-openconnect) checkout before `fetch-gpclient.sh` to build from source.

## Features

- **OpenVPN:** import `.ovpn` file or folder; user/pass, key passphrase, management MFA
- **GlobalProtect:** portal profiles with password, browser SSO, embedded webview SSO, client certs, HIP, gateway modes, cookie cache, advanced OpenConnect flags
- Dual tunnels with serial connect queue; dual titlebar status icons
- Protocol-scoped settings (kill switch, elevate, reconnect, auto-connect, GP cookie/TLS options)
- Credentials in OS keyring or session memory
- Linux kill switch (shared nftables allowlist for tun interfaces)
- Always-on system tray: close hides; Quit disconnects both then exits
- Best-effort Windows/macOS elevation and binary slots (see ARCHITECTURE)

## Icon

Lucide ShieldKeyhole + water-block. Export: `npm run icon:export`

## License

GPLv3
