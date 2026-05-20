import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const missing = [];

const requiredChecks = [
  ["package.json", '"build": "tsc --noEmit -p tsconfig.json && vite build"'],
  ["package.json", '"react"'],
  ["package.json", '"react-dom"'],
  ["package.json", '"lucide-react"'],
  ["package.json", '"@vitejs/plugin-react"'],
  ["vite.config.ts", 'from "@vitejs/plugin-react"'],
  ["vite.config.ts", "plugins: [react()]"],
  ["index.html", '<div id="root"></div>'],
  ["index.html", 'src="/src/main.tsx"'],
  ["src/main.tsx", "ReactDOM.createRoot"],
  ["src/main.tsx", 'from "./App"'],
  ["src/App.tsx", 'from "./components/StatusCard"'],
  ["src/App.tsx", 'from "./components/ForwardRulesPanel"'],
  ["src/App.tsx", 'from "./components/DeviceDdnsPanel"'],
  ["src/App.tsx", 'from "./components/ReverseProxyPanel"'],
  ["src/App.tsx", 'from "./components/LogPanel"'],
  ["src/App.tsx", 'from "./components/RuntimeSettingsPanel"'],
  ["src/App.tsx", 'invokeCommand<RuntimeStatus>("get_runtime_status")'],
  ["src/App.tsx", 'invokeCommand<AppUpdateResult>("install_app_update")'],
  ["src/App.tsx", 'invokeCommand<boolean>("get_auto_start")'],
  ["src/App.tsx", 'invokeCommand("set_auto_start"'],
  ["src/App.tsx", "<h1>HomeNet</h1>"],
  ["src/App.tsx", "正在检查更新"],
  ["src/App.tsx", "检查更新失败"],
  ["src/types.ts", "export interface AppUpdateResult"],
  ["src/types.ts", '"unavailable"'],
  ["src/components/RuntimeSettingsPanel.tsx", "检查更新"],
  ["src/components/RuntimeSettingsPanel.tsx", "自动检查更新"],
  ["src/components/ForwardRulesPanel.tsx", 'invokeCommand<ForwardRule[]>("list_forward_rules")'],
  ["src/components/ForwardRulesPanel.tsx", 'invokeCommand<ForwardRule>("save_forward_rule"'],
  ["src/components/DeviceDdnsPanel.tsx", 'invokeCommand<LanDevice[]>("list_lan_devices")'],
  [
    "src/components/DeviceDdnsPanel.tsx",
    'invokeCommand<DeviceDdnsConfig[]>("list_device_ddns_configs")',
  ],
  ["src/components/DeviceDdnsPanel.tsx", 'invokeCommand("save_device_ddns_config"'],
  ["src/components/DeviceDdnsPanel.tsx", 'invokeCommand("delete_device_ddns_config"'],
  ["src/components/DeviceDdnsPanel.tsx", "useDraggableModal"],
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
  ["src/components/ReverseProxyPanel.tsx", "匹配域名"],
  ["src/components/ReverseProxyPanel.tsx", "DDNS"],
  ["src/components/ReverseProxyPanel.tsx", "证书配置"],
  ["src/components/LogPanel.tsx", 'invokeCommand<LogEntry[]>("get_recent_logs")'],
  ["src/hooks/useDraggableModal.ts", "startModalDrag"],
  ["src/lib/tauri.ts", "currentTauriWindow"],
  ["src/styles/global.css", "--titlebar-height"],
  ["src/styles/panels.css", ".modal-backdrop"],
  ["src-tauri/tauri.conf.json", '"productName": "HomeNet"'],
  ["src-tauri/tauri.conf.json", '"title": "HomeNet"'],
  ["src-tauri/tauri.conf.json", '"devUrl": "http://localhost:1420"'],
  ["src-tauri/tauri.conf.json", '"updater"'],
  ["src-tauri/tauri.updater.conf.json", '"createUpdaterArtifacts": true'],
  ["src-tauri/Cargo.toml", 'name = "homenet"'],
  ["src-tauri/Cargo.toml", 'name = "HomeNet"'],
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
  ["src-tauri/src/tray.rs", 'const TRAY_TOOLTIP: &str = "HomeNet";'],
  ["src-tauri/src/tray.rs", "TrayIconEvent::DoubleClick"],
  ["src-tauri/src/tray.rs", "TrayIconAction::RestoreMainWindow"],
  ["src-tauri/src/config.rs", "pub reverse_proxy_rules: Vec<ReverseProxyRule>"],
  ["src-tauri/src/reverse_proxy.rs", "pub struct ReverseProxyManager"],
  ["src-tauri/src/lib.rs", "tauri_plugin_updater::Builder::new().build()"],
  ["src-tauri/src/lib.rs", "reverse_proxy_background_task"],
];

for (const [file, needle] of requiredChecks) {
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
  ["package.json", '"vue"'],
  ["package.json", '"vue-tsc"'],
  ["package.json", '"@vitejs/plugin-vue"'],
  ["package.json", '"@lucide/vue"'],
  ["vite.config.ts", "@vitejs/plugin-vue"],
  ["index.html", 'id="app"'],
  ["index.html", 'src="/src/main.ts"'],
  ["src/vite-env.d.ts", "*.vue"],
  ["src/vite-env.d.ts", 'from "vue"'],
  [".github/workflows/build.yml", "Read-UpdaterSignature"],
  [".github/workflows/build.yml", "macos_signing_enabled"],
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

const removedVueFiles = [
  "src/main.ts",
  "src/App.vue",
  "src/components/DdnsPanel.vue",
  "src/components/DeviceDdnsPanel.vue",
  "src/components/ForwardRulesPanel.vue",
  "src/components/LogPanel.vue",
  "src/components/ReverseProxyPanel.vue",
  "src/components/RuntimeSettingsPanel.vue",
  "src/components/StatusCard.vue",
  "src/composables/useDraggableModal.ts",
  "src/assets/vue.svg",
];

for (const file of removedVueFiles) {
  if (existsSync(join(root, file))) {
    missing.push(`${file}: stale Vue file should be removed`);
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
