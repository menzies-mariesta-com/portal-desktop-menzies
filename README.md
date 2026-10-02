# Portal desktop Menzies

Linux-first OpenVPN client for Menzies OS. Import `.ovpn` files or folders, connect with a simple Wash UI, and keep OpenVPN bundled inside the app.

## Requirements

- Node 22+
- Rust toolchain (Tauri 2)
- Linux: polkit (`pkexec`) for elevated connect
- Bundled OpenVPN under `src-tauri/resources/openvpn/` (Linux x86_64 copy is included when built on a machine that has OpenVPN; see `scripts/fetch-openvpn.sh`)

The app does **not** install packages on the user system and does **not** call a system OpenVPN from PATH at runtime.

## Develop

```bash
npm install
npm run tauri dev
```

Dev server: **http://localhost:2005**

Optional: `PORTAL_OPENVPN_BIN=/path/to/openvpn` overrides the bundled binary for local debugging only.

## Features (v0.1)

- Import single `.ovpn` or a folder of profiles (sidecars copied into app data)
- One active tunnel
- Username/password, key passphrase, management auth challenge
- Credentials in OS keyring or session memory
- Optional auto-connect, reconnect preference, Linux kill switch (nftables via pkexec)
- Connection status, duration, logs
- Always-on system tray: close hides the window; Quit disconnects then exits
- Best-effort Windows/macOS elevation (see ARCHITECTURE)

## Icon

Lucide ShieldKeyhole + water-block. Export: `npm run icon:export`

## License

GPLv3
