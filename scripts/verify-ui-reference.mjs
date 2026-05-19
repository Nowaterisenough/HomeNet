import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const missing = [];

const checks = [
  ["src/App.vue", 'from "./components/StatusCard.vue"'],
  ["src/App.vue", 'from "./components/ForwardRulesPanel.vue"'],
  ["src/App.vue", 'from "./components/DeviceDdnsPanel.vue"'],
  ["src/App.vue", 'from "./components/ReverseProxyPanel.vue"'],
  ["src/App.vue", 'from "./components/LogPanel.vue"'],
  ["src/App.vue", 'from "./components/RuntimeSettingsPanel.vue"'],
  ["src/App.vue", 'invoke<RuntimeStatus>("get_runtime_status")'],
  ["src/App.vue", 'invoke<AppUpdateResult>("install_app_update")'],
  ["src/App.vue", 'invoke<boolean>("get_auto_start")'],
  ["src/App.vue", 'invoke("set_auto_start"'],
  ["src/App.vue", "正在检查更新"],
  ["src/App.vue", "检查更新失败"],
  ["src/types.ts", "export interface AppUpdateResult"],
  ["src/types.ts", '"unavailable"'],
  ["src/components/RuntimeSettingsPanel.vue", "检查更新"],
  ["src/components/RuntimeSettingsPanel.vue", "自动检查更新"],
  ["src/components/ForwardRulesPanel.vue", "overflow: auto"],
  ["src/components/DeviceDdnsPanel.vue", "overflow: auto"],
  ["src/components/ReverseProxyPanel.vue", "overflow: auto"],
  ["src/components/ReverseProxyPanel.vue", 'invoke<ReverseProxyRule[]>("list_reverse_proxy_rules")'],
  ["src/components/ReverseProxyPanel.vue", 'invoke<ReverseProxyRule>("save_reverse_proxy_rule"'],
  ["src/components/ReverseProxyPanel.vue", 'invoke<ReverseProxyRule>("issue_reverse_proxy_certificate"'],
  ["src/components/ReverseProxyPanel.vue", "editor.acme_email"],
  ["src/components/ReverseProxyPanel.vue", 'value="auto"'],
  ["src/components/DeviceDdnsPanel.vue", 'invoke<LanDevice[]>("list_lan_devices")'],
  ["src/components/DeviceDdnsPanel.vue", 'invoke<DeviceDdnsConfig[]>("list_device_ddns_configs")'],
  ["src/components/DeviceDdnsPanel.vue", 'invoke("save_device_ddns_config"'],
  ["src/components/DeviceDdnsPanel.vue", 'invoke("delete_device_ddns_config"'],
  ["src/components/DeviceDdnsPanel.vue", "useDraggableModal"],
  ["src/components/ForwardRulesPanel.vue", "useDraggableModal"],
  ["src/components/ReverseProxyPanel.vue", "useDraggableModal"],
  ["src/components/ReverseProxyPanel.vue", "证书配置"],
  ["src/components/ReverseProxyPanel.vue", "editor.certificate"],
  ["src/types.ts", "acme_email"],
  ["src/types.ts", "certificate_path"],
  ["src/components/LogPanel.vue", "overflow: auto"],
  ["src-tauri/tauri.conf.json", '"productName": "HomeNet"'],
  ["src-tauri/tauri.conf.json", '"updater"'],
  ["src-tauri/tauri.updater.conf.json", '"createUpdaterArtifacts": true'],
  ["src-tauri/Cargo.toml", 'name = "homenet"'],
  ["src-tauri/Cargo.toml", "tauri-plugin-updater"],
  ["src-tauri/src/commands.rs", "install_app_update"],
  ["src-tauri/src/commands.rs", "list_reverse_proxy_rules"],
  ["src-tauri/src/commands.rs", "save_reverse_proxy_rule"],
  ["src-tauri/src/commands.rs", "enable_reverse_proxy_rule"],
  ["src-tauri/src/commands.rs", "issue_reverse_proxy_certificate"],
  ["src-tauri/src/certificates.rs", "issue_certificate"],
  ["src-tauri/src/commands.rs", "GITHUB_LATEST_RELEASE_API"],
  ["src-tauri/src/commands.rs", "select_github_update_asset"],
  ["src-tauri/src/commands.rs", "sudo xattr -dr com.apple.quarantine /Applications/HomeNet.app"],
  ["src-tauri/src/config.rs", "pub reverse_proxy_rules: Vec<ReverseProxyRule>"],
  ["src-tauri/src/reverse_proxy.rs", "pub struct ReverseProxyManager"],
  ["src-tauri/src/lib.rs", "tauri_plugin_updater::Builder::new().build()"],
  ["src-tauri/src/lib.rs", "reverse_proxy_background_task"],
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

const forbiddenChecks = [
  [".github/workflows/build.yml", "Read-UpdaterSignature"],
  [".github/workflows/build.yml", "macos_signing_enabled"],
  ["src/App.vue", "appMenuOpen"],
  ["src/App.vue", "fallbackStatus"],
  ["src/App.vue", "101.42.16.88"],
  ["src/App.vue", "2408:4007"],
  ["src/App.vue", 'from "./components/DdnsPanel.vue"'],
  ["src/components/ForwardRulesPanel.vue", "fallbackRules"],
  ["src/components/ForwardRulesPanel.vue", "192.168.1.10"],
  ["src/components/DeviceDdnsPanel.vue", "fallbackDevices"],
  ["src/components/DeviceDdnsPanel.vue", "example.com"],
  ["src/components/DdnsPanel.vue", "example.com"],
  ["src/components/LogPanel.vue", "fallbackLogs"],
  ["src/components/LogPanel.vue", "101.42.16.88"],
  ["src/components/ReverseProxyPanel.vue", "proxyRules"],
  ["src/components/ReverseProxyPanel.vue", "nas.example.com"],
];

for (const [file, needle] of forbiddenChecks) {
  const filePath = join(root, file);
  if (!existsSync(filePath)) {
    continue;
  }

  const content = readFileSync(filePath, "utf8");
  if (content.includes(needle)) {
    missing.push(`${file}: remove stale implementation '${needle}'`);
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
