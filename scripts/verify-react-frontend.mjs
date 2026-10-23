import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const missing = [];

const checks = [
  ["package.json", '"build"'],
  ["package.json", '"verify"'],
  ["src/App.tsx", "HomeNet"],
  ["src/App.tsx", 'invokeCommand<RuntimeStatus>("get_runtime_status")'],
  ["src/App.tsx", 'invokeCommand<AppUpdateResult>("check_app_update")'],
  ["src/App.tsx", 'invokeCommand<AppUpdateResult>("install_app_update",'],
  [
    "src/components/ForwardRulesPanel.tsx",
    'invokeCommand<ForwardRule[]>("list_forward_rules")',
  ],
  [
    "src/components/ForwardRulesPanel.tsx",
    'invokeCommand<ForwardRule>("save_forward_rule"',
  ],
  [
    "src/hooks/useDeviceInventory.ts",
    'invokeCommand<LanDevice[]>("list_lan_devices")',
  ],
  [
    "src/hooks/useDeviceInventory.ts",
    'invokeCommand<DeviceDdnsConfig[]>("list_device_ddns_configs")',
  ],
  [
    "src/components/DeviceDdnsPanel.tsx",
    'invokeCommand("save_device_ddns_config"',
  ],
  ["src/components/DeviceDdnsPanel.tsx", "prioritizeEnabledDeviceRows"],
  ["src/components/HomeNetIcon.tsx", "tray-icon.svg"],
  ["src/components/DeviceDdnsPanel.tsx", "device-status-icons"],
  [
    "src/components/ReverseProxyPanel.tsx",
    'invokeCommand<ReverseProxyRule[]>("list_reverse_proxy_rules")',
  ],
  [
    "src/components/ReverseProxyPanel.tsx",
    'invokeCommand<ReverseProxyRule>("save_reverse_proxy_rule"',
  ],
  [
    "src/components/ReverseProxyPanel.tsx",
    'invokeCommand<ReverseProxyRule>("issue_reverse_proxy_certificate"',
  ],
  [
    "src/components/WebDavPanel.tsx",
    'invokeCommand<WebDavConfigView>("get_webdav_config")',
  ],
  [
    "src/components/WebDavPanel.tsx",
    'invokeCommand<WebDavConfigView>("save_webdav_config"',
  ],
  ["src/components/WebDavPanel.tsx", "账号管理"],
  [
    "src/components/LogPanel.tsx",
    'invokeCommand<LogEntry[]>("get_recent_logs")',
  ],
  ["src/components/RuntimeSettingsPanel.tsx", "检查更新"],
  ["src/components/UpdateConfirmDialog.tsx", 'role="dialog"'],
  ["src/components/UpdateConfirmDialog.tsx", "下载并安装"],
  ["src/components/EditorDrawer.tsx", 'aria-modal="true"'],
  ["src/lib/tauri.ts", "currentTauriWindow"],
  ["src/styles/global.css", "--titlebar-height"],
  ["src/styles/panels.css", ".modal-backdrop"],
  ["src/styles/panels.css", ".webdav-panel"],
];

for (const [file, needle] of checks) {
  const filePath = join(root, file);
  if (!existsSync(filePath)) {
    missing.push(`${file}: file is missing`);
    continue;
  }

  const content = readFileSync(filePath, "utf8");
  // Command wiring checks should survive line wrapping by the formatter.
  if (!content.replace(/\s+/g, "").replace(/,\)/g, ")").includes(needle.replace(/\s+/g, ""))) {
    missing.push(`${file}: ${needle}`);
  }
}

const appContent = readFileSync(join(root, "src/App.tsx"), "utf8");
if (appContent.includes("window.confirm")) {
  missing.push(
    "src/App.tsx: browser confirm must not gate app update installation",
  );
}

if (missing.length > 0) {
  console.error("React frontend checks failed:");
  for (const item of missing) {
    console.error(`- Missing ${item}`);
  }
  process.exit(1);
}

console.log("React frontend checks passed.");
