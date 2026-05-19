<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Pencil, RefreshCw, Save, Search, Trash2, UserPlus, Zap } from "@lucide/vue";
import type { DeviceDdnsConfig, LanDevice } from "../types";
import { useDraggableModal } from "../composables/useDraggableModal";

interface DeviceRow {
  id: string;
  name: string;
  nativeName: string;
  ip: string;
  mac: string;
  online: boolean;
  configured: boolean;
  enabled: boolean;
  domain: string;
  lastSync: string;
  selectedIpv6: string;
  raw: LanDevice;
  config: DeviceDdnsConfig | null;
}

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

const devices = ref<LanDevice[]>([]);
const configs = ref<DeviceDdnsConfig[]>([]);
const selectedId = ref("");
const draft = ref<DeviceDdnsConfig | null>(null);
const configDialogOpen = ref(false);
const searchQuery = ref("");
const loading = ref(false);
const mutating = ref(false);
const syncing = ref(false);
const statusMessage = ref("");
const messageType = ref<"info" | "success" | "error">("info");
const { modalStyle, resetModalPosition, startModalDrag } = useDraggableModal();

const rows = computed(() => devices.value.map(mapLanDevice));
const selectedRow = computed(() => rows.value.find((row) => row.id === selectedId.value) ?? null);
const selectedCount = computed(() => (selectedRow.value ? 1 : 0));
const filteredDevices = computed(() => {
  const query = searchQuery.value.trim().toLowerCase();
  if (!query) return rows.value;
  return rows.value.filter((device) =>
    [device.name, device.nativeName, device.ip, device.mac, device.domain, device.selectedIpv6].some(
      (value) => value.toLowerCase().includes(query),
    ),
  );
});
const availableIpOptions = computed(() => {
  const row = selectedRow.value;
  const current = draft.value;
  if (!row || !current) return [];
  if (current.record_type.trim().toUpperCase() === "A") {
    return uniqueStrings(row.raw.ipv4);
  }
  return uniqueStrings(row.raw.global_ipv6.concat(row.raw.ipv6).filter((ip) => ip.includes(":")));
});
const previewRows = computed(() => {
  const current = draft.value;
  if (!selectedRow.value || !current?.domain.trim() || !current.sub_domain.trim()) return [];
  return [[deviceDisplayNameForDraft(), `${current.sub_domain.trim()}.${current.domain.trim()}`]];
});

function uniqueStrings(values: string[]): string[] {
  return values.reduce<string[]>((result, value) => {
    const trimmed = value.trim();
    if (trimmed && !result.includes(trimmed)) result.push(trimmed);
    return result;
  }, []);
}

function displayName(device: LanDevice, index: number): string {
  return device.hostname || device.display_name || `LAN-DEVICE-${index + 1}`;
}

function primaryIp(device: LanDevice): string {
  return device.ipv4[0] || device.global_ipv6[0] || device.ipv6[0] || "-";
}

function firstGlobalIpv6(device: LanDevice): string {
  return device.global_ipv6[0] || device.ipv6.find((ip) => ip.includes(":")) || "";
}

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
    record_type: data?.record_type?.trim().toUpperCase() === "A" ? "A" : "AAAA",
    ttl: Number(data?.ttl) || defaultConfig.ttl,
    interval_minutes: Number(data?.interval_minutes) || defaultConfig.interval_minutes,
    device_id: data?.device_id || "",
    device_mac: data?.device_mac || "",
    device_name: data?.device_name || "",
    selected_ipv6: data?.selected_ipv6 || "",
    selected_ip: data?.selected_ip || data?.selected_ipv6 || "",
    last_update_time: data?.last_update_time || "",
    last_result: data?.last_result || "",
    last_online: data?.last_online ?? defaultConfig.last_online,
  };
}

function configMatchesDevice(config: DeviceDdnsConfig, device: LanDevice): boolean {
  const configId = config.device_id.trim();
  if (configId && configId === device.id) return true;
  const configMac = config.device_mac.trim();
  return !!configMac && !!device.mac && configMac.toLowerCase() === device.mac.toLowerCase();
}

function configForDevice(device: LanDevice): DeviceDdnsConfig | null {
  return configs.value.find((config) => configMatchesDevice(config, device)) ?? null;
}

function configuredDomain(config: DeviceDdnsConfig | null): string {
  if (!config?.domain.trim() || !config.sub_domain.trim()) return "-";
  return `${config.sub_domain.trim()}.${config.domain.trim()}`;
}

function mapLanDevice(device: LanDevice, index: number): DeviceRow {
  const config = configForDevice(device);
  const nativeName = displayName(device, index);
  return {
    id: device.id || device.mac || nativeName,
    name: config?.device_name.trim() || nativeName,
    nativeName,
    ip: primaryIp(device),
    mac: device.mac || "-",
    online: device.online,
    configured: Boolean(config),
    enabled: Boolean(config?.enabled),
    domain: configuredDomain(config),
    lastSync: config?.last_update_time || "-",
    selectedIpv6: config?.selected_ip || config?.selected_ipv6 || firstGlobalIpv6(device) || "-",
    raw: device,
    config,
  };
}

function deviceDisplayNameForDraft(): string {
  const row = selectedRow.value;
  if (!row || !draft.value) return "";
  return draft.value.device_name.trim() || row.nativeName;
}

function buildDraft(row: DeviceRow): DeviceDdnsConfig {
  const existing = row.config;
  const template = existing ?? configs.value[0] ?? defaultConfig;
  return normalizeConfig({
    ...template,
    enabled: existing?.enabled ?? true,
    device_id: row.raw.id,
    device_mac: row.raw.mac,
    device_name: existing?.device_name || row.name || row.nativeName,
    record_type: existing?.record_type || "AAAA",
    selected_ip: existing?.selected_ip || existing?.selected_ipv6 || firstGlobalIpv6(row.raw),
    selected_ipv6: existing?.selected_ipv6 || existing?.selected_ip || firstGlobalIpv6(row.raw),
    last_update_time: existing?.last_update_time || "",
    last_result: existing?.last_result || "",
    last_online: existing?.last_online ?? false,
    sub_domain: existing?.sub_domain || "",
  });
}

function syncDraftWithSelection() {
  const row = selectedRow.value;
  draft.value = row ? buildDraft(row) : null;
}

async function loadConfigs() {
  configs.value = (await invoke<DeviceDdnsConfig[]>("list_device_ddns_configs")).map(normalizeConfig);
}

async function loadDevices() {
  loading.value = true;
  try {
    await loadConfigs();
    devices.value = await invoke<LanDevice[]>("list_lan_devices");
    const current = rows.value.find((row) => row.id === selectedId.value);
    const next = current ?? rows.value.find((row) => row.configured) ?? rows.value[0] ?? null;
    selectedId.value = next?.id ?? "";
    syncDraftWithSelection();
    statusMessage.value = "";
    window.dispatchEvent(new CustomEvent("homenet:devices-refresh"));
  } catch (e) {
    devices.value = [];
    configs.value = [];
    selectedId.value = "";
    draft.value = null;
    statusMessage.value = `读取局域网设备失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    loading.value = false;
  }
}

function selectRow(row: DeviceRow) {
  selectedId.value = row.id;
}

function toggleDevice(row: DeviceRow, checked: boolean) {
  selectedId.value = checked ? row.id : "";
}

function openConfigDialog(row: DeviceRow) {
  selectedId.value = row.id;
  draft.value = buildDraft(row);
  resetModalPosition();
  configDialogOpen.value = true;
}

function closeConfigDialog() {
  configDialogOpen.value = false;
  syncDraftWithSelection();
}

function notifyDataChanged() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
  window.dispatchEvent(new CustomEvent("homenet:devices-refresh"));
}

function validateDraft(requireEnabled: boolean): string {
  const current = draft.value;
  const row = selectedRow.value;
  if (!row || !current) return "请选择要配置的局域网设备";
  if (!row.raw.mac.trim()) return "绑定设备必须有 MAC 地址";
  if (requireEnabled && !current.enabled) return "请先启用同步";
  if (!current.access_key_id.trim() || !current.access_key_secret.trim()) {
    return "请填写完整的 AccessKey ID 和 Secret";
  }
  if (!current.domain.trim() || !current.sub_domain.trim()) {
    return "请填写主域名和子域名";
  }
  if (!["A", "AAAA"].includes(current.record_type.trim().toUpperCase())) {
    return "记录类型仅支持 A 或 AAAA";
  }
  if (current.sub_domain.includes(",")) return "每台设备请填写一个独立子域名";
  return "";
}

function buildPayload(): DeviceDdnsConfig | null {
  const row = selectedRow.value;
  const current = draft.value;
  if (!row || !current) return null;
  const recordType = current.record_type.trim().toUpperCase() === "A" ? "A" : "AAAA";
  const selectedIp =
    current.selected_ip ||
    (recordType === "A" ? row.raw.ipv4[0] : firstGlobalIpv6(row.raw)) ||
    "";
  return {
    ...current,
    provider: current.provider || defaultConfig.provider,
    record_type: recordType,
    ttl: Number(current.ttl) || defaultConfig.ttl,
    interval_minutes: Number(current.interval_minutes) || defaultConfig.interval_minutes,
    device_id: row.raw.id,
    device_mac: row.raw.mac,
    device_name: deviceDisplayNameForDraft(),
    selected_ip: selectedIp,
    selected_ipv6: recordType === "AAAA" ? selectedIp : "",
  };
}

async function saveSelectedConfig(showSuccess = true): Promise<DeviceDdnsConfig | null> {
  const error = validateDraft(false);
  if (error) {
    statusMessage.value = error;
    messageType.value = "error";
    return null;
  }

  const payload = buildPayload();
  if (!payload) return null;

  mutating.value = true;
  try {
    await invoke("save_device_ddns_config", { config: payload });
    await loadConfigs();
    syncDraftWithSelection();
    if (showSuccess) {
      statusMessage.value = "设备 DDNS 绑定已保存";
      messageType.value = "success";
      configDialogOpen.value = false;
    }
    notifyDataChanged();
    return payload;
  } catch (e) {
    statusMessage.value = `保存设备 DDNS 绑定失败：${String(e)}`;
    messageType.value = "error";
    return null;
  } finally {
    mutating.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

async function unbindSelected() {
  const row = selectedRow.value;
  if (!row) return;

  mutating.value = true;
  try {
    await invoke("delete_device_ddns_config", {
      deviceId: row.raw.id,
      deviceMac: row.raw.mac,
    });
    await loadConfigs();
    syncDraftWithSelection();
    statusMessage.value = "设备 DDNS 绑定已解除";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `解除设备 DDNS 绑定失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    mutating.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

async function syncNow() {
  const error = validateDraft(true);
  if (error) {
    statusMessage.value = error;
    messageType.value = "error";
    return;
  }

  syncing.value = true;
  try {
    const payload = await saveSelectedConfig(false);
    if (!payload) return;
    const result = await invoke<string>("trigger_device_ddns_update", { config: payload });
    await loadConfigs();
    syncDraftWithSelection();
    statusMessage.value = result || "设备 DDNS 已同步";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `设备 DDNS 同步失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    syncing.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 5200);
  }
}

watch(selectedRow, syncDraftWithSelection);

onMounted(() => {
  loadDevices();
});
</script>

<template>
  <div class="device-ddns-stack">
    <section class="panel devices-list-panel">
      <header class="panel-header">
        <h2>局域网设备与 DDNS 绑定</h2>
        <div class="toolbar">
          <button class="btn btn-secondary" type="button" :disabled="loading" @click="loadDevices">
            <RefreshCw :class="{ spinning: loading }" :size="13" :stroke-width="2.2" />
            扫描设备
          </button>
          <button class="btn btn-secondary" type="button" :disabled="loading" @click="loadDevices">
            <RefreshCw :size="13" :stroke-width="2.2" />
            刷新
          </button>
          <button class="btn btn-secondary" type="button" :disabled="loading" @click="loadDevices">
            <UserPlus :size="13" :stroke-width="2.2" />
            导入邻居
          </button>
          <label class="search-box">
            <Search :size="13" :stroke-width="2.1" />
            <input v-model="searchQuery" type="search" placeholder="搜索设备名称、IP 或 MAC" />
          </label>
        </div>
      </header>

      <p v-if="statusMessage" class="status-message" :class="`msg-${messageType}`">
        {{ statusMessage }}
      </p>

      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>选择</th>
              <th>设备名称</th>
              <th>IP 地址</th>
              <th>MAC 地址</th>
              <th>在线状态</th>
              <th>DDNS 状态</th>
              <th>域名地址</th>
              <th>可用 IP</th>
              <th>最后同步</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="filteredDevices.length === 0">
              <td class="empty-cell" colspan="10">
                {{ loading ? "正在扫描局域网设备" : "暂无局域网设备" }}
              </td>
            </tr>
            <tr
              v-for="device in filteredDevices"
              v-else
              :key="device.id"
              :class="{ 'selected-row': selectedId === device.id }"
              @click="selectRow(device)"
            >
              <td>
                <input
                  type="checkbox"
                  :checked="selectedId === device.id"
                  :disabled="mutating"
                  @click.stop
                  @change="toggleDevice(device, ($event.target as HTMLInputElement).checked)"
                />
              </td>
              <td :title="device.nativeName">{{ device.name }}</td>
              <td>{{ device.ip }}</td>
              <td>{{ device.mac }}</td>
              <td>
                <span class="state-pill" :class="device.online ? 'pill-online' : 'pill-offline'">
                  {{ device.online ? "在线" : "离线" }}
                </span>
              </td>
              <td>
                <span
                  class="bind-pill"
                  :class="device.configured ? 'pill-bound' : 'pill-unbound'"
                >
                  {{ device.configured ? (device.enabled ? "已启用" : "已配置") : "未配置" }}
                </span>
              </td>
              <td>{{ device.domain }}</td>
              <td>{{ device.selectedIpv6 }}</td>
              <td>{{ device.lastSync }}</td>
              <td>
                <button class="icon-action" type="button" title="编辑 DDNS" @click.stop="openConfigDialog(device)">
                  <Pencil :size="13" :stroke-width="2.1" />
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <footer class="panel-footer">
        <strong>已选择 {{ selectedCount }} 台设备</strong>
        <div class="footer-actions">
          <button
            class="btn btn-secondary"
            type="button"
            :disabled="mutating || !selectedRow?.configured"
            @click="unbindSelected"
          >
            <Trash2 :size="13" :stroke-width="2.1" />
            解除绑定
          </button>
          <button
            class="btn btn-primary"
            type="button"
            :disabled="!selectedRow"
            @click="selectedRow && openConfigDialog(selectedRow)"
          >
            <Pencil :size="13" :stroke-width="2.1" />
            编辑绑定
          </button>
        </div>
      </footer>
    </section>

    <div v-if="configDialogOpen && selectedRow && draft" class="modal-backdrop" @click.self="closeConfigDialog">
      <section class="modal-dialog binding-panel" :style="modalStyle">
        <header class="modal-header draggable-header" @pointerdown="startModalDrag">
          <h2>设备 DDNS 解析配置</h2>
          <button class="btn btn-secondary" type="button" @pointerdown.stop @click="closeConfigDialog">关闭</button>
        </header>

      <div class="binding-layout">
        <div class="form-grid">
          <label>
            <span>DDNS 服务商</span>
            <select v-model="draft.provider">
              <option value="aliyun">阿里云</option>
            </select>
          </label>
          <label>
            <span>设备名称</span>
            <input v-model="draft.device_name" type="text" />
          </label>
          <label>
            <span>AccessKey ID</span>
            <input v-model="draft.access_key_id" type="text" autocomplete="off" />
          </label>
          <label>
            <span>子域名</span>
            <input v-model="draft.sub_domain" type="text" />
          </label>
          <label>
            <span>AccessKey Secret</span>
            <input v-model="draft.access_key_secret" type="password" autocomplete="off" />
          </label>
          <label class="toggle-row">
            <span>启用同步</span>
            <input v-model="draft.enabled" type="checkbox" />
          </label>
          <label>
            <span>主域名</span>
            <input v-model="draft.domain" type="text" />
          </label>
          <label>
            <span>记录类型</span>
            <select v-model="draft.record_type">
              <option value="AAAA">AAAA - IPv6</option>
              <option value="A">A - IPv4</option>
            </select>
          </label>
          <label>
            <span>可用 IP</span>
            <select v-model="draft.selected_ip">
              <option value="">自动选择</option>
              <option v-for="ip in availableIpOptions" :key="ip" :value="ip">{{ ip }}</option>
            </select>
          </label>
          <label>
            <span>最短 TTL</span>
            <input v-model.number="draft.ttl" type="number" min="60" max="86400" />
          </label>
          <label>
            <span>绑定设备</span>
            <input :value="selectedRow.raw.mac || '未获取到 MAC 地址'" type="text" disabled />
          </label>
          <label>
            <span>同步间隔</span>
            <input v-model.number="draft.interval_minutes" type="number" min="1" max="1440" />
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

      <footer class="panel-footer binding-footer">
        <span><span class="info-dot">i</span> 保存后由后台任务按间隔更新，立即同步会调用真实 DDNS 接口。</span>
        <div class="footer-actions">
          <button class="btn btn-primary" type="button" :disabled="mutating" @click="saveSelectedConfig()">
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
    </div>
  </div>
</template>

<style scoped>
.device-ddns-stack {
  min-width: 0;
  min-height: 0;
  display: grid;
  grid-template-rows: minmax(0, 1fr);
  gap: 12px;
}

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
  height: 42px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 14px 0 18px;
  border-bottom: 1px solid #e6edf5;
}

.compact-header {
  height: 36px;
}

h2 {
  color: #111827;
  font-size: 13px;
  font-weight: 800;
  white-space: nowrap;
}

.toolbar,
.footer-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.btn {
  height: 26px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 0 10px;
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
  color: #4b5563;
  background: #ffffff;
  border-color: #d8e1ec;
}

.search-box {
  width: 222px;
  height: 28px;
  display: grid;
  grid-template-columns: 18px minmax(0, 1fr);
  align-items: center;
  gap: 5px;
  padding: 0 9px;
  border: 1px solid #d8e1ec;
  border-radius: 4px;
  background: #ffffff;
  color: #6b7280;
}

.search-box input {
  width: 100%;
  border: 0;
  outline: 0;
  color: #111827;
  font-size: 11px;
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

.table-wrap {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  scrollbar-gutter: stable;
  scrollbar-width: thin;
  scrollbar-color: #b8c7d8 #f3f7fc;
}

table {
  width: 100%;
  min-width: 980px;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 10.5px;
}

th,
td {
  height: 27px;
  padding: 0 7px;
  border-bottom: 1px solid #e6edf5;
  color: #111827;
  text-align: left;
  vertical-align: middle;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

th {
  position: sticky;
  top: 0;
  z-index: 2;
  height: 30px;
  color: #182033;
  font-weight: 800;
  background: #fbfcfe;
}

.selected-row td {
  background: #f1f6ff;
}

.empty-cell {
  height: 82px;
  text-align: center;
  color: #7b8495;
  font-size: 12px;
  font-weight: 700;
}

.table-wrap::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

.table-wrap::-webkit-scrollbar-track {
  background: #f3f7fc;
}

.table-wrap::-webkit-scrollbar-thumb {
  border: 2px solid #f3f7fc;
  border-radius: 999px;
  background: #b8c7d8;
}

th:nth-child(1),
td:nth-child(1) {
  width: 42px;
  text-align: center;
}

th:nth-child(2),
td:nth-child(2) {
  width: 132px;
}

th:nth-child(3),
td:nth-child(3),
th:nth-child(8),
td:nth-child(8) {
  width: 128px;
}

th:nth-child(4),
td:nth-child(4) {
  width: 128px;
}

th:nth-child(5),
td:nth-child(5),
th:nth-child(6),
td:nth-child(6) {
  width: 72px;
}

th:nth-child(9),
td:nth-child(9) {
  width: 116px;
}

th:nth-child(10),
td:nth-child(10) {
  width: 58px;
  text-align: center;
}

.state-pill,
.bind-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 42px;
  height: 18px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 800;
}

.pill-online,
.pill-bound {
  color: #0e9f4f;
  background: #eaf8ef;
}

.pill-offline,
.pill-unbound {
  color: #4b5563;
  background: #f0f3f7;
}

.panel-footer {
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 0 14px 0 18px;
  color: var(--color-primary);
  font-size: 11px;
}

.icon-action {
  width: 18px;
  height: 18px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 0;
  background: transparent;
  color: #4b5563;
}

.modal-backdrop {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: grid;
  place-items: center;
  background: rgba(15, 23, 42, 0.28);
}

.modal-dialog {
  width: min(900px, calc(100vw - 72px));
  max-height: calc(100vh - 96px);
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid rgba(218, 226, 237, 0.95);
  border-radius: var(--radius-md, 8px);
  background: #ffffff;
  box-shadow: 0 22px 70px rgba(15, 23, 42, 0.24);
}

.modal-header {
  height: 42px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 16px 0 18px;
  border-bottom: 1px solid #e6edf5;
  user-select: none;
}

.draggable-header {
  cursor: move;
}

.binding-layout {
  flex: 1 1 auto;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 238px;
  gap: 16px;
  padding: 12px 18px 6px;
}

.form-grid {
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 8px 18px;
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
  width: 13px;
  height: 13px;
  padding: 0;
  accent-color: var(--color-primary);
}

.toggle-row input[type="checkbox"] {
  width: 28px;
  height: 16px;
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

.binding-footer {
  height: 38px;
  color: #596579;
}

.binding-footer > span {
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

.spinning {
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
