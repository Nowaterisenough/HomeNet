import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const missing = [];

const checks = [
  ["package.json", '"build"'],
  ["package.json", '"verify"'],
  ["src/App.tsx", "HomeNet"],
  ["src/App.tsx", 'invokeCommand<RuntimeStatus>("get_runtime_status")'],
  ["src/App.tsx", 'invokeCommand<AppUpdateResult>("install_app_update")'],
  ["src/components/ForwardRulesPanel.tsx", 'invokeCommand<ForwardRule[]>("list_forward_rules")'],
  ["src/components/ForwardRulesPanel.tsx", 'invokeCommand<ForwardRule>("save_forward_rule"'],
  ["src/components/DeviceDdnsPanel.tsx", 'invokeCommand<LanDevice[]>("list_lan_devices")'],
  [
    "src/components/DeviceDdnsPanel.tsx",
    'invokeCommand<DeviceDdnsConfig[]>("list_device_ddns_configs")',
  ],
  ["src/components/DeviceDdnsPanel.tsx", 'invokeCommand("save_device_ddns_config"'],
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
  ["src/components/LogPanel.tsx", 'invokeCommand<LogEntry[]>("get_recent_logs")'],
  ["src/components/RuntimeSettingsPanel.tsx", "检查更新"],
  ["src/hooks/useDraggableModal.ts", "startModalDrag"],
  ["src/lib/tauri.ts", "currentTauriWindow"],
  ["src/styles/global.css", "--titlebar-height"],
  ["src/styles/panels.css", ".modal-backdrop"],
];

for (const [file, needle] of checks) {
  const filePath = join(root, file);
  if (!existsSync(filePath)) {
    missing.push(`${file}: file is missing`);
    continue;
  }

  const content = readFileSync(filePath, "utf8");
  if (!content.includes(needle)) {
    missing.push(`${file}: ${needle}`);
  }
}

if (missing.length > 0) {
  console.error("React frontend checks failed:");
  for (const item of missing) {
    console.error(`- Missing ${item}`);
  }
  process.exit(1);
}

console.log("React frontend checks passed.");
