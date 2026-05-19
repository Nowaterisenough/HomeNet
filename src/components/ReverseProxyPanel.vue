<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { CirclePlus, Pause, Pencil, Play, Trash2 } from "@lucide/vue";
import type { ReverseProxyRule } from "../types";
import { useDraggableModal } from "../composables/useDraggableModal";

interface ReverseProxyForm extends Omit<ReverseProxyRule, "listen_port" | "backend_port"> {
  listen_port: string;
  backend_port: string;
}

const rules = ref<ReverseProxyRule[]>([]);
const selectedIds = ref<Set<string>>(new Set());
const editor = ref<ReverseProxyForm>(createEmptyRule());
const statusMessage = ref("");
const messageType = ref<"info" | "success" | "error">("info");
const loading = ref(false);
const saving = ref(false);
const issuingCertificate = ref(false);
const mutating = ref(false);
const editorOpen = ref(false);
const { modalStyle, resetModalPosition, startModalDrag } = useDraggableModal();

const selectedCount = computed(() => selectedIds.value.size);
const selectedRule = computed(() => rules.value.find((rule) => selectedIds.value.has(rule.id)));
const editorTitle = computed(() => {
  if (!editor.value.id) return "新增反向代理";
  const index = rules.value.findIndex((rule) => rule.id === editor.value.id);
  return `编辑反向代理（第 ${index >= 0 ? index + 1 : 1} 行）`;
});

function normalizeProtocol(protocol: string): "HTTP" | "HTTPS" {
  return protocol.trim().toUpperCase() === "HTTPS" ? "HTTPS" : "HTTP";
}

function normalizeRule(rule: ReverseProxyRule): ReverseProxyRule {
  const protocol = normalizeProtocol(rule.protocol);
  return {
    ...rule,
    protocol,
    listen_addr: rule.listen_addr || "::",
    listen_port: Number(rule.listen_port) || (protocol === "HTTPS" ? 443 : 80),
    backend_port: Number(rule.backend_port) || (protocol === "HTTPS" ? 443 : 80),
    tls: rule.tls || (protocol === "HTTPS" ? "passthrough" : "off"),
    certificate: rule.certificate || (protocol === "HTTPS" ? "backend" : ""),
    acme_email: rule.acme_email || "",
    acme_dns_provider: rule.acme_dns_provider || "aliyun",
    acme_access_key_id: rule.acme_access_key_id || "",
    acme_access_key_secret: rule.acme_access_key_secret || "",
    acme_dns_domain: rule.acme_dns_domain || "",
    acme_directory_url:
      rule.acme_directory_url || "https://acme-v02.api.letsencrypt.org/directory",
    certificate_path: rule.certificate_path || "",
    private_key_path: rule.private_key_path || "",
    certificate_expires_at: rule.certificate_expires_at || "",
    certificate_last_issued_at: rule.certificate_last_issued_at || "",
    certificate_last_error: rule.certificate_last_error || "",
    status: rule.status || (rule.enabled ? "正常" : "已禁用"),
  };
}

function toForm(rule: ReverseProxyRule): ReverseProxyForm {
  const normalized = normalizeRule(rule);
  return {
    ...normalized,
    listen_port: String(normalized.listen_port || ""),
    backend_port: String(normalized.backend_port || ""),
  };
}

function createEmptyRule(): ReverseProxyForm {
  return {
    id: "",
    enabled: true,
    protocol: "HTTP",
    domain: "",
    listen_addr: "::",
    listen_port: "80",
    backend_ip: "",
    backend_port: "80",
    tls: "off",
    certificate: "",
    acme_email: "",
    acme_dns_provider: "aliyun",
    acme_access_key_id: "",
    acme_access_key_secret: "",
    acme_dns_domain: "",
    acme_directory_url: "https://acme-v02.api.letsencrypt.org/directory",
    certificate_path: "",
    private_key_path: "",
    certificate_expires_at: "",
    certificate_last_issued_at: "",
    certificate_last_error: "",
    remark: "",
    status: "正常",
  };
}

function parsePort(value: string): number | null {
  const port = Number(value.trim());
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
  return port;
}

function setRules(nextRules: ReverseProxyRule[]) {
  const normalized = nextRules.map(normalizeRule);
  const previousSelected = Array.from(selectedIds.value).find((id) =>
    normalized.some((rule) => rule.id === id),
  );
  const selected = normalized.find((rule) => rule.id === previousSelected) ?? normalized[0];

  rules.value = normalized;
  selectedIds.value = selected ? new Set([selected.id]) : new Set();
  editor.value = selected ? toForm(selected) : createEmptyRule();
}

async function loadRules() {
  loading.value = true;
  try {
    const data = await invoke<ReverseProxyRule[]>("list_reverse_proxy_rules");
    setRules(data);
    statusMessage.value = "";
  } catch (e) {
    rules.value = [];
    selectedIds.value = new Set();
    editor.value = createEmptyRule();
    statusMessage.value = `读取反向代理规则失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    loading.value = false;
  }
}

function notifyDataChanged() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
}

function buildPayload(): ReverseProxyRule | null {
  const listenPort = parsePort(editor.value.listen_port);
  const backendPort = parsePort(editor.value.backend_port);
  if (!editor.value.domain.trim()) {
    statusMessage.value = "请填写外部域名";
    messageType.value = "error";
    return null;
  }
  if (!editor.value.backend_ip.trim()) {
    statusMessage.value = "请填写后端地址";
    messageType.value = "error";
    return null;
  }
  if (listenPort === null || backendPort === null) {
    statusMessage.value = "端口需为 1-65535 的数字";
    messageType.value = "error";
    return null;
  }

  const protocol = normalizeProtocol(editor.value.protocol);
  return {
    id: editor.value.id,
    enabled: editor.value.enabled,
    protocol,
    domain: editor.value.domain.trim(),
    listen_addr: editor.value.listen_addr.trim() || "::",
    listen_port: listenPort,
    backend_ip: editor.value.backend_ip.trim(),
    backend_port: backendPort,
    tls: editor.value.tls || (protocol === "HTTPS" ? "passthrough" : "off"),
    certificate: editor.value.certificate.trim(),
    acme_email: editor.value.acme_email.trim(),
    acme_dns_provider: editor.value.acme_dns_provider.trim() || "aliyun",
    acme_access_key_id: editor.value.acme_access_key_id.trim(),
    acme_access_key_secret: editor.value.acme_access_key_secret.trim(),
    acme_dns_domain: editor.value.acme_dns_domain.trim(),
    acme_directory_url:
      editor.value.acme_directory_url.trim() ||
      "https://acme-v02.api.letsencrypt.org/directory",
    certificate_path: editor.value.certificate_path.trim(),
    private_key_path: editor.value.private_key_path.trim(),
    certificate_expires_at: editor.value.certificate_expires_at,
    certificate_last_issued_at: editor.value.certificate_last_issued_at,
    certificate_last_error: editor.value.certificate_last_error,
    remark: editor.value.remark.trim(),
    status: editor.value.status || "正常",
  };
}

function applySavedRule(rule: ReverseProxyRule) {
  const saved = normalizeRule(rule);
  const next = rules.value.slice();
  const index = next.findIndex((item) => item.id === saved.id);
  if (index >= 0) next[index] = saved;
  else next.push(saved);
  rules.value = next;
  selectedIds.value = new Set([saved.id]);
  editor.value = toForm(saved);
}

async function saveRule() {
  const payload = buildPayload();
  if (!payload) return;

  saving.value = true;
  try {
    const saved = await invoke<ReverseProxyRule>("save_reverse_proxy_rule", { rule: payload });
    applySavedRule(saved);
    statusMessage.value = "反向代理规则已保存";
    messageType.value = "success";
    editorOpen.value = false;
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `保存反向代理规则失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    saving.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

async function issueCertificate() {
  let payload = buildPayload();
  if (!payload) return;
  if (payload.tls !== "auto") {
    statusMessage.value = "请选择 TLS 自动证书后再申请";
    messageType.value = "error";
    return;
  }

  issuingCertificate.value = true;
  try {
    payload = await invoke<ReverseProxyRule>("save_reverse_proxy_rule", { rule: payload });
    applySavedRule(payload);
    const issued = await invoke<ReverseProxyRule>("issue_reverse_proxy_certificate", {
      ruleId: payload.id,
    });
    applySavedRule(issued);
    statusMessage.value = "自动证书已申请或续期";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `自动证书申请失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    issuingCertificate.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 5200);
  }
}

function startEdit(rule?: ReverseProxyRule) {
  editor.value = rule ? toForm(rule) : createEmptyRule();
  selectedIds.value = rule?.id ? new Set([rule.id]) : new Set();
  resetModalPosition();
  editorOpen.value = true;
}

function cancelEdit() {
  editorOpen.value = false;
  editor.value = selectedRule.value ? toForm(selectedRule.value) : createEmptyRule();
}

function toggleSelectRule(rule: ReverseProxyRule, checked: boolean) {
  const next = new Set(selectedIds.value);
  if (checked) {
    next.add(rule.id);
    editor.value = toForm(rule);
  } else {
    next.delete(rule.id);
  }
  selectedIds.value = next;
}

async function deleteRule(id: string) {
  mutating.value = true;
  try {
    await invoke("delete_reverse_proxy_rule", { ruleId: id });
    setRules(rules.value.filter((rule) => rule.id !== id));
    statusMessage.value = "反向代理规则已删除";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `删除反向代理规则失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    mutating.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

async function setRuleEnabled(rule: ReverseProxyRule, enabled: boolean) {
  const previous = rule.enabled;
  rule.enabled = enabled;
  mutating.value = true;
  try {
    await invoke("enable_reverse_proxy_rule", { ruleId: rule.id, enabled });
    notifyDataChanged();
  } catch (e) {
    rule.enabled = previous;
    statusMessage.value = `切换反向代理规则失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    mutating.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

async function batchEnable(enabled: boolean) {
  mutating.value = true;
  try {
    for (const id of Array.from(selectedIds.value)) {
      await invoke("enable_reverse_proxy_rule", { ruleId: id, enabled });
    }
    rules.value = rules.value.map((rule) =>
      selectedIds.value.has(rule.id) ? { ...rule, enabled } : rule,
    );
    statusMessage.value = enabled ? "已启用选中反代" : "已禁用选中反代";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `批量更新反向代理失败：${String(e)}`;
    messageType.value = "error";
    await loadRules();
  } finally {
    mutating.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

async function batchDelete() {
  mutating.value = true;
  try {
    for (const id of Array.from(selectedIds.value)) {
      await invoke("delete_reverse_proxy_rule", { ruleId: id });
    }
    setRules(rules.value.filter((rule) => !selectedIds.value.has(rule.id)));
    statusMessage.value = "已删除选中反代";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `批量删除反向代理失败：${String(e)}`;
    messageType.value = "error";
    await loadRules();
  } finally {
    mutating.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

onMounted(() => {
  loadRules();
});
</script>

<template>
  <section class="panel reverse-panel">
    <header class="panel-header">
      <h2>反向代理配置</h2>
      <div class="toolbar">
        <button class="btn btn-primary" type="button" @click="startEdit()">
          <CirclePlus :size="13" :stroke-width="2.2" />
          新增代理
        </button>
        <button
          class="btn btn-secondary"
          type="button"
          :disabled="selectedCount === 0 || loading || mutating"
          @click="batchEnable(true)"
        >
          <Play :size="13" :stroke-width="2.2" />
          启用
        </button>
        <button
          class="btn btn-secondary"
          type="button"
          :disabled="selectedCount === 0 || loading || mutating"
          @click="batchEnable(false)"
        >
          <Pause :size="13" :stroke-width="2.2" />
          禁用
        </button>
        <button
          class="btn btn-secondary"
          type="button"
          :disabled="selectedCount === 0 || loading || mutating"
          @click="batchDelete"
        >
          <Trash2 :size="13" :stroke-width="2.2" />
          删除
        </button>
      </div>
    </header>

    <p v-if="statusMessage" class="status-message" :class="`msg-${messageType}`">
      {{ statusMessage }}
    </p>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>启用</th>
            <th>协议</th>
            <th>外部域名</th>
            <th>监听地址</th>
            <th>监听端口</th>
            <th>后端地址</th>
            <th>后端端口</th>
            <th>TLS</th>
            <th>证书</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="rules.length === 0">
            <td class="empty-cell" colspan="11">
              {{ loading ? "正在读取反向代理规则" : "暂无反向代理规则" }}
            </td>
          </tr>
          <tr v-for="rule in rules" v-else :key="rule.id">
            <td>
              <label class="toggle-switch">
                <input
                  type="checkbox"
                  :checked="rule.enabled"
                  :disabled="mutating"
                  @change="setRuleEnabled(rule, ($event.target as HTMLInputElement).checked)"
                />
                <span class="toggle-slider"></span>
              </label>
            </td>
            <td>{{ rule.protocol }}</td>
            <td>{{ rule.domain }}</td>
            <td>{{ rule.listen_addr }}</td>
            <td>{{ rule.listen_port }}</td>
            <td>{{ rule.backend_ip }}</td>
            <td>{{ rule.backend_port }}</td>
            <td>{{ rule.tls }}</td>
            <td>{{ rule.certificate || "-" }}</td>
            <td>
              <span class="status-pill">{{ rule.status || "-" }}</span>
            </td>
            <td>
              <div class="row-actions">
                <input
                  class="row-check"
                  type="checkbox"
                  :checked="selectedIds.has(rule.id)"
                  @change="toggleSelectRule(rule, ($event.target as HTMLInputElement).checked)"
                />
                <button class="icon-action" type="button" title="编辑" @click="startEdit(rule)">
                  <Pencil :size="13" :stroke-width="2.1" />
                </button>
                <button class="icon-action" type="button" title="删除" @click="deleteRule(rule.id)">
                  <Trash2 :size="13" :stroke-width="2.1" />
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="editorOpen" class="modal-backdrop" @click.self="cancelEdit">
      <section class="modal-dialog" :style="modalStyle">
        <header class="modal-header draggable-header" @pointerdown="startModalDrag">
          <h3>{{ editorTitle }}</h3>
          <button class="btn btn-secondary" type="button" @pointerdown.stop @click="cancelEdit">关闭</button>
        </header>
        <div class="editor-section">
          <div class="editor-grid">
            <label>
              <span>协议</span>
              <select v-model="editor.protocol">
                <option>HTTP</option>
                <option>HTTPS</option>
              </select>
            </label>
            <label>
              <span>外部域名</span>
              <input v-model="editor.domain" type="text" />
            </label>
            <label>
              <span>监听地址</span>
              <input v-model="editor.listen_addr" type="text" />
            </label>
            <label>
              <span>监听端口</span>
              <input v-model.trim="editor.listen_port" type="text" inputmode="numeric" />
            </label>
            <label>
              <span>后端地址</span>
              <input v-model="editor.backend_ip" type="text" />
            </label>
            <label>
              <span>后端端口</span>
              <input v-model.trim="editor.backend_port" type="text" inputmode="numeric" />
            </label>
            <label class="wide-field">
              <span>证书配置</span>
              <input
                v-model="editor.certificate"
                type="text"
                placeholder="后端服务提供证书，或填写证书路径/备注"
              />
            </label>
            <template v-if="editor.tls === 'auto'">
              <label>
                <span>ACME 邮箱</span>
                <input v-model="editor.acme_email" type="email" />
              </label>
              <label>
                <span>DNS 主域名</span>
                <input v-model="editor.acme_dns_domain" type="text" placeholder="example.com" />
              </label>
              <label>
                <span>DNS 服务商</span>
                <select v-model="editor.acme_dns_provider">
                  <option value="aliyun">阿里云</option>
                </select>
              </label>
              <label>
                <span>AccessKey ID</span>
                <input v-model="editor.acme_access_key_id" type="text" />
              </label>
              <label>
                <span>AccessKey Secret</span>
                <input v-model="editor.acme_access_key_secret" type="password" />
              </label>
              <label>
                <span>到期时间</span>
                <input :value="editor.certificate_expires_at || '未申请'" type="text" readonly />
              </label>
              <label class="wide-field">
                <span>ACME 地址</span>
                <input v-model="editor.acme_directory_url" type="text" />
              </label>
              <label v-if="editor.certificate_last_error" class="wide-field">
                <span>证书错误</span>
                <input :value="editor.certificate_last_error" type="text" readonly />
              </label>
            </template>
            <template v-if="editor.tls === 'manual'">
              <label class="wide-field">
                <span>证书文件</span>
                <input v-model="editor.certificate_path" type="text" placeholder="fullchain.pem" />
              </label>
              <label class="wide-field">
                <span>私钥文件</span>
                <input v-model="editor.private_key_path" type="text" placeholder="private-key.pem" />
              </label>
            </template>
          </div>
          <div class="editor-footer">
            <label>
              <span>TLS</span>
              <select v-model="editor.tls">
                <option value="off">关闭</option>
                <option value="passthrough">HTTPS 透传</option>
                <option value="auto">自动证书</option>
                <option value="manual">手动证书</option>
              </select>
            </label>
            <label class="remark-field">
              <span>备注</span>
              <input v-model="editor.remark" type="text" />
            </label>
            <button
              class="btn btn-secondary"
              type="button"
              :disabled="saving || issuingCertificate || editor.tls !== 'auto'"
              @click="issueCertificate"
            >
              申请/续期
            </button>
            <button class="btn btn-primary save-button" type="button" :disabled="saving" @click="saveRule">
              保存代理
            </button>
            <button class="btn btn-secondary" type="button" :disabled="saving || issuingCertificate" @click="cancelEdit">
              取消
            </button>
          </div>
        </div>
      </section>
    </div>

    <footer class="panel-footer">
      <span class="info-dot">i</span>
      HTTP 按 Host 转发；HTTPS 可 SNI 透传，也可用自动/手动证书在本机终止 TLS 后转发。
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
  justify-content: space-between;
  gap: 12px;
  padding: 0 14px 0 18px;
  border-bottom: 1px solid #e6edf5;
}

h2 {
  color: #111827;
  font-size: 13px;
  font-weight: 800;
}

.toolbar,
.row-actions {
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
  cursor: not-allowed;
  opacity: 0.52;
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
  min-width: 840px;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 11px;
}

th,
td {
  height: 23px;
  padding: 0 8px;
  border-bottom: 1px solid #e6edf5;
  color: #111827;
  text-align: left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

th {
  position: sticky;
  top: 0;
  z-index: 2;
  height: 25px;
  color: #182033;
  font-weight: 800;
  background: #fbfcfe;
}

.empty-cell {
  height: 70px;
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
  width: 48px;
  text-align: center;
}

th:nth-child(2),
td:nth-child(2),
th:nth-child(5),
td:nth-child(5),
th:nth-child(7),
td:nth-child(7),
th:nth-child(8),
td:nth-child(8),
th:nth-child(10),
td:nth-child(10) {
  width: 72px;
}

th:nth-child(11),
td:nth-child(11) {
  width: 76px;
}

.toggle-switch {
  position: relative;
  display: inline-block;
  width: 26px;
  height: 15px;
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
}

.toggle-slider::before {
  content: "";
  position: absolute;
  left: 2px;
  top: 2px;
  width: 11px;
  height: 11px;
  border-radius: 50%;
  background: #ffffff;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.2);
  transition: transform 0.15s ease;
}

.toggle-switch input:checked + .toggle-slider {
  background: var(--color-primary);
}

.toggle-switch input:checked + .toggle-slider::before {
  transform: translateX(11px);
}

.status-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 34px;
  height: 18px;
  border-radius: 4px;
  color: #0e9f4f;
  background: #eaf8ef;
  font-size: 10px;
  font-weight: 800;
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
  width: min(820px, calc(100vw - 72px));
  max-height: calc(100vh - 72px);
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

.modal-header h3 {
  color: #111827;
  font-size: 13px;
  font-weight: 800;
}

.draggable-header {
  cursor: move;
}

.editor-section {
  min-height: 0;
  overflow: auto;
  padding: 16px 18px 18px;
}

.editor-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 14px;
}

.wide-field {
  grid-column: 1 / -1;
}

.editor-footer {
  display: grid;
  grid-template-columns: minmax(120px, 0.7fr) minmax(0, 1.2fr) 86px 86px 66px;
  gap: 6px 12px;
  align-items: center;
  margin-top: 6px;
}

label {
  min-width: 0;
  display: grid;
  grid-template-columns: 88px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
}

label span {
  color: #303847;
  font-size: 11px;
  font-weight: 700;
  white-space: nowrap;
}

input,
select {
  width: 100%;
  height: 24px;
  border: 1px solid #dae3ee;
  border-radius: 4px;
  background: #ffffff;
  color: #111827;
  padding: 0 8px;
  font-size: 11px;
  outline: none;
}

.save-button {
  justify-self: end;
}

.panel-footer {
  height: 22px;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 18px;
  color: #1769f6;
  font-size: 11px;
  border-top: 1px solid #e6edf5;
  background: #fbfdff;
}

.info-dot {
  width: 13px;
  height: 13px;
  display: inline-grid;
  place-items: center;
  border: 1px solid currentColor;
  border-radius: 50%;
  font-size: 9px;
  font-weight: 800;
}
</style>
