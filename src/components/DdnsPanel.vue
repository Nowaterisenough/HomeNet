<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Save, Zap } from "@lucide/vue";
import type { DeviceDdnsConfig } from "../types";

const defaultConfig: DeviceDdnsConfig = {
  enabled: false,
  provider: "aliyun",
  access_key_id: "",
  access_key_secret: "",
  domain: "",
  sub_domain: "",
  record_type: "AAAA",
  ttl: 600,
  interval_minutes: 10,
  device_id: "",
  device_mac: "",
  device_name: "",
  selected_ipv6: "",
  selected_ip: "",
  last_update_time: "",
  last_result: "",
  last_online: false,
};

const config = ref<DeviceDdnsConfig>({ ...defaultConfig });
const subDomain = ref("");
const saving = ref(false);
const syncing = ref(false);
const statusMessage = ref("");
const messageType = ref<"info" | "success" | "error">("info");

const previewRows = computed(() => {
  if (!config.value.domain.trim() || !subDomain.value.trim()) return [];
  const deviceName = config.value.device_name.trim() || "已绑定设备";
  return [[deviceName, `${subDomain.value.trim()}.${config.value.domain.trim()}`]];
});

function normalizeConfig(data: Partial<DeviceDdnsConfig> | null | undefined): DeviceDdnsConfig {
  return {
    ...defaultConfig,
    ...data,
    enabled: data?.enabled ?? defaultConfig.enabled,
    provider: data?.provider || defaultConfig.provider,
    access_key_id: data?.access_key_id || "",
    access_key_secret: data?.access_key_secret || "",
    domain: data?.domain || "",
    sub_domain: data?.sub_domain || "",
    record_type: data?.record_type || defaultConfig.record_type,
    ttl: Number(data?.ttl) || defaultConfig.ttl,
    interval_minutes: Number(data?.interval_minutes) || defaultConfig.interval_minutes,
  };
}

async function loadConfig() {
  try {
    const data = await invoke<DeviceDdnsConfig>("get_device_ddns_config");
    config.value = normalizeConfig(data);
    subDomain.value = config.value.sub_domain;
    statusMessage.value = "";
  } catch (e) {
    config.value = { ...defaultConfig };
    subDomain.value = "";
    statusMessage.value = `读取 DDNS 绑定配置失败：${String(e)}`;
    messageType.value = "error";
  }
}

function notifyDataChanged() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
  window.dispatchEvent(new CustomEvent("homenet:devices-refresh"));
}

function validateConfig(): string {
  if (!config.value.access_key_id.trim() || !config.value.access_key_secret.trim()) {
    return "请填写完整的 AccessKey ID 和 Secret";
  }
  if (!config.value.domain.trim() || !subDomain.value.trim()) {
    return "请填写主域名和子域名";
  }
  if (subDomain.value.includes(",")) {
    return "当前后端按单设备 DDNS 生效，请填写一个子域名";
  }
  return "";
}

function buildPayload(): DeviceDdnsConfig {
  return {
    ...config.value,
    sub_domain: subDomain.value.trim(),
    ttl: Number(config.value.ttl) || defaultConfig.ttl,
    interval_minutes: Number(config.value.interval_minutes) || defaultConfig.interval_minutes,
  };
}

async function saveConfig() {
  const error = validateConfig();
  if (error) {
    statusMessage.value = error;
    messageType.value = "error";
    return;
  }

  saving.value = true;
  const payload = buildPayload();

  try {
    await invoke("save_device_ddns_config", { config: payload });
    config.value = payload;
    statusMessage.value = "DDNS 绑定配置已保存";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `保存 DDNS 绑定配置失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    saving.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

async function syncNow() {
  const error = validateConfig();
  if (error) {
    statusMessage.value = error;
    messageType.value = "error";
    return;
  }

  syncing.value = true;
  try {
    const result = await invoke<string>("trigger_device_ddns_update", {
      config: buildPayload(),
    });
    await loadConfig();
    statusMessage.value = result || "DDNS 同步请求已发送";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `DDNS 同步失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    syncing.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 5200);
  }
}

onMounted(() => {
  loadConfig();
});
</script>

<template>
  <section class="panel binding-panel">
    <header class="panel-header">
      <h2>设备 DDNS 解析配置</h2>
    </header>

    <p v-if="statusMessage" class="status-message" :class="`msg-${messageType}`">
      {{ statusMessage }}
    </p>

    <div class="binding-layout">
      <div class="form-grid">
        <label>
          <span>DDNS 服务商</span>
          <select v-model="config.provider">
            <option value="aliyun">阿里云</option>
          </select>
        </label>
        <label>
          <span>子域名</span>
          <input v-model="subDomain" type="text" />
        </label>
        <label>
          <span>AccessKey ID</span>
          <input v-model="config.access_key_id" type="text" autocomplete="off" />
        </label>
        <label class="toggle-row">
          <span>启用同步</span>
          <input v-model="config.enabled" type="checkbox" />
        </label>
        <label>
          <span>AccessKey Secret</span>
          <input v-model="config.access_key_secret" type="password" autocomplete="off" />
        </label>
        <label>
          <span>记录类型</span>
          <input type="text" value="AAAA" disabled />
        </label>
        <label>
          <span>主域名</span>
          <input v-model="config.domain" type="text" />
        </label>
        <label>
          <span>最短 TTL</span>
          <input v-model.number="config.ttl" type="number" min="60" max="86400" />
        </label>
        <label>
          <span>绑定设备</span>
          <input :value="config.device_name || config.device_mac || '-'" type="text" disabled />
        </label>
        <label>
          <span>同步间隔</span>
          <input v-model.number="config.interval_minutes" type="number" min="1" max="1440" />
        </label>
      </div>

      <aside class="preview-card">
        <h3>当前生效预览</h3>
        <div v-if="previewRows.length === 0" class="preview-empty">暂无可预览解析</div>
        <div v-for="[device, domain] in previewRows" :key="device" class="preview-row">
          <span>{{ device }}</span>
          <strong>→</strong>
          <span>{{ domain }}</span>
        </div>
        <p>共 {{ previewRows.length }} 条绑定</p>
      </aside>
    </div>

    <footer class="panel-footer">
      <span><span class="info-dot">i</span> 保存后由后台任务按间隔更新，立即同步会调用真实 DDNS 接口。</span>
      <div class="footer-actions">
        <button class="btn btn-primary" type="button" :disabled="saving" @click="saveConfig">
          <Save :size="13" :stroke-width="2.2" />
          保存绑定
        </button>
        <button class="btn btn-secondary" type="button" :disabled="syncing" @click="syncNow">
          <Zap :size="13" :stroke-width="2.2" />
          立即同步
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
  border: 1px solid rgba(218, 226, 237, 0.95);
  border-radius: var(--radius-md, 8px);
  background: rgba(255, 255, 255, 0.96);
  box-shadow: var(--shadow-panel);
}

.panel-header {
  height: 36px;
  display: flex;
  align-items: center;
  padding: 0 18px;
  border-bottom: 1px solid #e6edf5;
}

h2 {
  color: #111827;
  font-size: 13px;
  font-weight: 800;
  white-space: nowrap;
}

.status-message {
  margin: 6px 14px 0;
  padding: 5px 8px;
  border-radius: 5px;
  font-size: 11px;
}

.msg-info {
  color: #1769f6;
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

.binding-layout {
  flex: 1 1 auto;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 266px;
  gap: 18px;
  padding: 14px 18px 8px;
}

.form-grid {
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 8px 22px;
  align-content: start;
}

label {
  min-width: 0;
  display: grid;
  grid-template-columns: 86px minmax(0, 1fr);
  align-items: center;
  gap: 10px;
}

label > span {
  color: #303847;
  font-size: 11px;
  font-weight: 700;
  white-space: nowrap;
}

input,
select {
  width: 100%;
  height: 26px;
  border: 1px solid #dae3ee;
  border-radius: 4px;
  background: #ffffff;
  color: #111827;
  padding: 0 8px;
  font-size: 11px;
  outline: none;
}

input:disabled {
  color: #697386;
  background: #f4f7fb;
}

input[type="checkbox"] {
  width: 28px;
  height: 16px;
  accent-color: var(--color-primary);
}

.toggle-row {
  grid-template-columns: 86px minmax(0, 1fr);
}

.preview-card {
  min-height: 0;
  padding: 13px 17px;
  border: 1px solid #e2e9f2;
  border-radius: 7px;
  background: #ffffff;
}

.preview-card h3 {
  margin-bottom: 14px;
  color: #111827;
  font-size: 12px;
  font-weight: 800;
}

.preview-empty,
.preview-row {
  height: 28px;
  color: #596579;
  font-size: 11px;
}

.preview-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 22px minmax(0, 1fr);
  align-items: center;
  gap: 9px;
  color: #111827;
}

.preview-row strong {
  color: #344052;
  text-align: center;
}

.preview-row span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.preview-card p {
  margin-top: 12px;
  color: #596579;
  font-size: 11px;
}

.panel-footer {
  height: 38px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 18px;
  color: #596579;
  font-size: 11px;
}

.panel-footer > span {
  min-width: 0;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  white-space: nowrap;
}

.info-dot {
  width: 13px;
  height: 13px;
  display: inline-grid;
  place-items: center;
  border: 1px solid var(--color-primary);
  border-radius: 50%;
  color: var(--color-primary);
  font-size: 9px;
  font-weight: 800;
}

.footer-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.btn {
  height: 26px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 0 12px;
  border-radius: 4px;
  border: 1px solid transparent;
  font-size: 11px;
  font-weight: 700;
  white-space: nowrap;
}

.btn:disabled {
  cursor: wait;
  opacity: 0.58;
}

.btn-primary {
  color: #ffffff;
  background: var(--color-primary);
  border-color: var(--color-primary);
}

.btn-secondary {
  color: var(--color-primary);
  background: #ffffff;
  border-color: #d8e1ec;
}
</style>
