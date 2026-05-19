<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Menu } from "@lucide/vue";
import StatusCard from "./components/StatusCard.vue";
import DeviceDdnsPanel from "./components/DeviceDdnsPanel.vue";
import ForwardRulesPanel from "./components/ForwardRulesPanel.vue";
import LogPanel from "./components/LogPanel.vue";
import ReverseProxyPanel from "./components/ReverseProxyPanel.vue";
import RuntimeSettingsPanel from "./components/RuntimeSettingsPanel.vue";
import type { AppUpdateResult, LanDevice, RuntimeStatus } from "./types";

const emptyStatus: RuntimeStatus = {
  public_ipv4: "",
  public_ipv6: "",
  ddns_status: "",
  last_update_time: "",
  rule_count: 0,
  enabled_rule_count: 0,
  reverse_proxy_rule_count: 0,
  enabled_reverse_proxy_rule_count: 0,
  uptime: 0,
};

const statusData = ref<RuntimeStatus>({ ...emptyStatus });
const deviceSummary = ref({ online: 0, total: 0 });
const autoStartEnabled = ref(false);
const autoStartSaving = ref(false);
const updateChecking = ref(false);
const appVersion = ref("0.1.4");
const updateStatusMessage = ref("");
const updateStatusType = ref<"normal" | "success" | "error">("normal");

const DESIGN_WIDTH = 1600;
const DESIGN_HEIGHT = 1000;
const referenceWindowSize = new LogicalSize(DESIGN_WIDTH, DESIGN_HEIGHT);
let runtimeTimer: ReturnType<typeof setInterval> | null = null;
let deviceTimer: ReturnType<typeof setInterval> | null = null;

const disabledRuleCount = computed(() =>
  Math.max(0, statusData.value.rule_count - statusData.value.enabled_rule_count),
);
const disabledReverseProxyCount = computed(() =>
  Math.max(
    0,
    statusData.value.reverse_proxy_rule_count -
      statusData.value.enabled_reverse_proxy_rule_count,
  ),
);

const statusCards = computed(() => [
  {
    title: "公网 IPv4",
    value: displayValue(statusData.value.public_ipv4),
    subtitle: "运营商：中国电信",
    icon: "globe",
    status: "normal" as const,
  },
  {
    title: "公网 IPv6",
    value: statusData.value.public_ipv6.trim()
      ? `${displayValue(statusData.value.public_ipv6)}/64`
      : "--",
    subtitle: ipv6PrefixSubtitle(statusData.value.public_ipv6),
    icon: "ipv6",
    status: "normal" as const,
  },
  {
    title: "DDNS 状态",
    value: statusData.value.ddns_status || "未连接",
    subtitle: `最后同步：${statusData.value.last_update_time || "暂无"}`,
    icon: "shield",
    status: ddnsStatusType(),
  },
  {
    title: "转发规则数",
    value: String(statusData.value.rule_count),
    subtitle: `启用：${statusData.value.enabled_rule_count}　禁用：${disabledRuleCount.value}`,
    icon: "rules",
    status: "normal" as const,
  },
  {
    title: "反向代理数",
    value: String(statusData.value.reverse_proxy_rule_count),
    subtitle: `启用：${statusData.value.enabled_reverse_proxy_rule_count}　禁用：${disabledReverseProxyCount.value}`,
    icon: "proxy",
    status: "normal" as const,
  },
  {
    title: "在线设备 / 发现设备",
    value: `${deviceSummary.value.online} / ${deviceSummary.value.total}`,
    subtitle: "局域网扫描结果",
    icon: "devices",
    status: "normal" as const,
  },
]);

function syncUiScale() {
  const scale = Math.min(
    window.innerWidth / DESIGN_WIDTH,
    window.innerHeight / DESIGN_HEIGHT,
  );
  const frameX = Math.max(0, (window.innerWidth - DESIGN_WIDTH * scale) / 2);
  const frameY = Math.max(0, (window.innerHeight - DESIGN_HEIGHT * scale) / 2);

  document.documentElement.style.setProperty("--ui-scale", scale.toFixed(4));
  document.documentElement.style.setProperty("--frame-x", `${frameX.toFixed(2)}px`);
  document.documentElement.style.setProperty("--frame-y", `${frameY.toFixed(2)}px`);
}

function displayValue(value: string): string {
  return value?.trim() ? value : "--";
}

function ipv6PrefixSubtitle(value: string): string {
  const parts = value.split(":");
  if (!value.trim() || parts.length < 4) {
    return "前缀：--";
  }
  return `前缀：${parts.slice(0, 4).join(":")}::/64`;
}

function ddnsStatusType(): "normal" | "warning" | "error" | "success" {
  if (statusData.value.ddns_status === "运行中") return "success";
  if (statusData.value.ddns_status === "未连接") return "warning";
  return "normal";
}

async function loadRuntimeStatus() {
  try {
    const data = await invoke<RuntimeStatus>("get_runtime_status");
    statusData.value = {
      ...emptyStatus,
      ...data,
    };
  } catch {
    statusData.value = { ...emptyStatus };
  }
}

async function loadDeviceSummary() {
  try {
    const devices = await invoke<LanDevice[]>("list_lan_devices");
    deviceSummary.value = {
      online: devices.filter((device) => device.online).length,
      total: devices.length,
    };
  } catch {
    deviceSummary.value = { online: 0, total: 0 };
  }
}

async function loadAutoStart() {
  try {
    autoStartEnabled.value = await invoke<boolean>("get_auto_start");
  } catch {
    autoStartEnabled.value = false;
  }
}

function notifyLogsChanged() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
}

async function toggleAutoStart(enabled: boolean) {
  const previous = autoStartEnabled.value;
  autoStartEnabled.value = enabled;
  autoStartSaving.value = true;

  try {
    await invoke("set_auto_start", { enabled });
    notifyLogsChanged();
  } catch (e) {
    autoStartEnabled.value = previous;
    console.warn("设置开机自启动失败:", e);
  } finally {
    autoStartSaving.value = false;
  }
}

async function installAppUpdate() {
  if (updateChecking.value) return;
  updateChecking.value = true;
  updateStatusType.value = "normal";
  updateStatusMessage.value = "正在检查更新";

  try {
    const result = await invoke<AppUpdateResult>("install_app_update");
    updateStatusMessage.value = result.message;
    appVersion.value = result.current_version || appVersion.value;
    updateStatusType.value = result.status === "installed" ? "success" : "normal";
    notifyLogsChanged();
  } catch (e) {
    updateStatusMessage.value = `检查更新失败：${String(e)}`;
    updateStatusType.value = "error";
  } finally {
    updateChecking.value = false;
  }
}

async function minimizeWindow() {
  const appWindow = currentTauriWindow();
  if (!appWindow) return;
  await appWindow.minimize();
}

async function maximizeWindow() {
  const appWindow = currentTauriWindow();
  if (!appWindow) return;
  if (await appWindow.isMaximized()) {
    await appWindow.unmaximize();
  } else {
    await appWindow.maximize();
  }
}

async function closeWindow() {
  const appWindow = currentTauriWindow();
  if (!appWindow) return;
  await appWindow.close();
}

async function startWindowDrag(event: MouseEvent) {
  if (event.button !== 0) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest("button, input, select, textarea, a, label")) return;
  try {
    const appWindow = currentTauriWindow();
    if (!appWindow) return;
    await appWindow.startDragging();
  } catch (e) {
    console.warn("启动窗口拖动失败:", e);
  }
}

function currentTauriWindow() {
  if (!("__TAURI_INTERNALS__" in window)) return null;
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

async function restoreReferenceWindow() {
  try {
    const appWindow = currentTauriWindow();
    if (!appWindow) return;
    if (await appWindow.isMaximized()) {
      await appWindow.unmaximize();
    }
    await appWindow.setSize(referenceWindowSize);
    await appWindow.center();
  } catch {
    // Browser preview and restricted window managers do not expose Tauri sizing APIs.
  }
}

onMounted(() => {
  syncUiScale();
  window.addEventListener("resize", syncUiScale);
  restoreReferenceWindow();
  loadRuntimeStatus();
  loadDeviceSummary();
  loadAutoStart();
  window.addEventListener("homenet:refresh-status", loadRuntimeStatus);
  window.addEventListener("homenet:devices-refresh", loadDeviceSummary);
  runtimeTimer = setInterval(loadRuntimeStatus, 10000);
  deviceTimer = setInterval(loadDeviceSummary, 30000);
});

onUnmounted(() => {
  window.removeEventListener("resize", syncUiScale);
  window.removeEventListener("homenet:refresh-status", loadRuntimeStatus);
  window.removeEventListener("homenet:devices-refresh", loadDeviceSummary);
  if (runtimeTimer !== null) {
    clearInterval(runtimeTimer);
    runtimeTimer = null;
  }
  if (deviceTimer !== null) {
    clearInterval(deviceTimer);
    deviceTimer = null;
  }
});
</script>

<template>
  <div class="window-frame">
    <header class="titlebar" @mousedown="startWindowDrag">
      <div class="titlebar-left">
        <button class="menu-button" type="button" aria-label="菜单" @mousedown.stop>
          <Menu :size="21" :stroke-width="2.1" />
        </button>
        <h1>网络管家 · DDNS 与端口转发</h1>
        <span
          v-if="updateStatusMessage"
          class="update-status"
          :class="`update-${updateStatusType}`"
        >
          {{ updateStatusMessage }}
        </span>
      </div>

      <div class="window-controls">
        <button
          class="window-control control-minimize"
          type="button"
          aria-label="最小化"
          @click="minimizeWindow"
          @mousedown.stop
        ></button>
        <button
          class="window-control control-maximize"
          type="button"
          aria-label="最大化"
          @click="maximizeWindow"
          @mousedown.stop
        ></button>
        <button
          class="window-control control-close"
          type="button"
          aria-label="关闭"
          @click="closeWindow"
          @mousedown.stop
        ></button>
      </div>
    </header>

    <main class="main-content">
      <section class="section-cards" aria-label="运行状态">
        <StatusCard
          v-for="card in statusCards"
          :key="card.title"
          :title="card.title"
          :value="card.value"
          :subtitle="card.subtitle"
          :icon="card.icon"
          :status="card.status"
        />
      </section>

      <section class="dashboard-grid" aria-label="网络配置">
        <ForwardRulesPanel />
        <DeviceDdnsPanel class="device-ddns-column" />
        <ReverseProxyPanel />
      </section>

      <section class="bottom-grid" aria-label="日志与设置">
        <LogPanel />
        <RuntimeSettingsPanel
          :uptime="statusData.uptime"
          :version="appVersion"
          :auto-start-enabled="autoStartEnabled"
          :auto-start-saving="autoStartSaving"
          :update-checking="updateChecking"
          @toggle-autostart="toggleAutoStart"
          @check-update="installAppUpdate"
        />
      </section>
    </main>
  </div>
</template>

<style>
*,
*::before,
*::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

:root {
  --design-width: 1600px;
  --design-height: 1000px;
  --ui-scale: 1;
  --frame-x: 0px;
  --frame-y: 0px;
  --color-window-bg: #f7f9fc;
  --color-card-bg: #ffffff;
  --color-primary: #1769f6;
  --color-primary-hover: #0d58dd;
  --color-primary-soft: #eaf2ff;
  --color-text-primary: #101522;
  --color-text-secondary: #4e5969;
  --color-text-muted: #7b8495;
  --color-border: #dfe6ef;
  --color-border-strong: #cbd6e5;
  --color-input-bg: #ffffff;
  --color-success: #12a150;
  --color-success-soft: #e9f8ef;
  --color-warning: #d97706;
  --color-warning-soft: #fff6df;
  --color-error: #dc2626;
  --shadow-panel: 0 9px 24px rgba(34, 48, 76, 0.06);
  --shadow-soft: 0 2px 8px rgba(28, 42, 66, 0.06);
  --radius-sm: 4px;
  --radius-md: 8px;
  --titlebar-height: 52px;
}

html,
body {
  height: 100%;
  overflow: hidden;
}

body {
  font-family:
    "Microsoft YaHei UI",
    "Microsoft YaHei",
    "Segoe UI",
    -apple-system,
    BlinkMacSystemFont,
    sans-serif;
  font-size: 12px;
  line-height: 1.42;
  color: var(--color-text-primary);
  background: #edf2f8;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

#app {
  height: 100%;
}

button,
input,
select {
  font: inherit;
}

button {
  cursor: pointer;
}

input,
select {
  min-width: 0;
}
</style>

<style scoped>
.window-frame {
  width: var(--design-width);
  height: var(--design-height);
  overflow: hidden;
  transform: translate(var(--frame-x), var(--frame-y)) scale(var(--ui-scale));
  transform-origin: top left;
  border: 1px solid #b9c6d8;
  border-radius: 8px;
  background: var(--color-window-bg);
  box-shadow: 0 18px 55px rgba(24, 42, 72, 0.18);
}

.titlebar {
  height: var(--titlebar-height);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 13px 0 18px;
  border-bottom: 1px solid rgba(222, 229, 239, 0.95);
  background: rgba(255, 255, 255, 0.9);
  backdrop-filter: blur(10px);
}

.titlebar-left {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 13px;
}

.menu-button {
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: #344052;
}

.menu-button:hover {
  background: #eef3f9;
}

.titlebar h1 {
  color: #101522;
  font-size: 15px;
  font-weight: 800;
  letter-spacing: 0;
  white-space: nowrap;
}

.update-status {
  max-width: 450px;
  overflow: hidden;
  color: #667085;
  font-size: 12px;
  font-weight: 700;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.update-success {
  color: #15803d;
}

.update-error {
  color: #b91c1c;
}

.window-controls {
  display: flex;
  align-items: center;
  gap: 22px;
  height: 100%;
}

.window-control {
  position: relative;
  width: 28px;
  height: 28px;
  border: 0;
  border-radius: 4px;
  background: transparent;
}

.window-control:hover {
  background: #edf2f8;
}

.control-close:hover {
  background: #ef4444;
}

.control-minimize::before,
.control-maximize::before,
.control-close::before,
.control-close::after {
  content: "";
  position: absolute;
  display: block;
  background: #111827;
}

.control-minimize::before {
  left: 9px;
  right: 9px;
  top: 15px;
  height: 1.8px;
}

.control-maximize::before {
  left: 9px;
  top: 8px;
  width: 10px;
  height: 10px;
  border: 1.8px solid #111827;
  background: transparent;
}

.control-close::before,
.control-close::after {
  left: 13px;
  top: 7px;
  width: 1.7px;
  height: 15px;
  border-radius: 999px;
}

.control-close::before {
  transform: rotate(45deg);
}

.control-close::after {
  transform: rotate(-45deg);
}

.control-close:hover::before,
.control-close:hover::after {
  background: #ffffff;
}

.main-content {
  height: calc(var(--design-height) - var(--titlebar-height));
  display: grid;
  grid-template-rows: 106px 594px 188px;
  gap: 12px;
  padding: 6px 18px 18px;
  overflow: hidden;
}

.section-cards {
  min-width: 0;
  display: grid;
  grid-template-columns: 1.2fr 1.3fr 1.05fr 0.95fr 0.95fr 1fr;
  gap: 8px;
}

.dashboard-grid {
  min-height: 0;
  display: grid;
  grid-template-columns: 684px minmax(0, 1fr);
  grid-template-rows: 304px 278px;
  gap: 12px;
}

.device-ddns-column {
  grid-column: 2;
  grid-row: 1 / span 2;
}

.bottom-grid {
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 198px;
  gap: 12px;
}
</style>
