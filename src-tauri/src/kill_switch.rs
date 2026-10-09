//! Shared Linux nftables kill switch for dual tunnels (OpenVPN + GlobalProtect).

use std::collections::HashSet;
use std::process::Command;
use std::sync::Mutex;

static ACTIVE_IFNAMES: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Register a tunnel interface and (re)apply the drop-all-except-allowlist rules.
pub fn register_interface(ifname: &str) -> Result<(), String> {
    let mut guard = ACTIVE_IFNAMES
        .lock()
        .map_err(|e| format!("kill switch lock: {e}"))?;
    let set = guard.get_or_insert_with(HashSet::new);
    set.insert(ifname.to_string());
    let names: Vec<String> = set.iter().cloned().collect();
    drop(guard);
    apply_rules(&names)
}

/// Unregister a tunnel interface. Clears the table when none remain.
pub fn unregister_interface(ifname: &str) -> Result<(), String> {
    let mut guard = ACTIVE_IFNAMES
        .lock()
        .map_err(|e| format!("kill switch lock: {e}"))?;
    if let Some(set) = guard.as_mut() {
        set.remove(ifname);
        if set.is_empty() {
            *guard = None;
            drop(guard);
            return clear_rules();
        }
        let names: Vec<String> = set.iter().cloned().collect();
        drop(guard);
        return apply_rules(&names);
    }
    drop(guard);
    clear_rules()
}

/// Tear down kill switch entirely (both protocols idle or user disabled).
#[allow(dead_code)]
pub fn clear_all() -> Result<(), String> {
    if let Ok(mut guard) = ACTIVE_IFNAMES.lock() {
        *guard = None;
    }
    clear_rules()
}

#[cfg(target_os = "linux")]
fn apply_rules(ifnames: &[String]) -> Result<(), String> {
    let mut script = String::from(
        r#"nft delete table inet portal_ks 2>/dev/null || true
nft add table inet portal_ks
nft add chain inet portal_ks output '{ type filter hook output priority 0; policy drop; }'
nft add rule inet portal_ks output oiflo "lo" accept
nft add rule inet portal_ks output ct state established,related accept
nft add rule inet portal_ks output meta l4proto icmp accept
"#,
    );
    // Wildcard VPN interfaces (OpenVPN tun/tap, OpenConnect/gpclient).
    script.push_str("nft add rule inet portal_ks output oifname \"tun*\" accept\n");
    script.push_str("nft add rule inet portal_ks output oifname \"tap*\" accept\n");
    script.push_str("nft add rule inet portal_ks output oifname \"gpd*\" accept\n");
    for name in ifnames {
        let safe = name.replace('\'', "").replace('"', "");
        if safe.is_empty() {
            continue;
        }
        script.push_str(&format!(
            "nft add rule inet portal_ks output oifname \"{safe}\" accept\n"
        ));
    }
    let status = Command::new("pkexec")
        .arg("sh")
        .arg("-c")
        .arg(&script)
        .status()
        .map_err(|e| format!("pkexec nft kill switch failed: {e}"))?;
    if !status.success() {
        return Err("Kill switch nftables rules failed (polkit / nft)".into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn clear_rules() -> Result<(), String> {
    let status = Command::new("pkexec")
        .arg("sh")
        .arg("-c")
        .arg("nft delete table inet portal_ks 2>/dev/null || true")
        .status()
        .map_err(|e| format!("pkexec clear kill switch failed: {e}"))?;
    if !status.success() {
        return Err("Could not clear kill switch nftables table".into());
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn apply_rules(_ifnames: &[String]) -> Result<(), String> {
    log::warn!("Kill switch is Linux-only; skipping apply");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn clear_rules() -> Result<(), String> {
    Ok(())
}
