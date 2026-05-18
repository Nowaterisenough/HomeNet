use crate::config::add_log;

#[cfg(target_os = "windows")]
const SYSTEM_FORWARDING_NOTE: &str =
    "Windows 内核 NAT 和透明源地址透传尚未接入，当前不会写入 netsh、WinNAT 或防火墙规则";

#[cfg(target_os = "macos")]
const SYSTEM_FORWARDING_NOTE: &str =
    "macOS 内核 NAT 和透明源地址透传尚未接入，当前不会写入 pfctl 规则";

#[cfg(target_os = "linux")]
const SYSTEM_FORWARDING_NOTE: &str =
    "Linux 内核 NAT 和透明源地址透传尚未接入，当前不会写入 nftables、iptables 或 TPROXY 规则";

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
const SYSTEM_FORWARDING_NOTE: &str =
    "当前平台的内核 NAT 和透明源地址透传尚未接入系统规则";

pub fn capabilities_summary() -> &'static str {
    SYSTEM_FORWARDING_NOTE
}

pub fn log_capabilities() {
    add_log(
        "info",
        "转发",
        &format!(
            "普通 TCP/UDP 转发已启用；{}",
            capabilities_summary()
        ),
    );
}
