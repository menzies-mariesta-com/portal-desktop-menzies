# Contributing

- Conventional Commits
- `npm run lint` then `npm run check` then `npm run lint` again, then unit tests
- Rust: `cargo fmt --all -- --check` and `cargo check` in `src-tauri`
- Do not commit `.cursor/` or secrets
- Bundle OpenVPN via `scripts/fetch-openvpn.sh` before release packaging
