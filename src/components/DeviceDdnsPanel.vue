<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Check, RefreshCw, Router, Save, ShieldCheck, Zap } from "@lucide/vue";
import type { DeviceDdnsConfig, LanDevice } from "../types";

const defaultConfig: DeviceDdnsConfig = {
  enabled: false,
  provider: "aliyun",
  access_key_id: "",
  access_key_secret: "",
  domain: "",
  sub_domain: "",
  ttl: 600,
  interval_minutes: 10,
  device_id: "",
  device_mac: "",
  device_name: "",
  selected_ipv6: "",
  last_update_time: "",
  last_result: "",
};

const devices = ref<LanDevice[]>([]);
const config = ref<DeviceDdnsConfig>({ ...defaultConfig });
const selectedIpv6 = ref("");
const currentRecord = ref("");
const loadingDevices = ref(false);
const loadingConfig = ref(false);
const saving = ref(false);
const updating = ref(false);
const statusMessage = ref("");
const messageType = ref<"info" | "success" | "error">("info");
let statusTimeoutId: ReturnType<typeof window.setTimeout> | null = null;

const busy = computed(() => loadingConfig.value || loadingDevices.value || saving.value || updating.value);

const selectedDevice = computed(() =>
  devices.value.find(
    (device) =>
      device.id === config.value.device_id ||
      (!!config.value.device_mac && device.mac === config.value.device_mac),
  ),
);

const bindableDeviceCount = computed(
  () => devices.value.filter((device) => device.global_ipv6.length > 0).length,
);

const deviceSummary = computed(() => {
  if (loadingDevices.value) return "正在发现局域网设备";
  return `${devices.value.length} 台设备，${bindableDeviceCount.value} 台可绑定公网 IPv6`;
});

const footerText = computed(() => {
  if (currentRecord.value) return `当前解析：${currentRecord.value}`;
  if (config.value.last_result) return config.value.last_result;
  if (config.value.last_update_time) return `最近更新：${config.value.last_update_time}`;
  return "暂无设备 DDNS 更新记录";
});

function normalizeConfig(data: Partial<DeviceDdnsConfig> | null | undefined): DeviceDdnsConfig {
  return {
    ...defaultConfig,
    ...data,
    provider: data?.provider || defaultConfig.provider,
    ttl: Number(data?.ttl) || defaultConfig.ttl,
    interval_minutes: Number(data?.interval_minutes) || defaultConfig.interval_minutes,
  };
}

function primaryIpv6(device: LanDevice): string {
  return device.global_ipv6[0] || "";
}

function deviceTitle(device: LanDevice): string {
  return device.hostname || device.display_name || device.mac || device.ipv4[0] || "未知设备";
}

function deviceSubtitle(device: LanDevice): string {
  const ipv4 = device.ipv4[0] || "无 IPv4";
  const ipv6 = device.global_ipv6[0] || "无公网 IPv6";
  return `${ipv4} / ${ipv6}`;
}

function isSelected(device: LanDevice): boolean {
  return selectedDevice.value?.id === device.id;
}

function hasConfiguredDevice(): boolean {
  return Boolean(config.value.device_id.trim() || config.value.device_mac.trim());
}

function notifyLogsRefresh() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
}

function clearStatusTimeout() {
  if (statusTimeoutId !== null) {
    window.clearTimeout(statusTimeoutId);
    statusTimeoutId = null;
  }
}

function scheduleSuccessClear(message: string) {
  clearStatusTimeout();
  statusTimeoutId = window.setTimeout(() => {
    if (messageType.value === "success" && statusMessage.value === message) {
      statusMessage.value = "";
    }
    statusTimeoutId = null;
  }, 5000);
}

function setStatus(message: string, type: "info" | "success" | "error") {
  clearStatusTimeout();
  statusMessage.value = message;
  messageType.value = type;
  if (type === "success") {
    scheduleSuccessClear(message);
  }
}

function reconcileSelectedIpv6() {
  const matched = selectedDevice.value;
  if (matched) {
    if (matched.global_ipv6.includes(selectedIpv6.value)) {
      config.value.selected_ipv6 = selectedIpv6.value;
      return;
    }

    selectedIpv6.value = matched.global_ipv6.includes(config.value.selected_ipv6)
      ? config.value.selected_ipv6
      : primaryIpv6(matched);
    config.value.selected_ipv6 = selectedIpv6.value;
    return;
  }

  if (!hasConfiguredDevice()) {
    selectedIpv6.value = "";
    config.value.selected_ipv6 = "";
  }
}

function selectDevice(device: LanDevice) {
  if (busy.value || device.global_ipv6.length === 0) return;

  config.value.device_id = device.id;
  config.value.device_mac = device.mac;
  config.value.device_name = deviceTitle(device);
  config.value.selected_ipv6 = primaryIpv6(device);
  reconcileSelectedIpv6();
  statusMessage.value = "";
}

function validateConfig(requireEnabled: boolean): string {
  if (requireEnabled && !config.value.enabled) {
    return "设备 DDNS 未启用";
  }
  if (!config.value.access_key_id.trim() || !config.value.access_key_secret.trim()) {
    return "请填写完整的 AccessKey ID 和 Secret";
  }
  if (!config.value.domain.trim() || !config.value.sub_domain.trim()) {
    return "请填写主域名和子域名";
  }
  if (!hasConfiguredDevice()) {
    return "请选择一台有公网 IPv6 的设备";
  }
  if (!selectedIpv6.value.trim()) {
    return "选中设备没有公网 IPv6";
  }
  if (selectedDevice.value && !selectedDevice.value.global_ipv6.includes(selectedIpv6.value)) {
    return "设备 IPv6 已过期，请重新选择";
  }
  return "";
}

async function loadDevices() {
  if (loadingDevices.value || saving.value || updating.value) return;

  loadingDevices.value = true;
  try {
    devices.value = await invoke<LanDevice[]>("list_lan_devices");
    reconcileSelectedIpv6();
  } catch (e: any) {
    devices.value = [];
    setStatus(`设备发现失败：${String(e)}`, "error");
  } finally {
    loadingDevices.value = false;
  }
}

async function loadConfig() {
  if (loadingConfig.value) return;

  loadingConfig.value = true;
  try {
    const data = await invoke<DeviceDdnsConfig>("get_device_ddns_config");
    config.value = normalizeConfig(data);
    selectedIpv6.value = config.value.selected_ipv6;
    reconcileSelectedIpv6();
  } catch (e: any) {
    config.value = { ...defaultConfig };
    setStatus(`加载设备 DDNS 配置失败：${String(e)}`, "error");
  } finally {
    loadingConfig.value = false;
  }
}

async function loadCurrentRecord() {
  const hasAccount = config.value.access_key_id.trim() && config.value.access_key_secret.trim();
  const hasDomain = config.value.domain.trim() && config.value.sub_domain.trim();
  if (!hasAccount || !hasDomain || !hasConfiguredDevice()) {
    currentRecord.value = "";
    return;
  }

  try {
    currentRecord.value = await invoke<string>("get_device_ddns_current_record");
  } catch {
    currentRecord.value = "";
  }
}

async function persistConfig(showSuccess: boolean, allowWhileUpdating = false) {
  if (loadingConfig.value || saving.value || (updating.value && !allowWhileUpdating)) {
    return false;
  }

  reconcileSelectedIpv6();
  const error = config.value.enabled ? validateConfig(false) : "";
  if (error) {
    setStatus(error, "error");
    return false;
  }

  config.value.selected_ipv6 = selectedIpv6.value;
  const payload: DeviceDdnsConfig = { ...config.value };
  saving.value = true;
  try {
    await invoke("save_device_ddns_config", { config: payload });
    notifyLogsRefresh();
    if (showSuccess) {
      setStatus("设备 DDNS 配置已保存", "success");
    }
    return true;
  } catch (e: any) {
    setStatus(`保存失败：${String(e)}`, "error");
    return false;
  } finally {
    saving.value = false;
  }
}

async function saveConfig() {
  await persistConfig(true);
}

async function triggerUpdate() {
  if (loadingConfig.value || loadingDevices.value || saving.value || updating.value) {
    return;
  }

  reconcileSelectedIpv6();
  const error = validateConfig(true);
  if (error) {
    setStatus(error, "error");
    return;
  }

  updating.value = true;
  clearStatusTimeout();
  statusMessage.value = "";
  try {
    const saved = await persistConfig(false, true);
    if (!saved) return;

    const payload: DeviceDdnsConfig = {
      ...config.value,
      selected_ipv6: selectedIpv6.value,
    };
    const result = await invoke<string>("trigger_device_ddns_update", {
      config: payload,
    });
    setStatus(result || "设备 DDNS 更新完成", "success");
    await loadConfig();
    await loadCurrentRecord();
    notifyLogsRefresh();
  } catch (e: any) {
    setStatus(`更新失败：${String(e)}`, "error");
  } finally {
    updating.value = false;
  }
}

async function toggleEnabled(event: Event) {
  if (busy.value) return;

  const input = event.target as HTMLInputElement;
  const previous = config.value.enabled;
  config.value.enabled = input.checked;

  const saved = await persistConfig(false);
  if (!saved) {
    config.value.enabled = previous;
  }
}

onMounted(async () => {
  await loadConfig();
  await loadDevices();
  reconcileSelectedIpv6();
  await loadCurrentRecord();
});

onUnmounted(() => {
  clearStatusTimeout();
});
</script>

<template>
  <section class="panel device-ddns-panel">
    <header class="panel-header">
      <div class="title-block">
        <h2>局域网设备 DDNS</h2>
        <p>{{ deviceSummary }}</p>
      </div>
      <div class="header-actions">
        <label class="toggle-switch" aria-label="启用设备 DDNS">
          <input
            type="checkbox"
            :checked="config.enabled"
            :disabled="busy"
            @change="toggleEnabled"
          />
          <span class="toggle-slider"></span>
        </label>
        <button
          class="icon-button"
          type="button"
          title="刷新设备"
          aria-label="刷新设备"
          :disabled="busy"
          @click="loadDevices"
        >
          <RefreshCw :size="15" :stroke-width="2.2" />
        </button>
      </div>
    </header>

    <p v-if="statusMessage" class="status-message" :class="`msg-${messageType}`">
      {{ statusMessage }}
    </p>

    <div class="device-layout">
      <div class="device-list" role="listbox" aria-label="局域网设备">
        <button
          v-for="device in devices"
          :key="device.id"
          class="device-row"
          :class="{
            selected: isSelected(device),
            disabled: busy || device.global_ipv6.length === 0,
          }"
          type="button"
          role="option"
          :aria-selected="isSelected(device)"
          :disabled="busy || device.global_ipv6.length === 0"
          :title="deviceSubtitle(device)"
          @click="selectDevice(device)"
        >
          <Router class="device-icon" :size="16" :stroke-width="2.2" />
          <span class="device-copy">
            <strong>{{ deviceTitle(device) }}</strong>
            <span>{{ deviceSubtitle(device) }}</span>
          </span>
          <Check v-if="isSelected(device)" class="device-check" :size="15" :stroke-width="2.4" />
        </button>
        <div v-if="devices.length === 0" class="empty-state">
          {{ loadingDevices ? "正在发现设备..." : "未发现局域网设备" }}
        </div>
      </div>

      <div class="device-form">
        <label>
          <span>AccessKey ID</span>
          <input v-model="config.access_key_id" type="text" autocomplete="off" :disabled="busy" />
        </label>
        <label>
          <span>AccessKey Secret</span>
          <input
            v-model="config.access_key_secret"
            type="password"
            autocomplete="off"
            :disabled="busy"
          />
        </label>
        <label>
          <span>主域名</span>
          <input v-model="config.domain" type="text" placeholder="example.com" :disabled="busy" />
        </label>
        <label>
          <span>子域名</span>
          <input v-model="config.sub_domain" type="text" placeholder="nas" :disabled="busy" />
        </label>
        <label>
          <span>设备 IPv6</span>
          <select v-model="selectedIpv6" :disabled="busy || !selectedDevice">
            <option value="">请选择公网 IPv6</option>
            <option v-for="ip in selectedDevice?.global_ipv6 ?? []" :key="ip" :value="ip">
              {{ ip }}
            </option>
          </select>
        </label>
        <label>
          <span>TTL / 间隔</span>
          <div class="split-inputs">
            <input
              v-model.number="config.ttl"
              type="number"
              min="1"
              max="86400"
              title="TTL（秒）"
              :disabled="busy"
            />
            <input
              v-model.number="config.interval_minutes"
              type="number"
              min="1"
              max="1440"
              title="更新间隔（分钟）"
              :disabled="busy"
            />
          </div>
        </label>
      </div>
    </div>

    <footer class="panel-footer">
      <span class="footer-status" :title="footerText">
        <ShieldCheck :size="17" :stroke-width="2.2" />
        <span>{{ footerText }}</span>
      </span>
      <div class="footer-actions">
        <button class="btn btn-secondary" type="button" :disabled="busy" @click="saveConfig">
          <Save :size="14" :stroke-width="2.2" />
          {{ saving ? "保存中..." : "保存" }}
        </button>
        <button class="btn btn-primary" type="button" :disabled="busy" @click="triggerUpdate">
          <Zap :size="14" :stroke-width="2.2" />
          {{ updating ? "更新中..." : "立即更新" }}
        </button>
      </div>
    </footer>
  </section>
</template>

<style scoped>
.panel {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid rgba(217, 225, 237, 0.95);
  border-radius: var(--radius-md, 8px);
  background: rgba(255, 255, 255, 0.94);
  box-shadow: var(--shadow-card);
}

.panel-header {
  flex: 0 0 52px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 14px;
  border-bottom: 1px solid #e1e8f2;
}

.title-block {
  min-width: 0;
}

.panel-header h2 {
  color: #151922;
  font-size: 16px;
  font-weight: 800;
  line-height: 1.2;
}

.panel-header p {
  margin-top: 2px;
  color: #64748b;
  font-size: 12px;
  line-height: 1.2;
}

.header-actions,
.footer-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.icon-button {
  width: 30px;
  height: 30px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid #d7e0eb;
  border-radius: 5px;
  background: #ffffff;
  color: #2563eb;
}

.icon-button:disabled,
.btn:disabled,
.device-form input:disabled,
.device-form select:disabled {
  opacity: 0.56;
  cursor: not-allowed;
}

.icon-button svg,
.btn svg,
.footer-status svg,
.device-row svg {
  flex: 0 0 auto;
  display: block;
}

.icon-button:focus-visible,
.device-row:focus-visible,
.btn:focus-visible,
.device-form input:focus-visible,
.device-form select:focus-visible,
.toggle-switch input:focus-visible + .toggle-slider {
  outline: 2px solid #2563eb;
  outline-offset: 2px;
}

.status-message {
  margin: 8px 12px 0;
  padding: 7px 9px;
  border-radius: 6px;
  font-size: 12px;
}

.msg-info {
  color: #1d4ed8;
  background: #eaf2ff;
}

.msg-success {
  color: #15803d;
  background: #e8f8ee;
}

.msg-error {
  color: #b91c1c;
  background: #fee2e2;
}

.device-layout {
  flex: 1 1 auto;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 10px;
  padding: 10px 12px;
}

.device-list {
  min-height: 0;
  overflow-y: auto;
  display: grid;
  align-content: start;
  gap: 6px;
}

.device-row {
  width: 100%;
  min-height: 46px;
  display: grid;
  grid-template-columns: 20px minmax(0, 1fr) 18px;
  align-items: center;
  gap: 8px;
  padding: 7px 8px;
  border: 1px solid #dbe4ee;
  border-radius: 6px;
  background: #ffffff;
  color: #111827;
  text-align: left;
}

.device-row.selected {
  border-color: #2563eb;
  background: #eef6ff;
}

.device-row.disabled {
  color: #7b8798;
  background: #f8fafc;
  opacity: 0.72;
  cursor: not-allowed;
}

.device-icon {
  color: #2563eb;
}

.device-check {
  color: #15803d;
}

.device-copy {
  min-width: 0;
  display: grid;
  gap: 2px;
}

.device-copy strong,
.device-copy span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.device-copy strong {
  font-size: 12px;
  font-weight: 800;
}

.device-copy span {
  color: #64748b;
  font-size: 11px;
}

.device-form {
  min-width: 0;
  display: grid;
  align-content: start;
  gap: 7px;
}

.device-form label {
  min-width: 0;
  display: grid;
  grid-template-columns: 86px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
}

.device-form span {
  color: #374151;
  font-size: 12px;
  font-weight: 700;
}

.device-form input,
.device-form select {
  min-width: 0;
  width: 100%;
  height: 30px;
  border: 1px solid #d7e0eb;
  border-radius: 5px;
  background: #ffffff;
  color: #202532;
  padding: 0 8px;
  font-size: 12px;
  outline: none;
}

.device-form input:focus,
.device-form select:focus {
  border-color: #78a7f9;
  box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.12);
}

.split-inputs {
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 6px;
}

.empty-state {
  min-height: 80px;
  display: grid;
  place-items: center;
  color: #8a94a6;
  font-size: 12px;
  font-weight: 600;
}

.panel-footer {
  flex: 0 0 42px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 0 12px;
  border-top: 1px solid #e1e8f2;
  color: #64748b;
  font-size: 12px;
}

.footer-status {
  min-width: 0;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  color: #64748b;
}

.footer-status span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.btn {
  height: 30px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 0 10px;
  border-radius: 5px;
  border: 1px solid transparent;
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}

.btn-primary {
  color: #ffffff;
  background: var(--color-primary, #2563eb);
  border-color: var(--color-primary, #2563eb);
}

.btn-secondary {
  color: #374151;
  background: #ffffff;
  border-color: #d7e0eb;
}

.toggle-switch {
  position: relative;
  display: inline-block;
  width: 34px;
  height: 19px;
  flex: 0 0 34px;
}

.toggle-switch input {
  width: 0;
  height: 0;
  opacity: 0;
}

.toggle-slider {
  position: absolute;
  inset: 0;
  border-radius: 999px;
  background: #cbd5e1;
  transition: background 0.15s ease;
}

.toggle-slider::before {
  content: "";
  position: absolute;
  left: 3px;
  top: 3px;
  width: 13px;
  height: 13px;
  border-radius: 50%;
  background: #ffffff;
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.22);
  transition: transform 0.15s ease;
}

.toggle-switch input:checked + .toggle-slider {
  background: var(--color-primary, #2563eb);
}

.toggle-switch input:checked + .toggle-slider::before {
  transform: translateX(15px);
}

@media (max-width: 760px) {
  .device-layout {
    grid-template-columns: 1fr;
  }

  .panel-footer {
    flex-wrap: wrap;
    min-height: 72px;
    padding: 8px 12px;
  }
}
</style>
