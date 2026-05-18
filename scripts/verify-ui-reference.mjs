import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();

const checks = [
  ["src/App.vue", "HomeNet · DDNS 与端口转发"],
  ["src/App.vue", 'invoke<RuntimeStatus>("get_runtime_status")'],
  ["src/App.vue", "setInterval(loadRuntimeStatus"],
  ["src/App.vue", "homenet:focus-logs"],
  ["src/App.vue", 'invoke<NetworkInterfaceInfo[]>("list_network_interfaces")'],
  ["src/App.vue", 'invoke<string>("get_ipv6_interface")'],
  ["src/App.vue", 'invoke("set_ipv6_interface"'],
  ["src/App.vue", "绑定网卡"],
  ["src/App.vue", "自动选择"],
  ["src/types.ts", "export interface NetworkInterfaceInfo"],
  ["src/components/DdnsPanel.vue", "AccessKey ID"],
  ["src/components/DdnsPanel.vue", "立即更新"],
  ["src/components/DdnsPanel.vue", 'invoke<string>("get_ddns_current_record")'],
  ["src/components/DdnsPanel.vue", "暂无成功更新记录"],
  ["src/components/ForwardRulesPanel.vue", "暂无转发规则"],
  ["src/components/ForwardRulesPanel.vue", 'invoke<ForwardRule>("save_forward_rule"'],
  ["src/components/ForwardRulesPanel.vue", "80;443;1000-1003"],
  ["src/components/ForwardRulesPanel.vue", "pairPortExpressions"],
  ["src/utils/ports.ts", "parsePortExpression"],
  ["src/utils/ports.ts", "pairPortExpressions"],
  ["src/components/LogPanel.vue", "最近日志"],
  ["src/components/LogPanel.vue", 'invoke<LogEntry[]>("get_recent_logs")'],
  ["src-tauri/tauri.conf.json", '"productName": "HomeNet"'],
  ["src-tauri/tauri.conf.json", '"title": "HomeNet · DDNS与端口转发"'],
  ["src-tauri/Cargo.toml", 'name = "homenet"'],
  ["src-tauri/Cargo.toml", 'description = "HomeNet · DDNS与端口转发"'],
  ["src-tauri/src/tray.rs", '.tooltip("HomeNet · DDNS与端口转发")'],
  ["src-tauri/src/autostart.rs", 'let value_name = "HomeNet"'],
  ["src-tauri/src/autostart.rs", "Name=HomeNet"],
];

const missing = [];

for (const [file, needle] of checks) {
  const content = readFileSync(join(root, file), "utf8");
  if (!content.includes(needle)) {
    missing.push(`${file}: ${needle}`);
  }
}

const brandFiles = [
  "src/App.vue",
  "src-tauri/tauri.conf.json",
  "src-tauri/Cargo.toml",
  "src-tauri/src/tray.rs",
  "src-tauri/src/autostart.rs",
];

for (const file of brandFiles) {
  const content = readFileSync(join(root, file), "utf8");
  for (const needle of ["网络管家", "homenet ·", "Name=homenet"]) {
    if (content.includes(needle)) {
      missing.push(`${file}: replace '${needle}' with HomeNet`);
    }
  }
}

const cssChecks = [
  [
    "src/App.vue",
    "scale reference canvas to current window",
    (content) =>
      content.includes("--design-width: 1586px;") &&
      content.includes("--design-height: 992px;") &&
      content.includes("window.innerWidth / DESIGN_WIDTH") &&
      content.includes("window.innerHeight / DESIGN_HEIGHT") &&
      content.includes("width: var(--design-width);") &&
      content.includes("height: var(--design-height);") &&
      content.includes("translate(var(--frame-x), var(--frame-y)) scale(var(--ui-scale))") &&
      !content.includes("Math.min(1,"),
  ],
  [
    "src/App.vue",
    "reference titlebar height",
    (content) => content.includes("--titlebar-height: 52px;"),
  ],
  [
    "src/App.vue",
    "sidebar removed from main layout",
    (content) =>
      !content.includes('from "./components/Sidebar.vue"') &&
      !content.includes("<Sidebar") &&
      !content.includes("margin-left: var(--sidebar-width)"),
  ],
  [
    "src/App.vue",
    "reference middle panel columns",
    (content) =>
      content.includes("grid-template-columns: 430px minmax(0, 1fr);"),
  ],
  [
    "src/App.vue",
    "reference vertical rhythm",
    (content) =>
      content.includes("grid-template-rows: 118px minmax(0, 1fr) 234px;"),
  ],
  [
    "src/App.vue",
    "titlebar menu button removed",
    (content) =>
      content.includes('from "@lucide/vue"') &&
      !content.includes("<Menu") &&
      !content.includes("appMenuOpen"),
  ],
  [
    "src/App.vue",
    "titlebar menu panel removed",
    (content) =>
      !content.includes("appMenuOpen") &&
      !content.includes("app-menu-panel") &&
      !content.includes("toggleAppMenu") &&
      !content.includes("refreshDashboard") &&
      !content.includes("focusLogsFromMenu") &&
      !content.includes('@click.stop="toggleAppMenu"'),
  ],
  [
    "src/App.vue",
    "custom titlebar can initiate native window drag",
    (content) =>
      content.includes("startWindowDrag") &&
      content.includes("appWindow.startDragging()") &&
      content.includes('@mousedown="startWindowDrag"'),
  ],
  [
    "src/App.vue",
    "maximize button removed from custom titlebar",
    (content) =>
      !content.includes("toggleMaximizeWindow") &&
      !content.includes("control-maximize") &&
      !content.includes("appWindow.toggleMaximize()"),
  ],
  [
    "src/App.vue",
    "IPv6 interface picker uses styled custom dropdown",
    (content) =>
      content.includes("ipv6DropdownOpen") &&
      content.includes("ipv6-select-menu") &&
      content.includes("selectIpv6Interface") &&
      content.includes("ChevronDown") &&
      !content.includes('<select\n                  v-model="selectedIpv6Interface"'),
  ],
  [
    "src/components/StatusCard.vue",
    "uses Lucide status icons",
    (content) =>
      content.includes('from "@lucide/vue"') &&
      content.includes("Globe") &&
      content.includes("ShieldCheck"),
  ],
  [
    "src/components/StatusCard.vue",
    "status cards support a top-right action slot",
    (content) =>
      content.includes("$slots.action") &&
      content.includes('class="card-action"') &&
      content.includes(".has-action .card-copy"),
  ],
  [
    "src/App.vue",
    "public IPv4 and IPv6 cards have copy buttons",
    (content) =>
      content.includes("copyPublicIp") &&
      content.includes("copiedPublicIp") &&
      content.includes("navigator.clipboard.writeText") &&
      content.includes("copy-public-ip") &&
      content.includes("Copy") &&
      content.includes("复制公网 IPv4") &&
      content.includes("复制公网 IPv6"),
  ],
  [
    "src-tauri/tauri.conf.json",
    "only manual tray icon is configured",
    (content) => !content.includes('"trayIcon"'),
  ],
  [
    "src-tauri/src/tray.rs",
    "manual tray icon remains configured",
    (content) => content.includes("TrayIconBuilder::new()") && content.includes(".menu(&menu)"),
  ],
  [
    "src/components/DdnsPanel.vue",
    "success footer uses Lucide icon",
    (content) =>
      content.includes("CircleCheck") &&
      content.includes("<CircleCheck") &&
      !content.includes('class="checkmark"') &&
      !content.includes(".checkmark::before"),
  ],
  [
    "src/components/DdnsPanel.vue",
    "domain field is not styled as an unimplemented dropdown",
    (content) =>
      content.includes('<input v-model="config.domain" type="text" />') &&
      !content.includes('class="select-like"') &&
      !content.includes('class="chevron"'),
  ],
  [
    "src/components/ForwardRulesPanel.vue",
    "hint footer uses Lucide icon",
    (content) =>
      content.includes("Info") &&
      content.includes("<Info") &&
      !content.includes('class="info-icon"') &&
      !content.includes(".info-icon::before"),
  ],
  [
    "src/components/ForwardRulesPanel.vue",
    "rules panel footer stays at groupbox bottom",
    (content) => {
      const normalized = content.replace(/\r\n/g, "\n");
      return (
        normalized.includes(".rules-panel {\n  height: 100%;\n}") &&
        normalized.includes(".table-wrapper {\n  flex: 1 1 auto;\n  overflow: auto;\n  min-height: 0;\n}") &&
        normalized.includes(".panel-footer {\n  flex: 0 0 auto;")
      );
    },
  ],
  [
    "src/components/ForwardRulesPanel.vue",
    "forward form exposes implemented TCP and UDP relay modes",
    (content) =>
      content.includes('const IMPLEMENTED_PROTOCOLS = ["TCP", "UDP", "TCP+UDP"] as const;') &&
      content.includes('function normalizeProtocol(protocol: string): ForwardProtocol') &&
      content.includes('const FORWARD_MODE = "relay";') &&
      content.includes('const FORWARD_MODE_LABEL = "普通 TCP/UDP 转发";') &&
      content.includes('<select v-model="editorForm.protocol">') &&
      content.includes('v-for="protocol in IMPLEMENTED_PROTOCOLS"') &&
      content.includes(':value="FORWARD_MODE_LABEL"') &&
      !content.includes("NAT（默认）") &&
      !content.includes("透明转发"),
  ],
  [
    "src-tauri/src/config.rs",
    "forward defaults match implemented TCP relay mode",
    (content) =>
      content.includes('fn default_protocol() -> String {\n    "TCP".into()\n}') &&
      content.includes('fn default_mode() -> String {\n    "relay".into()\n}'),
  ],
  [
    "src-tauri/src/commands.rs",
    "forward save normalizes to implemented TCP and UDP relay modes",
    (content) =>
      content.includes("ensure_supported_forward_mode(&rule.mode)?;") &&
      content.includes("fn ensure_supported_forward_mode(mode: &str) -> Result<(), String>") &&
      content.includes("当前版本仅支持普通 TCP/UDP 转发") &&
      content.includes("内核 NAT") &&
      content.includes("透明源地址透传") &&
      content.includes("normalize_forward_rule(&mut rule);"),
  ],
  [
    "src-tauri/src/forward/mod.rs",
    "system forwarding capability module is registered",
    (content) => content.includes("pub mod system;"),
  ],
  [
    "src-tauri/src/forward/system.rs",
    "system forwarding capability boundary is explicit",
    (content) =>
      content.includes("pub fn capabilities_summary() -> &'static str") &&
      content.includes("pub fn log_capabilities()") &&
      content.includes("普通 TCP/UDP 转发已启用") &&
      content.includes("内核 NAT") &&
      content.includes("透明源地址透传") &&
      content.includes("尚未接入"),
  ],
  [
    "src-tauri/src/lib.rs",
    "startup logs forwarding capability boundary",
    (content) => content.includes("forward::system::log_capabilities();"),
  ],
  [
    "src-tauri/src/config.rs",
    "legacy forward rules are migrated to implemented relay modes",
    (content) =>
      content.includes("pub fn normalize_forward_protocol(protocol: &str) -> String") &&
      content.includes('"UDP" => "UDP".to_string()') &&
      content.includes('"TCP+UDP" | "UDP+TCP" => "TCP+UDP".to_string()') &&
      content.includes("pub fn normalize_forward_rule(rule: &mut ForwardRule) -> bool") &&
      content.includes('rule.mode = "relay".into();') &&
      content.includes("fn normalize_app_config(config: &mut AppConfig) -> bool") &&
      content.includes("normalize_forward_rule(rule)") &&
      content.includes("已迁移旧版转发规则配置"),
  ],
  [
    "src-tauri/src/forward/mod.rs",
    "UDP forward module is registered",
    (content) => content.includes("pub mod udp;"),
  ],
  [
    "src-tauri/src/forward/manager.rs",
    "forward manager starts TCP and UDP listeners",
    (content) =>
      content.includes("signature: String") &&
      content.includes("cancels: Vec<CancellationToken>") &&
      content.includes("fn rule_signature(rule: &ForwardRule) -> String") &&
      content.includes("fn protocol_kinds(rule: &ForwardRule) -> Vec<ForwardProtocolKind>") &&
      content.includes("ForwardProtocolKind::Tcp") &&
      content.includes("ForwardProtocolKind::Udp") &&
      content.includes("udp::spawn_forwarder(rule).await") &&
      content.includes("active.signature == signature") &&
      content.includes("配置已变更，正在重启监听"),
  ],
  [
    "src-tauri/src/forward/tcp.rs",
    "TCP forwarder formats IPv6 listen and target endpoints",
    (content) =>
      content.includes("fn format_socket_endpoint(host: &str, port: u16, default_host: &str) -> String") &&
      content.includes('format!("[{}]:{}", normalized, port)') &&
      content.includes('format_socket_endpoint(&rule.listen_addr, rule.listen_port, "::")') &&
      content.includes('format_socket_endpoint(target_ip, target_port, "")'),
  ],
  [
    "src-tauri/src/forward/udp.rs",
    "UDP relay forwarder is implemented",
    (content) =>
      content.includes("pub async fn spawn_forwarder(rule: &ForwardRule) -> Result<CancellationToken, String>") &&
      content.includes("UdpSocket::bind") &&
      content.includes("recv_from") &&
      content.includes("send_to") &&
      content.includes("get_or_create_target_socket") &&
      content.includes("UDP 转发规则"),
  ],
  [
    "src/App.vue",
    "runtime cards do not show fake online device count",
    (content) =>
      content.includes('title="运行时长"') &&
      content.includes("formatUptime(statusData.uptime)") &&
      content.includes('icon="uptime"') &&
      !content.includes("在线设备") &&
      !content.includes("online_device_count"),
  ],
  [
    "src/types.ts",
    "RuntimeStatus omits fake online device count",
    (content) => !content.includes("online_device_count"),
  ],
  [
    "src-tauri/src/config.rs",
    "RuntimeStatus omits fake online device count",
    (content) => !content.includes("online_device_count"),
  ],
  [
    "src-tauri/src/commands.rs",
    "runtime status omits fake online device count",
    (content) => !content.includes("online_device_count: 0"),
  ],
  [
    "src-tauri/capabilities/default.json",
    "custom window controls have Tauri permissions",
    (content) =>
      [
        "core:window:allow-minimize",
        "core:window:allow-close",
        "core:window:allow-start-dragging",
        "core:window:allow-is-maximized",
        "core:window:allow-unmaximize",
        "core:window:allow-set-size",
        "core:window:allow-center",
      ].every((permission) => content.includes(permission)),
  ],
];

for (const [file, label, predicate] of cssChecks) {
  const filePath = join(root, file);
  if (!existsSync(filePath)) {
    missing.push(`${file}: ${label}`);
    continue;
  }
  const content = readFileSync(filePath, "utf8");
  if (!predicate(content)) {
    missing.push(`${file}: ${label}`);
  }
}

const noSampleChecks = [
  [
    "src/App.vue",
    ["101.42.16.88", "2408:4007:808:1234::1", "fallbackStatus", "useReferenceStatus"],
  ],
  [
    "src/components/DdnsPanel.vue",
    ["fallbackConfig", "example.com", "LTAI5t", "2025-05-15 14:32:18"],
  ],
  [
    "src/components/ForwardRulesPanel.vue",
    ["fallbackRules", "192.168.1.10", "rule-rdp", "远程桌面", "HTTPS 服务", "temp-"],
  ],
  [
    "src/components/LogPanel.vue",
    ["fallbackLogs", "home.example.com", "useReferenceLogs", "2025-05-15"],
  ],
];

for (const [file, needles] of noSampleChecks) {
  const content = readFileSync(join(root, file), "utf8");
  for (const needle of needles) {
    if (content.includes(needle)) {
      missing.push(`${file}: remove sample data '${needle}'`);
    }
  }
}

const backendLogChecks = [
  ["src-tauri/src/commands.rs", "add_log(\"info\", \"DDNS\""],
  ["src-tauri/src/commands.rs", "配置已保存"],
  ["src-tauri/src/commands.rs", "日志已清空"],
  ["src-tauri/src/commands.rs", "转发规则"],
  ["src-tauri/src/commands.rs", "get_ddns_current_record"],
  ["src-tauri/src/commands.rs", "get_auto_start"],
  ["src-tauri/src/commands.rs", "list_network_interfaces"],
  ["src-tauri/src/commands.rs", "get_ipv6_interface"],
  ["src-tauri/src/commands.rs", "set_ipv6_interface"],
  ["src-tauri/src/config.rs", "ipv6_interface"],
  ["src-tauri/src/ddns/mod.rs", "NetworkInterfaceInfo"],
  ["src-tauri/src/ddns/mod.rs", "get_local_ipv6_for_interface"],
  ["src-tauri/src/lib.rs", "应用启动中"],
  ["src-tauri/src/lib.rs", "commands::list_network_interfaces"],
  ["src-tauri/src/lib.rs", "commands::set_ipv6_interface"],
];

for (const [file, needle] of backendLogChecks) {
  const content = readFileSync(join(root, file), "utf8");
  if (!content.includes(needle)) {
    missing.push(`${file}: Chinese log text '${needle}'`);
  }
}

const noEnglishBackendLogs = [
  ["src-tauri/src/commands.rs", ["DDNS config saved", "Forward rule saved", "Log buffer cleared", "Auto-start"]],
  ["src-tauri/src/lib.rs", ["Application starting", "Application setup complete", "background task started"]],
  ["src-tauri/src/forward/manager.rs", ["Started forwarder", "Stopped listener", "Failed to start forwarder"]],
  ["src-tauri/src/forward/tcp.rs", ["listening on", "listener stopped", "connection from", "accept error", "failed to connect"]],
  ["src-tauri/src/autostart.rs", ["Auto-start enabled", "Auto-start disabled"]],
  ["src-tauri/src/tray.rs", ["User requested exit"]],
];

for (const [file, needles] of noEnglishBackendLogs) {
  const content = readFileSync(join(root, file), "utf8");
  for (const needle of needles) {
    if (content.includes(needle)) {
      missing.push(`${file}: replace English backend log '${needle}'`);
    }
  }
}

if (missing.length > 0) {
  console.error("Reference UI checks failed:");
  for (const item of missing) {
    console.error(`- Missing ${item}`);
  }
  process.exit(1);
}

console.log("Reference UI checks passed.");
