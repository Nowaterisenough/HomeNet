<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { Check, ChevronDown, Copy, RefreshCw } from "@lucide/vue";
import StatusCard from "./components/StatusCard.vue";
import DdnsPanel from "./components/DdnsPanel.vue";
import DeviceDdnsPanel from "./components/DeviceDdnsPanel.vue";
import ForwardRulesPanel from "./components/ForwardRulesPanel.vue";
import LogPanel from "./components/LogPanel.vue";
import type { AppUpdateResult, NetworkInterfaceInfo, RuntimeStatus } from "./types";

const emptyStatus: RuntimeStatus = {
  public_ipv4: "",
  public_ipv6: "",
  ddns_status: "未启用",
  last_update_time: "暂无",
  rule_count: 0,
  enabled_rule_count: 0,
  uptime: 0,
};

const statusData = ref<RuntimeStatus>({ ...emptyStatus });
const networkInterfaces = ref<NetworkInterfaceInfo[]>([]);
const selectedIpv6Interface = ref("");
const ipv6BindingSaving = ref(false);
const ipv6DropdownOpen = ref(false);
const ipv6SelectRef = ref<HTMLElement | null>(null);
const logSection = ref<HTMLElement | null>(null);
const logsFocused = ref(false);
const copiedPublicIp = ref<"ipv4" | "ipv6" | null>(null);
const autoStartEnabled = ref(false);
const autoStartSaving = ref(false);
const updateChecking = ref(false);
const updateStatusMessage = ref("");
const updateStatusType = ref<"normal" | "success" | "error">("normal");
const appWindow = getCurrentWindow();
const DESIGN_WIDTH = 1586;
const DESIGN_HEIGHT = 992;
const referenceWindowSize = new LogicalSize(DESIGN_WIDTH, DESIGN_HEIGHT);
let runtimeTimer: ReturnType<typeof setInterval> | null = null;
let copiedPublicIpTimer: ReturnType<typeof setTimeout> | null = null;

const selectedInterfaceMissing = computed(
  () =>
    Boolean(selectedIpv6Interface.value) &&
    !networkInterfaces.value.some((item) => item.name === selectedIpv6Interface.value),
);

interface Ipv6InterfaceOption {
  value: string;
  label: string;
  hint: string;
}

const ipv6InterfaceOptions = computed<Ipv6InterfaceOption[]>(() => {
  const options: Ipv6InterfaceOption[] = [
    {
      value: "",
      label: "自动选择",
      hint: "使用第一个可用公网 IPv6",
    },
  ];

  if (selectedInterfaceMissing.value) {
    options.push({
      value: selectedIpv6Interface.value,
      label: selectedIpv6Interface.value,
      hint: "未检测到",
    });
  }

  for (const item of networkInterfaces.value) {
    const ipv6 = preferredIpv6(item);
    options.push({
      value: item.name,
      label: item.name,
      hint: ipv6 || "无 IPv6",
    });
  }

  return options;
});

const selectedIpv6Option = computed(
  () =>
    ipv6InterfaceOptions.value.find((option) => option.value === selectedIpv6Interface.value) ??
    ipv6InterfaceOptions.value[0],
);

const selectedIpv6Label = computed(() => {
  const option = selectedIpv6Option.value;
  if (!option.value) return option.label;
  return `${option.label} - ${option.hint}`;
});

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

async function loadRuntimeStatus() {
  try {
    const data = await invoke<RuntimeStatus>("get_runtime_status");
    statusData.value = {
      ...emptyStatus,
      ...data,
      public_ipv4: data.public_ipv4 || "",
      public_ipv6: data.public_ipv6 || "",
      ddns_status: data.ddns_status || emptyStatus.ddns_status,
      last_update_time: data.last_update_time || emptyStatus.last_update_time,
    };
  } catch {
    statusData.value = {
      ...emptyStatus,
      ddns_status: "未连接",
    };
  }
}

async function loadAutoStart() {
  try {
    autoStartEnabled.value = await invoke<boolean>("get_auto_start");
  } catch {
    autoStartEnabled.value = false;
  }
}

async function loadNetworkInterfaces() {
  try {
    networkInterfaces.value = await invoke<NetworkInterfaceInfo[]>("list_network_interfaces");
  } catch {
    networkInterfaces.value = [];
  }
}

async function loadIpv6Interface() {
  try {
    selectedIpv6Interface.value = await invoke<string>("get_ipv6_interface");
  } catch {
    selectedIpv6Interface.value = "";
  }
}

async function loadIpv6Binding() {
  await Promise.all([loadNetworkInterfaces(), loadIpv6Interface()]);
}

function notifyLogsChanged() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
}

async function toggleAutoStart(event: Event) {
  const input = event.target as HTMLInputElement;
  const previous = autoStartEnabled.value;
  const enabled = input.checked;

  autoStartEnabled.value = enabled;
  autoStartSaving.value = true;
  try {
    await invoke("set_auto_start", { enabled });
    notifyLogsChanged();
  } catch (e) {
    autoStartEnabled.value = previous;
    console.warn("设置开机自启失败:", e);
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
    updateStatusType.value = result.status === "installed" ? "success" : "normal";
    notifyLogsChanged();

    if (result.status !== "installed") {
      updateChecking.value = false;
    }
  } catch (e) {
    updateStatusMessage.value = `更新失败：${String(e)}`;
    updateStatusType.value = "error";
    updateChecking.value = false;
  }
}

function looksLikeGlobalIpv6(value: string): boolean {
  const first = value.trim().split(":")[0] || "";
  return first.startsWith("2") || first.startsWith("3");
}

function preferredIpv6(item: NetworkInterfaceInfo): string {
  return item.ipv6.find(looksLikeGlobalIpv6) || item.ipv6[0] || "";
}

function toggleIpv6Dropdown() {
  if (ipv6BindingSaving.value) return;
  ipv6DropdownOpen.value = !ipv6DropdownOpen.value;
}

async function selectIpv6Interface(interfaceName: string) {
  selectedIpv6Interface.value = interfaceName;
  ipv6DropdownOpen.value = false;
  await saveIpv6Interface();
}

async function saveIpv6Interface() {
  const interfaceName = selectedIpv6Interface.value;
  ipv6BindingSaving.value = true;
  try {
    await invoke("set_ipv6_interface", { interfaceName });
    await loadRuntimeStatus();
    notifyLogsChanged();
  } catch {
    await loadIpv6Interface();
  } finally {
    ipv6BindingSaving.value = false;
  }
}

function displayValue(value: string): string {
  return value.trim() ? value : "--";
}

async function copyPublicIp(kind: "ipv4" | "ipv6", value: string) {
  const text = value.trim();
  if (!text) return;

  try {
    await navigator.clipboard.writeText(text);
  } catch {
    copiedPublicIp.value = null;
    return;
  }
  copiedPublicIp.value = kind;

  if (copiedPublicIpTimer !== null) {
    clearTimeout(copiedPublicIpTimer);
  }
  copiedPublicIpTimer = setTimeout(() => {
    copiedPublicIp.value = null;
    copiedPublicIpTimer = null;
  }, 1200);
}

function ipv6PrefixSubtitle(value: string): string {
  const parts = value.split(":");
  if (!value.trim() || parts.length < 4) {
    return "未获取 IPv6 前缀";
  }
  return `前缀：${parts.slice(0, 4).join(":")}::/64`;
}

function ddnsStatusType(): "normal" | "warning" | "error" | "success" {
  if (statusData.value.ddns_status === "运行中") return "success";
  if (statusData.value.ddns_status === "未连接") return "warning";
  return "normal";
}

function formatUptime(seconds: number): string {
  const totalSeconds = Math.max(0, Math.floor(Number(seconds) || 0));
  const minutes = Math.floor(totalSeconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (totalSeconds < 60) return `${totalSeconds} 秒`;
  if (minutes < 60) return `${minutes} 分钟`;
  if (hours < 24) return `${hours} 小时 ${minutes % 60} 分钟`;
  return `${days} 天 ${hours % 24} 小时`;
}

function handleRuntimeRefresh() {
  void loadRuntimeStatus();
}

function handleFocusLogs() {
  logsFocused.value = true;
  logSection.value?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  window.setTimeout(() => {
    logsFocused.value = false;
  }, 1400);
}

async function minimizeWindow() {
  await appWindow.minimize();
}

async function closeWindow() {
  await appWindow.close();
}

async function startWindowDrag(event: MouseEvent) {
  if (event.button !== 0) return;
  const target = event.target as HTMLElement | null;
  if (target?.closest("button, input, select, textarea, a, label")) return;
  try {
    await appWindow.startDragging();
  } catch (e) {
    console.warn("启动窗口拖动失败:", e);
  }
}

function closeIpv6DropdownOnOutside(event: MouseEvent) {
  if (!ipv6DropdownOpen.value) return;
  const target = event.target as Node | null;
  if (target && ipv6SelectRef.value?.contains(target)) return;
  ipv6DropdownOpen.value = false;
}

async function restoreReferenceWindow() {
  try {
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
  window.addEventListener("homenet:refresh-status", handleRuntimeRefresh);
  window.addEventListener("homenet:focus-logs", handleFocusLogs);
  document.addEventListener("mousedown", closeIpv6DropdownOnOutside);
  restoreReferenceWindow();
  loadRuntimeStatus();
  loadAutoStart();
  loadIpv6Binding();
  runtimeTimer = setInterval(loadRuntimeStatus, 10000);
});

onUnmounted(() => {
  window.removeEventListener("resize", syncUiScale);
  window.removeEventListener("homenet:refresh-status", handleRuntimeRefresh);
  window.removeEventListener("homenet:focus-logs", handleFocusLogs);
  document.removeEventListener("mousedown", closeIpv6DropdownOnOutside);
  if (runtimeTimer !== null) {
    clearInterval(runtimeTimer);
    runtimeTimer = null;
  }
  if (copiedPublicIpTimer !== null) {
    clearTimeout(copiedPublicIpTimer);
    copiedPublicIpTimer = null;
  }
});
</script>

<template>
  <div class="window-frame">
    <header class="titlebar" @mousedown="startWindowDrag">
      <div class="titlebar-left">
        <h1>HomeNet · DDNS 与端口转发</h1>
        <span
          v-if="updateStatusMessage"
          class="update-status"
          :class="`update-${updateStatusType}`"
        >
          {{ updateStatusMessage }}
        </span>
      </div>
      <div class="titlebar-actions">
        <button
          class="update-button"
          type="button"
          title="检查并静默安装更新"
          :disabled="updateChecking"
          @click="installAppUpdate"
          @mousedown.stop
        >
          <RefreshCw
            :class="{ spinning: updateChecking }"
            :size="14"
            :stroke-width="2.4"
          />
          <span>{{ updateChecking ? "更新中" : "检测更新" }}</span>
        </button>
        <label
          class="autostart-option"
          :class="{ saving: autoStartSaving }"
          title="系统启动后自动运行 HomeNet"
          @mousedown.stop
        >
          <input
            class="autostart-checkbox"
            type="checkbox"
            aria-label="开机自启"
            :checked="autoStartEnabled"
            :disabled="autoStartSaving"
            @change="toggleAutoStart"
          />
          <span>开机自启</span>
        </label>
        <div class="window-controls">
          <button
            class="control control-minimize"
            type="button"
            aria-label="最小化"
            @click="minimizeWindow"
          ></button>
          <button
            class="control control-close"
            type="button"
            aria-label="关闭"
            @click="closeWindow"
          ></button>
        </div>
      </div>
    </header>

    <div class="app-shell">
      <main class="main-content">
        <section class="section-cards" aria-label="运行状态">
          <StatusCard
            title="公网 IPv4"
            :value="displayValue(statusData.public_ipv4)"
            subtitle="公网 IPv4 出口地址"
            status="normal"
            icon="globe"
          >
            <template #action>
              <button
                class="copy-public-ip"
                type="button"
                aria-label="复制公网 IPv4"
                title="复制公网 IPv4"
                :disabled="!statusData.public_ipv4"
                @click="copyPublicIp('ipv4', statusData.public_ipv4)"
              >
                <Check
                  v-if="copiedPublicIp === 'ipv4'"
                  :size="14"
                  :stroke-width="2.6"
                />
                <Copy v-else :size="14" :stroke-width="2.4" />
              </button>
            </template>
          </StatusCard>
          <StatusCard
            title="公网 IPv6"
            :value="displayValue(statusData.public_ipv6)"
            :subtitle="ipv6PrefixSubtitle(statusData.public_ipv6)"
            status="normal"
            icon="ipv6"
          >
            <template #action>
              <button
                class="copy-public-ip"
                type="button"
                aria-label="复制公网 IPv6"
                title="复制公网 IPv6"
                :disabled="!statusData.public_ipv6"
                @click="copyPublicIp('ipv6', statusData.public_ipv6)"
              >
                <Check
                  v-if="copiedPublicIp === 'ipv6'"
                  :size="14"
                  :stroke-width="2.6"
                />
                <Copy v-else :size="14" :stroke-width="2.4" />
              </button>
            </template>
            <template #control>
              <div ref="ipv6SelectRef" class="ipv6-bind-control">
                <span class="ipv6-bind-label">绑定网卡</span>
                <div class="ipv6-select" :class="{ open: ipv6DropdownOpen }">
                  <button
                    class="ipv6-select-trigger"
                    type="button"
                    aria-haspopup="listbox"
                    :aria-expanded="ipv6DropdownOpen"
                    :disabled="ipv6BindingSaving"
                    title="IPv6 绑定网卡"
                    @click.stop="toggleIpv6Dropdown"
                    @keydown.escape.stop="ipv6DropdownOpen = false"
                  >
                    <span class="ipv6-select-value">{{ selectedIpv6Label }}</span>
                    <ChevronDown
                      class="ipv6-select-chevron"
                      :size="15"
                      :stroke-width="2.3"
                    />
                  </button>
                  <div
                    v-if="ipv6DropdownOpen"
                    class="ipv6-select-menu"
                    role="listbox"
                    aria-label="IPv6 绑定网卡"
                  >
                    <button
                      v-for="option in ipv6InterfaceOptions"
                      :key="option.value || 'auto'"
                      class="ipv6-select-option"
                      :class="{ selected: option.value === selectedIpv6Interface }"
                      type="button"
                      role="option"
                      :aria-selected="option.value === selectedIpv6Interface"
                      @click.stop="selectIpv6Interface(option.value)"
                    >
                      <span class="ipv6-option-copy">
                        <span class="ipv6-option-label">{{ option.label }}</span>
                        <span class="ipv6-option-hint">{{ option.hint }}</span>
                      </span>
                      <Check
                        v-if="option.value === selectedIpv6Interface"
                        class="ipv6-option-check"
                        :size="15"
                        :stroke-width="2.4"
                      />
                    </button>
                  </div>
                </div>
              </div>
            </template>
          </StatusCard>
          <StatusCard
            title="DDNS 状态"
            :value="statusData.ddns_status"
            :subtitle="`最后更新：${statusData.last_update_time}`"
            :status="ddnsStatusType()"
            icon="shield"
          />
          <StatusCard
            title="转发规则数"
            :value="String(statusData.rule_count)"
            :subtitle="`启用：${statusData.enabled_rule_count}　禁用：${
              statusData.rule_count - statusData.enabled_rule_count
            }`"
            status="normal"
            icon="rules"
          />
          <StatusCard
            title="运行时长"
            :value="formatUptime(statusData.uptime)"
            subtitle="应用已运行"
            status="success"
            icon="uptime"
          />
        </section>

        <section class="section-panels" aria-label="配置面板">
          <div class="ddns-stack">
            <DeviceDdnsPanel />
            <DdnsPanel />
          </div>
          <ForwardRulesPanel />
        </section>

        <section
          ref="logSection"
          class="section-logs"
          :class="{ 'logs-focus': logsFocused }"
          aria-label="最近日志"
        >
          <LogPanel />
        </section>
      </main>
    </div>
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
  --design-width: 1586px;
  --design-height: 992px;
  --ui-scale: 1;
  --frame-x: 0px;
  --frame-y: 0px;
  --color-window-bg: #f4f7fb;
  --color-sidebar-bg: #f8fbff;
  --color-sidebar-active: #e8f1ff;
  --color-card-bg: #ffffff;
  --color-primary: #2563eb;
  --color-primary-hover: #1d4ed8;
  --color-primary-soft: #e7f0ff;
  --color-text-primary: #151922;
  --color-text-secondary: #5f6877;
  --color-text-muted: #8a94a6;
  --color-border: #dde5ef;
  --color-border-strong: #c9d5e5;
  --color-input-bg: #ffffff;
  --color-success: #16a34a;
  --color-success-soft: #e8f8ee;
  --color-warning: #d97706;
  --color-warning-soft: #fff6df;
  --color-error: #dc2626;
  --color-info: #2563eb;
  --shadow-card: 0 8px 22px rgba(20, 35, 66, 0.07);
  --shadow-soft: 0 2px 8px rgba(20, 35, 66, 0.06);
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
  font-size: 14px;
  line-height: 1.45;
  color: var(--color-text-primary);
  background: var(--color-window-bg);
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
</style>

<style scoped>
.window-frame {
  width: var(--design-width);
  height: var(--design-height);
  overflow: hidden;
  transform: translate(var(--frame-x), var(--frame-y)) scale(var(--ui-scale));
  transform-origin: top left;
  background: var(--color-window-bg);
}

.titlebar {
  height: var(--titlebar-height);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 18px;
  border-bottom: 1px solid rgba(204, 214, 226, 0.78);
  background: rgba(255, 255, 255, 0.78);
  backdrop-filter: blur(12px);
}

.titlebar-left {
  display: flex;
  align-items: center;
  gap: 14px;
}

.titlebar h1 {
  font-size: 16px;
  font-weight: 700;
  letter-spacing: 0;
  color: #121722;
}

.update-status {
  max-width: 420px;
  overflow: hidden;
  color: #5f6877;
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

.titlebar-actions {
  display: flex;
  align-items: center;
  gap: 20px;
}

.update-button {
  height: 32px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 0 10px;
  border: 1px solid #c8d8ef;
  border-radius: 6px;
  background: #ffffff;
  color: #1f4f9a;
  font-size: 12px;
  font-weight: 800;
  white-space: nowrap;
}

.update-button:hover:not(:disabled),
.update-button:focus-visible {
  border-color: #8fb5ff;
  outline: none;
  background: #f8fbff;
}

.update-button:disabled {
  cursor: wait;
  opacity: 0.68;
}

.spinning {
  animation: update-spin 0.8s linear infinite;
}

@keyframes update-spin {
  to {
    transform: rotate(360deg);
  }
}

.autostart-option {
  min-height: 32px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 0 2px;
  color: #374151;
  font-size: 13px;
  font-weight: 700;
  user-select: none;
  white-space: nowrap;
  cursor: pointer;
}

.autostart-option.saving {
  cursor: wait;
  opacity: 0.62;
}

.autostart-checkbox {
  width: 15px;
  height: 15px;
  accent-color: var(--color-primary, #2563eb);
  cursor: pointer;
}

.autostart-checkbox:disabled {
  cursor: not-allowed;
}

.window-controls {
  display: flex;
  align-items: center;
  gap: 34px;
  padding-right: 3px;
}

.control {
  position: relative;
  width: 32px;
  height: 32px;
  display: inline-block;
  border: 0;
  background: transparent;
  border-radius: 4px;
}

.control:hover {
  background: rgba(15, 23, 42, 0.06);
}

.control-close:hover {
  background: #ef4444;
}

.control-close:hover::before,
.control-close:hover::after {
  background: #ffffff;
}

.control-minimize::before,
.control-close::before,
.control-close::after {
  content: "";
  position: absolute;
  background: #111827;
}

.control-minimize::before {
  left: 9px;
  right: 9px;
  top: 16px;
  height: 2px;
}

.control-close::before,
.control-close::after {
  left: 15px;
  top: 8px;
  width: 1.8px;
  height: 15px;
  border-radius: 999px;
}

.control-close::before {
  transform: rotate(45deg);
}

.control-close::after {
  transform: rotate(-45deg);
}

.app-shell {
  display: flex;
  height: calc(var(--design-height) - var(--titlebar-height));
  overflow: hidden;
}

.main-content {
  flex: 1;
  min-width: 0;
  padding: 16px 24px 20px;
  overflow: hidden;
  display: grid;
  grid-template-rows: 112px minmax(0, 1fr) 168px;
  gap: 14px;
}

.section-cards {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 14px;
}

.section-panels {
  min-height: 0;
  display: grid;
  grid-template-columns: 430px minmax(0, 1fr);
  gap: 12px;
}

.ddns-stack {
  min-width: 0;
  min-height: 0;
  display: grid;
  grid-template-rows: 368px minmax(0, 1fr);
  gap: 12px;
}

.section-logs {
  min-height: 0;
  border-radius: var(--radius-md, 8px);
  transition:
    box-shadow 0.18s ease,
    outline-color 0.18s ease;
}

.section-logs.logs-focus {
  outline: 2px solid rgba(37, 99, 235, 0.42);
  box-shadow: 0 0 0 4px rgba(37, 99, 235, 0.1);
}

.copy-public-ip {
  width: 26px;
  height: 26px;
  display: grid;
  place-items: center;
  border: 1px solid rgba(191, 208, 232, 0.9);
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.88);
  color: #2563eb;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.08);
  transition:
    border-color 0.15s ease,
    background 0.15s ease,
    color 0.15s ease,
    box-shadow 0.15s ease;
}

.copy-public-ip:hover:not(:disabled),
.copy-public-ip:focus-visible {
  border-color: #8fb5ff;
  outline: none;
  background: #ffffff;
  color: #0f4bcc;
  box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.12);
}

.copy-public-ip:disabled {
  cursor: not-allowed;
  opacity: 0.42;
}

.ipv6-bind-control {
  position: relative;
  min-width: 0;
  display: grid;
  grid-template-columns: 50px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
}

.ipv6-bind-label {
  color: #4f5968;
  font-size: 11px;
  font-weight: 700;
  white-space: nowrap;
}

.ipv6-select {
  position: relative;
  min-width: 0;
}

.ipv6-select-trigger {
  width: 100%;
  min-width: 0;
  height: 28px;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 18px;
  align-items: center;
  gap: 3px;
  padding: 0 6px 0 9px;
  border: 1px solid #bcd2fb;
  border-radius: 7px;
  outline: none;
  background: linear-gradient(180deg, #ffffff 0%, #f6faff 100%);
  color: #111827;
  box-shadow: none;
  transition:
    border-color 0.15s ease,
    box-shadow 0.15s ease,
    background 0.15s ease;
}

.ipv6-select-trigger:hover:not(:disabled) {
  border-color: #8fb5ff;
  background: #ffffff;
}

.ipv6-select.open .ipv6-select-trigger,
.ipv6-select-trigger:focus-visible {
  border-color: #6ea0ff;
  box-shadow: none;
}

.ipv6-select-trigger:disabled {
  cursor: not-allowed;
  opacity: 0.66;
}

.ipv6-select-value {
  min-width: 0;
  overflow: hidden;
  color: #0f172a;
  font-size: 12px;
  font-weight: 650;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ipv6-select-chevron {
  color: #1f2937;
  justify-self: center;
  transition: transform 0.15s ease;
}

.ipv6-select.open .ipv6-select-chevron {
  transform: rotate(180deg);
}

.ipv6-select-menu {
  position: absolute;
  z-index: 50;
  top: calc(100% + 6px);
  left: -58px;
  width: calc(100% + 58px);
  max-height: 234px;
  overflow-y: auto;
  padding: 5px;
  border: 1px solid #bfd0e8;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.98);
  box-shadow: none;
}

.ipv6-select-option {
  width: 100%;
  min-height: 40px;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 20px;
  align-items: center;
  gap: 8px;
  padding: 5px 8px 5px 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: #111827;
  text-align: left;
}

.ipv6-select-option:hover {
  background: #eef5ff;
}

.ipv6-select-option.selected {
  background: #2563eb;
  color: #ffffff;
}

.ipv6-option-copy {
  min-width: 0;
  display: grid;
  gap: 1px;
}

.ipv6-option-label,
.ipv6-option-hint {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ipv6-option-label {
  color: inherit;
  font-size: 12px;
  font-weight: 800;
}

.ipv6-option-hint {
  color: #64748b;
  font-size: 11px;
  font-weight: 600;
}

.ipv6-select-option.selected .ipv6-option-hint {
  color: rgba(255, 255, 255, 0.78);
}

.ipv6-option-check {
  color: currentColor;
  justify-self: center;
}

</style>
