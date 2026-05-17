<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { CircleCheck, Copy, Eye, EyeOff } from "@lucide/vue";
import type { DdnsConfig, RuntimeStatus } from "../types";

const defaultConfig: DdnsConfig = {
  enabled: false,
  provider: "aliyun",
  access_key_id: "",
  access_key_secret: "",
  domain: "",
  sub_domain: "",
  record_type: "AAAA",
  ttl: 600,
  interval_minutes: 10,
};

const config = ref<DdnsConfig>({ ...defaultConfig });
const currentRecord = ref("");
const lastSuccessTime = ref("");
const showSecret = ref(false);
const saving = ref(false);
const testing = ref(false);
const updating = ref(false);
const statusMessage = ref("");
const messageType = ref<"info" | "success" | "error">("info");

const isConfigured = computed(
  () =>
    Boolean(config.value.access_key_id.trim()) &&
    Boolean(config.value.access_key_secret.trim()) &&
    Boolean(config.value.domain.trim()) &&
    Boolean(config.value.sub_domain.trim()),
);

const fullDomain = computed(() => {
  if (!config.value.domain.trim() || !config.value.sub_domain.trim()) {
    return "未配置域名";
  }
  return `${config.value.sub_domain.trim()}.${config.value.domain.trim()}`;
});

const enabledChip = computed(() => (config.value.enabled ? "已启用" : "未启用"));
const statusChip = computed(() => (config.value.enabled ? "运行中" : "已停止"));
const footerUpdateText = computed(() =>
  lastSuccessTime.value ? `最后成功更新：${lastSuccessTime.value}` : "暂无成功更新记录",
);

function normalizeConfig(data: Partial<DdnsConfig> | null | undefined): DdnsConfig {
  return {
    ...defaultConfig,
    ...data,
    provider: data?.provider || defaultConfig.provider,
    record_type: data?.record_type || defaultConfig.record_type,
    ttl: Number(data?.ttl) || defaultConfig.ttl,
    interval_minutes: Number(data?.interval_minutes) || defaultConfig.interval_minutes,
  };
}

async function loadConfig() {
  try {
    const data = await invoke<DdnsConfig>("get_ddns_config");
    config.value = normalizeConfig(data);
    await Promise.all([loadCurrentRecord(), loadLastUpdateTime()]);
  } catch (e: any) {
    config.value = { ...defaultConfig };
    statusMessage.value = `加载 DDNS 配置失败：${String(e)}`;
    messageType.value = "error";
  }
}

async function loadCurrentRecord() {
  if (!isConfigured.value) {
    currentRecord.value = "";
    return;
  }
  try {
    currentRecord.value = await invoke<string>("get_ddns_current_record");
  } catch {
    currentRecord.value = "";
  }
}

async function loadLastUpdateTime() {
  try {
    const status = await invoke<RuntimeStatus>("get_runtime_status");
    lastSuccessTime.value =
      status.last_update_time && status.last_update_time !== "暂无"
        ? status.last_update_time
        : "";
  } catch {
    lastSuccessTime.value = "";
  }
}

function notifyDataChanged() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
}

function validateConfig(requireEnabled: boolean): string {
  if (requireEnabled && !config.value.enabled) {
    return "DDNS 未启用，请先打开开关";
  }
  if (!config.value.access_key_id.trim() || !config.value.access_key_secret.trim()) {
    return "请填写完整的 AccessKey ID 和 Secret";
  }
  if (!config.value.domain.trim() || !config.value.sub_domain.trim()) {
    return "请填写完整的主域名和子域名";
  }
  return "";
}

async function persistConfig(showSuccess: boolean) {
  saving.value = true;
  try {
    await invoke("save_ddns_config", { config: config.value });
    notifyDataChanged();
    if (showSuccess) {
      statusMessage.value = "DDNS 配置已保存";
      messageType.value = "success";
    }
    return true;
  } catch (e: any) {
    statusMessage.value = `保存失败：${String(e)}`;
    messageType.value = "error";
    return false;
  } finally {
    saving.value = false;
  }
}

async function saveConfig() {
  if (config.value.enabled) {
    const error = validateConfig(false);
    if (error) {
      statusMessage.value = error;
      messageType.value = "error";
      return;
    }
  }
  await persistConfig(true);
  setTimeout(() => (statusMessage.value = ""), 5000);
}

async function testConnection() {
  const error = validateConfig(false);
  if (error) {
    statusMessage.value = error;
    messageType.value = "error";
    return;
  }
  testing.value = true;
  statusMessage.value = "";
  try {
    const result = await invoke<string>("test_ddns_connection", {
      config: config.value,
    });
    statusMessage.value = result || "测试连接成功";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e: any) {
    statusMessage.value = `测试连接失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    testing.value = false;
  }
}

async function triggerUpdate() {
  const error = validateConfig(true);
  if (error) {
    statusMessage.value = error;
    messageType.value = "error";
    return;
  }
  updating.value = true;
  statusMessage.value = "";
  try {
    const saved = await persistConfig(false);
    if (!saved) return;
    const result = await invoke<string>("trigger_ddns_update", {
      config: config.value,
    });
    statusMessage.value = result || "更新请求已发出";
    messageType.value = "success";
    await Promise.all([loadCurrentRecord(), loadLastUpdateTime()]);
    notifyDataChanged();
  } catch (e: any) {
    statusMessage.value = `更新失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    updating.value = false;
  }
}

async function toggleEnabled(event: Event) {
  const input = event.target as HTMLInputElement;
  const previous = config.value.enabled;
  config.value.enabled = input.checked;
  if (config.value.enabled) {
    const error = validateConfig(false);
    if (error) {
      config.value.enabled = previous;
      statusMessage.value = error;
      messageType.value = "error";
      return;
    }
  }
  const saved = await persistConfig(false);
  if (!saved) {
    config.value.enabled = previous;
  }
}

function toggleSecret() {
  showSecret.value = !showSecret.value;
}

async function copyCurrentRecord() {
  if (!currentRecord.value.trim()) {
    statusMessage.value = "暂无可复制的解析值";
    messageType.value = "info";
    return;
  }
  try {
    await navigator.clipboard.writeText(currentRecord.value);
    statusMessage.value = "解析值已复制";
    messageType.value = "success";
  } catch (e: any) {
    statusMessage.value = `复制失败：${String(e)}`;
    messageType.value = "error";
  }
}

function openLogs() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  window.dispatchEvent(new CustomEvent("homenet:focus-logs"));
}

onMounted(() => {
  loadConfig();
});
</script>

<template>
  <section class="panel ddns-panel">
    <header class="panel-header">
      <h2>阿里云 DDNS</h2>
      <div class="panel-chips">
        <span class="chip" :class="config.enabled ? 'chip-success' : 'chip-muted'">
          {{ enabledChip }}
        </span>
        <span class="chip" :class="config.enabled ? 'chip-success' : 'chip-muted'">
          {{ statusChip }}
        </span>
        <label class="toggle-switch" aria-label="启用 DDNS">
          <input
            type="checkbox"
            :checked="config.enabled"
            :disabled="saving"
            @change="toggleEnabled"
          />
          <span class="toggle-slider"></span>
        </label>
      </div>
    </header>

    <p v-if="statusMessage" class="status-message" :class="`msg-${messageType}`">
      {{ statusMessage }}
    </p>

    <div class="form-grid">
      <label class="field-row">
        <span>AccessKey ID</span>
        <input v-model="config.access_key_id" type="text" />
      </label>

      <label class="field-row">
        <span>AccessKey Secret</span>
        <div class="secret-field">
          <input
            v-model="config.access_key_secret"
            :type="showSecret ? 'text' : 'password'"
          />
          <button
            type="button"
            class="icon-button eye-button"
            :aria-label="showSecret ? '隐藏密钥' : '显示密钥'"
            @click="toggleSecret"
          >
            <EyeOff v-if="showSecret" :size="16" :stroke-width="2" />
            <Eye v-else :size="16" :stroke-width="2" />
          </button>
        </div>
      </label>

      <label class="field-row">
        <span>主域名</span>
        <div class="select-like">
          <input v-model="config.domain" type="text" />
          <span class="chevron"></span>
        </div>
      </label>

      <label class="field-row">
        <span>子域名</span>
        <input v-model="config.sub_domain" type="text" />
      </label>

      <label class="field-row">
        <span>记录类型</span>
        <select v-model="config.record_type">
          <option value="A">A - 将域名指向一个 IPv4 地址</option>
          <option value="AAAA">AAAA - 将域名指向一个 IPv6 地址</option>
        </select>
      </label>

      <label class="field-row">
        <span>TTL（秒）</span>
        <input v-model.number="config.ttl" type="number" min="1" max="86400" />
      </label>

      <label class="field-row">
        <span>更新间隔（分钟）</span>
        <input
          v-model.number="config.interval_minutes"
          type="number"
          min="1"
          max="1440"
        />
      </label>

      <label class="field-row">
        <span>当前解析值</span>
        <div class="copy-field">
          <input
            :value="currentRecord || '暂无解析值'"
            type="text"
            readonly
            :title="fullDomain"
          />
          <button
            type="button"
            class="icon-button copy-button"
            aria-label="复制解析值"
            @click="copyCurrentRecord"
          >
            <Copy :size="16" :stroke-width="2" />
          </button>
        </div>
      </label>
    </div>

    <div class="actions">
      <button class="btn btn-primary" type="button" :disabled="updating" @click="triggerUpdate">
        {{ updating ? "更新中..." : "立即更新" }}
      </button>
      <button class="btn btn-secondary" type="button" :disabled="testing" @click="testConnection">
        {{ testing ? "测试中..." : "测试连接" }}
      </button>
      <button class="btn btn-outline-primary" type="button" :disabled="saving" @click="saveConfig">
        {{ saving ? "保存中..." : "保存配置" }}
      </button>
    </div>

    <footer class="panel-footer">
      <span class="footer-status">
        <CircleCheck class="footer-icon footer-icon-success" :size="20" :stroke-width="2.1" />
        {{ footerUpdateText }}
      </span>
      <button type="button" class="link-button" @click="openLogs">查看历史日志</button>
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
  height: 56px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
  border-bottom: 1px solid #e1e8f2;
}

.panel-header h2 {
  font-size: 18px;
  font-weight: 800;
  color: #151922;
}

.panel-chips {
  display: flex;
  align-items: center;
  gap: 8px;
}

.chip {
  height: 21px;
  display: inline-flex;
  align-items: center;
  padding: 0 11px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 700;
}

.chip-success {
  color: #16803c;
  background: #e7f8ed;
  border: 1px solid #bdeccf;
}

.chip-muted {
  color: #64748b;
  background: #f1f5f9;
  border: 1px solid #dbe4ee;
}

.status-message {
  margin: 10px 16px 0;
  padding: 8px 10px;
  border-radius: 6px;
  font-size: 13px;
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

.form-grid {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding: 14px 16px 10px;
}

.field-row {
  display: grid;
  grid-template-columns: 128px minmax(0, 1fr);
  align-items: center;
  gap: 12px;
  min-height: 36px;
  color: #1f2430;
}

.field-row > span {
  font-size: 13px;
  font-weight: 500;
}

input,
select {
  width: 100%;
  height: 36px;
  border: 1px solid #d7e0eb;
  border-radius: 5px;
  background: var(--color-input-bg, #ffffff);
  color: #222936;
  padding: 0 9px;
  font-size: 13px;
  outline: none;
  box-shadow: inset 0 1px 2px rgba(15, 23, 42, 0.03);
  transition:
    border-color 0.15s ease,
    box-shadow 0.15s ease;
}

select {
  appearance: none;
}

input:focus,
select:focus {
  border-color: #78a7f9;
  box-shadow: 0 0 0 3px rgba(37, 99, 235, 0.12);
}

input[readonly] {
  color: #64748b;
  background: #f8fafc;
}

.secret-field,
.copy-field,
.select-like {
  position: relative;
}

.secret-field input,
.copy-field input,
.select-like input {
  padding-right: 42px;
}

.icon-button {
  position: absolute;
  right: 5px;
  top: 50%;
  width: 30px;
  height: 28px;
  border: 0;
  border-left: 1px solid #dce4ee;
  background: transparent;
  transform: translateY(-50%);
}

.icon-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #6b7280;
}

.icon-button svg {
  display: block;
}

.chevron {
  position: absolute;
  right: 13px;
  top: 50%;
  width: 9px;
  height: 9px;
  border-right: 1.8px solid #6b7280;
  border-bottom: 1.8px solid #6b7280;
  transform: translateY(-70%) rotate(45deg);
}

.actions {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 9px;
  padding: 0 16px 11px;
}

.btn {
  height: 38px;
  border-radius: 5px;
  font-size: 13px;
  font-weight: 700;
  border: 1px solid transparent;
  transition:
    background 0.15s ease,
    border-color 0.15s ease,
    color 0.15s ease;
}

.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-primary {
  color: #ffffff;
  background: var(--color-primary, #2563eb);
  border-color: var(--color-primary, #2563eb);
}

.btn-primary:hover:not(:disabled) {
  background: var(--color-primary-hover, #1d4ed8);
}

.btn-secondary {
  color: #1f2937;
  background: #ffffff;
  border-color: #d7e0eb;
}

.btn-outline-primary {
  color: var(--color-primary, #2563eb);
  background: #ffffff;
  border-color: var(--color-primary, #2563eb);
}

.panel-footer {
  min-height: 40px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-top: auto;
  padding: 0 16px;
  border-top: 1px solid #e1e8f2;
  color: #697386;
  font-size: 12px;
}

.footer-status {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.footer-icon {
  flex: 0 0 auto;
  display: block;
}

.footer-icon-success {
  color: #16a34a;
  filter: drop-shadow(0 0 0 #e8f8ee);
}

.link-button {
  border: 0;
  background: transparent;
  color: #2563eb;
  font-weight: 600;
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
</style>
