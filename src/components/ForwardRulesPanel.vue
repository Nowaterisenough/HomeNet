<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { CirclePlus, Pause, Pencil, Play, Trash2 } from "@lucide/vue";
import type { ForwardRule } from "../types";
import { useDraggableModal } from "../composables/useDraggableModal";

interface ForwardRuleForm extends Omit<ForwardRule, "listen_port" | "target_port"> {
  listen_port: string;
  target_port: string;
}

const IMPLEMENTED_PROTOCOLS = ["TCP", "UDP", "TCP+UDP"] as const;
const rules = ref<ForwardRule[]>([]);
const selectedIds = ref<Set<string>>(new Set());
const editorForm = ref<ForwardRuleForm>(createEmptyRule());
const statusMessage = ref("");
const messageType = ref<"info" | "success" | "error">("info");
const loading = ref(false);
const saving = ref(false);
const mutating = ref(false);
const editorOpen = ref(false);
const { modalStyle, resetModalPosition, startModalDrag } = useDraggableModal();

const selectedCount = computed(() => selectedIds.value.size);
const selectedRule = computed(() => rules.value.find((rule) => selectedIds.value.has(rule.id)));
const editorTitle = computed(() => {
  if (!editorForm.value.id) return "新增转发规则";
  const index = rules.value.findIndex((rule) => rule.id === editorForm.value.id);
  return `编辑转发规则（第 ${index >= 0 ? index + 1 : 1} 行）`;
});

function normalizeProtocol(protocol: string): ForwardRule["protocol"] {
  const normalized = protocol.trim().toUpperCase().replace("＋", "+");
  if (normalized === "UDP") return "UDP";
  if (normalized === "TCP+UDP" || normalized === "UDP+TCP") return "TCP+UDP";
  return "TCP";
}

function normalizeRule(rule: ForwardRule): ForwardRule {
  return {
    ...rule,
    protocol: normalizeProtocol(rule.protocol),
    listen_addr: rule.listen_addr || "::",
    mode: rule.mode || "relay",
    status: rule.status || (rule.enabled ? "正常" : "已禁用"),
  };
}

function toForm(rule: ForwardRule): ForwardRuleForm {
  return {
    ...normalizeRule(rule),
    listen_port: String(rule.listen_port || ""),
    target_port: String(rule.target_port || ""),
  };
}

function createEmptyRule(): ForwardRuleForm {
  return {
    id: "",
    enabled: true,
    protocol: "TCP",
    listen_addr: "::",
    listen_port: "",
    target_ip: "",
    target_port: "",
    mode: "relay",
    remark: "",
    status: "正常",
  };
}

function parsePort(value: string): number | null {
  const port = Number(value.trim());
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
  return port;
}

function setRules(nextRules: ForwardRule[]) {
  const normalized = nextRules.map(normalizeRule);
  const previousSelected = Array.from(selectedIds.value).find((id) =>
    normalized.some((rule) => rule.id === id),
  );
  const selected = normalized.find((rule) => rule.id === previousSelected) ?? normalized[0];

  rules.value = normalized;
  selectedIds.value = selected ? new Set([selected.id]) : new Set();
  editorForm.value = selected ? toForm(selected) : createEmptyRule();
}

async function loadRules() {
  loading.value = true;
  try {
    const data = await invoke<ForwardRule[]>("list_forward_rules");
    setRules(data);
    statusMessage.value = "";
  } catch (e) {
    rules.value = [];
    selectedIds.value = new Set();
    editorForm.value = createEmptyRule();
    statusMessage.value = `读取转发规则失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    loading.value = false;
  }
}

function notifyDataChanged() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
}

function buildRulePayload(): ForwardRule | null {
  const listenPort = parsePort(editorForm.value.listen_port);
  const targetPort = parsePort(editorForm.value.target_port);

  if (!editorForm.value.target_ip.trim()) {
    statusMessage.value = "请填写目标设备 IP";
    messageType.value = "error";
    return null;
  }
  if (listenPort === null || targetPort === null) {
    statusMessage.value = "端口需为 1-65535 的数字";
    messageType.value = "error";
    return null;
  }

  return {
    id: editorForm.value.id,
    enabled: editorForm.value.enabled,
    protocol: normalizeProtocol(editorForm.value.protocol),
    listen_addr: editorForm.value.listen_addr.trim() || "::",
    listen_port: listenPort,
    target_ip: editorForm.value.target_ip.trim(),
    target_port: targetPort,
    mode: editorForm.value.mode.trim() || "relay",
    remark: editorForm.value.remark.trim(),
    status: editorForm.value.status || "正常",
  };
}

function applySavedRule(rule: ForwardRule) {
  const saved = normalizeRule(rule);
  const next = rules.value.slice();
  const index = next.findIndex((item) => item.id === saved.id);
  if (index >= 0) next[index] = saved;
  else next.push(saved);
  rules.value = next;
  selectedIds.value = new Set([saved.id]);
  editorForm.value = toForm(saved);
}

async function saveRule() {
  const payload = buildRulePayload();
  if (!payload) return;

  saving.value = true;
  try {
    const saved = await invoke<ForwardRule>("save_forward_rule", { rule: payload });
    applySavedRule(saved);
    statusMessage.value = "转发规则已保存";
    messageType.value = "success";
    editorOpen.value = false;
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `保存转发规则失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    saving.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

function startEdit(rule?: ForwardRule) {
  editorForm.value = rule ? toForm(rule) : createEmptyRule();
  selectedIds.value = rule?.id ? new Set([rule.id]) : new Set();
  resetModalPosition();
  editorOpen.value = true;
}

function cancelEdit() {
  editorOpen.value = false;
  editorForm.value = selectedRule.value ? toForm(selectedRule.value) : createEmptyRule();
}

function toggleSelectRule(rule: ForwardRule, checked: boolean) {
  const next = new Set(selectedIds.value);
  if (checked) {
    next.add(rule.id);
    editorForm.value = toForm(rule);
  } else {
    next.delete(rule.id);
  }
  selectedIds.value = next;
}

async function deleteRule(id: string) {
  mutating.value = true;
  try {
    await invoke("delete_forward_rule", { ruleId: id });
    setRules(rules.value.filter((rule) => rule.id !== id));
    statusMessage.value = "转发规则已删除";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `删除转发规则失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    mutating.value = false;
    window.setTimeout(() => (statusMessage.value = ""), 4200);
  }
}

async function setRuleEnabled(rule: ForwardRule, enabled: boolean) {
  const previous = rule.enabled;
  rule.enabled = enabled;
  mutating.value = true;
  try {
    await invoke("enable_forward_rule", { ruleId: rule.id, enabled });
    notifyDataChanged();
  } catch (e) {
    rule.enabled = previous;
    statusMessage.value = `切换转发规则失败：${String(e)}`;
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
      await invoke("enable_forward_rule", { ruleId: id, enabled });
    }
    rules.value = rules.value.map((rule) =>
      selectedIds.value.has(rule.id) ? { ...rule, enabled } : rule,
    );
    statusMessage.value = enabled ? "已启用选中规则" : "已禁用选中规则";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `批量更新转发规则失败：${String(e)}`;
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
      await invoke("delete_forward_rule", { ruleId: id });
    }
    setRules(rules.value.filter((rule) => !selectedIds.value.has(rule.id)));
    statusMessage.value = "已删除选中规则";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e) {
    statusMessage.value = `批量删除转发规则失败：${String(e)}`;
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
  <section class="panel rules-panel">
    <header class="panel-header">
      <h2>IPv6/IPv4 转发规则</h2>
      <div class="toolbar">
        <button class="btn btn-primary" type="button" @click="startEdit()">
          <CirclePlus :size="13" :stroke-width="2.2" />
          新增规则
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
      <table class="rules-table">
        <thead>
          <tr>
            <th>启用</th>
            <th>协议</th>
            <th>监听地址</th>
            <th>监听端口</th>
            <th>目标设备 IP</th>
            <th>目标端口</th>
            <th>模式</th>
            <th>备注</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="rules.length === 0">
            <td class="empty-cell" colspan="10">
              {{ loading ? "正在读取转发规则" : "暂无转发规则" }}
            </td>
          </tr>
          <tr
            v-for="rule in rules"
            v-else
            :key="rule.id"
            :class="{ selected: selectedIds.has(rule.id) }"
          >
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
            <td>{{ rule.listen_addr }}</td>
            <td>{{ rule.listen_port }}</td>
            <td>{{ rule.target_ip }}</td>
            <td>{{ rule.target_port }}</td>
            <td>{{ rule.mode }}</td>
            <td>{{ rule.remark || "-" }}</td>
            <td>{{ rule.status || "-" }}</td>
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
              <select v-model="editorForm.protocol">
                <option
                  v-for="protocol in IMPLEMENTED_PROTOCOLS"
                  :key="protocol"
                  :value="protocol"
                >
                  {{ protocol }}
                </option>
              </select>
            </label>
            <label>
              <span>监听地址</span>
              <input v-model="editorForm.listen_addr" type="text" />
            </label>
            <label>
              <span>监听端口</span>
              <input v-model.trim="editorForm.listen_port" type="text" inputmode="numeric" />
            </label>
            <label>
              <span>目标设备 IP</span>
              <input v-model="editorForm.target_ip" type="text" />
            </label>
            <label>
              <span>目标端口</span>
              <input v-model.trim="editorForm.target_port" type="text" inputmode="numeric" />
            </label>
            <label>
              <span>转发模式</span>
              <select v-model="editorForm.mode">
                <option value="relay">普通 TCP/UDP 转发</option>
              </select>
            </label>
            <label>
              <span>备注</span>
              <input v-model="editorForm.remark" type="text" />
            </label>
          </div>
          <div class="editor-actions">
            <button class="btn btn-primary" type="button" :disabled="saving" @click="saveRule">
              保存规则
            </button>
            <button class="btn btn-secondary" type="button" :disabled="saving" @click="cancelEdit">
              取消
            </button>
          </div>
        </div>
      </section>
    </div>
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
  height: 38px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 14px 0 18px;
  border-bottom: 1px solid #e6edf5;
}

.panel-header h2 {
  color: #111827;
  font-size: 13px;
  font-weight: 800;
  white-space: nowrap;
}

.toolbar,
.editor-actions,
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
  min-width: 1080px;
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
  width: 44px;
  text-align: center;
}

th:nth-child(2),
td:nth-child(2),
th:nth-child(4),
td:nth-child(4),
th:nth-child(6),
td:nth-child(6),
th:nth-child(9),
td:nth-child(9) {
  width: 72px;
}

th:nth-child(3),
td:nth-child(3),
th:nth-child(5),
td:nth-child(5) {
  width: 126px;
}

th:nth-child(10),
td:nth-child(10) {
  width: 76px;
}

input[type="checkbox"] {
  width: 13px;
  height: 13px;
  accent-color: var(--color-primary);
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
  width: min(760px, calc(100vw - 72px));
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
  padding: 16px 18px 18px;
}

.editor-grid {
  display: grid;
  grid-template-columns: 96px minmax(150px, 1fr) 96px minmax(170px, 1.15fr);
  gap: 8px 12px;
}

label {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.editor-grid label:nth-child(6) {
  grid-column: 2 / 3;
}

.editor-grid label:nth-child(7) {
  grid-column: 3 / 5;
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
  height: 28px;
  border: 1px solid #dae3ee;
  border-radius: 4px;
  background: #ffffff;
  color: #111827;
  padding: 0 8px;
  font-size: 11px;
  outline: none;
}

.editor-actions {
  justify-content: flex-end;
  margin-top: 9px;
}
</style>
