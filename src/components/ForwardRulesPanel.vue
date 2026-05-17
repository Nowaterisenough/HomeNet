<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { CirclePlus, Info, Pause, Pencil, Play, Trash2 } from "@lucide/vue";
import type { ForwardRule } from "../types";
import { formatPortExpression, pairPortExpressions } from "../utils/ports";

interface ForwardRuleForm extends Omit<ForwardRule, "listen_port" | "target_port"> {
  listen_port: string;
  target_port: string;
}

const rules = ref<ForwardRule[]>([]);
const selectedIds = ref<Set<string>>(new Set());
const showEditor = ref(false);
const editorForm = ref<ForwardRuleForm>(createEmptyRule());
const statusMessage = ref("");
const messageType = ref<"info" | "success" | "error">("info");
const loading = ref(false);
const saving = ref(false);
const mutating = ref(false);

function createEmptyRule(): ForwardRuleForm {
  return {
    id: "",
    enabled: true,
    protocol: "TCP",
    listen_addr: "::",
    listen_port: "",
    target_ip: "",
    target_port: "",
    mode: "nat",
    remark: "",
    status: "正常",
  };
}

function toForm(rule: ForwardRule): ForwardRuleForm {
  return {
    ...rule,
    listen_port: formatPortExpression(rule.listen_port),
    target_port: formatPortExpression(rule.target_port),
  };
}

function normalizeStatus(status: string): string {
  if (!status || status === "姝ｅ父") return "正常";
  return status;
}

function normalizeRule(rule: ForwardRule): ForwardRule {
  return {
    ...rule,
    protocol: rule.protocol.toUpperCase(),
    listen_addr: rule.listen_addr || "::",
    mode: rule.mode || "nat",
    status: normalizeStatus(rule.status),
  };
}

const enabledCount = computed(() => rules.value.filter((r) => r.enabled).length);
const selectedCount = computed(() => selectedIds.value.size);

const allSelected = computed(() => {
  if (rules.value.length === 0) return false;
  return rules.value.every((r) => selectedIds.value.has(r.id));
});

const editorTitle = computed(() => {
  if (!editorForm.value.id) return "新增规则";
  const index = rules.value.findIndex((rule) => rule.id === editorForm.value.id);
  return `编辑规则（第 ${index + 1} 行）`;
});

function toggleSelectAll(checked: boolean) {
  selectedIds.value = checked ? new Set(rules.value.map((r) => r.id)) : new Set();
}

function toggleSelectRule(id: string) {
  const next = new Set(selectedIds.value);
  if (next.has(id)) {
    next.delete(id);
  } else {
    next.add(id);
  }
  selectedIds.value = next;
}

function isSelected(id: string): boolean {
  return selectedIds.value.has(id) || editorForm.value.id === id;
}

async function loadRules() {
  loading.value = true;
  try {
    const data = await invoke<ForwardRule[]>("list_forward_rules");
    rules.value = data.map(normalizeRule);
    selectedIds.value = new Set(
      Array.from(selectedIds.value).filter((id) => rules.value.some((rule) => rule.id === id)),
    );
    if (editorForm.value.id) {
      const current = rules.value.find((rule) => rule.id === editorForm.value.id);
      if (current) {
        editorForm.value = toForm(current);
      }
    }
  } catch (e: any) {
    rules.value = [];
    selectedIds.value = new Set();
    editorForm.value = createEmptyRule();
    showEditor.value = false;
    statusMessage.value = `加载规则失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    loading.value = false;
  }
}

function notifyDataChanged() {
  window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
}

function buildRulePayloads(): ForwardRule[] | null {
  const form = editorForm.value;
  if (!form.target_ip.trim()) {
    statusMessage.value = "请填写目标设备 IP";
    messageType.value = "error";
    return null;
  }
  const portPairs = pairPortExpressions(form.listen_port, form.target_port);
  if (!portPairs.ok) {
    statusMessage.value = portPairs.message;
    messageType.value = "error";
    return null;
  }
  const baseRule = {
    protocol: form.protocol.toUpperCase(),
    listen_addr: form.listen_addr.trim(),
    target_ip: form.target_ip.trim(),
    mode: form.mode,
    remark: form.remark.trim(),
    status: form.status || "正常",
  };

  return portPairs.pairs.map((pair, index) => ({
    ...baseRule,
    id: index === 0 ? form.id : "",
    enabled: form.enabled,
    listen_port: pair.listenPort,
    target_port: pair.targetPort,
  }));
}

async function saveRule() {
  const payloads = buildRulePayloads();
  if (!payloads) return;
  saving.value = true;
  try {
    const savedRules: ForwardRule[] = [];
    for (const payload of payloads) {
      const saved = await invoke<ForwardRule>("save_forward_rule", { rule: payload });
      savedRules.push(saved);
    }
    await loadRules();
    const firstSaved = savedRules[0];
    editorForm.value = toForm(normalizeRule(firstSaved));
    selectedIds.value = new Set(savedRules.map((rule) => rule.id));
    showEditor.value = true;
    statusMessage.value =
      savedRules.length > 1
        ? `已保存 ${savedRules.length} 条转发规则`
        : "转发规则已保存";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e: any) {
    statusMessage.value = `保存失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    saving.value = false;
    setTimeout(() => (statusMessage.value = ""), 5000);
  }
}

async function deleteRule(id: string) {
  mutating.value = true;
  try {
    await invoke("delete_forward_rule", { ruleId: id });
    selectedIds.value = new Set(Array.from(selectedIds.value).filter((item) => item !== id));
    if (editorForm.value.id === id) {
      editorForm.value = createEmptyRule();
      showEditor.value = false;
    }
    await loadRules();
    statusMessage.value = "转发规则已删除";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e: any) {
    statusMessage.value = `删除失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    mutating.value = false;
  }
}

async function toggleRuleEnabled(rule: ForwardRule) {
  const newEnabled = !rule.enabled;
  mutating.value = true;
  try {
    await invoke("enable_forward_rule", { ruleId: rule.id, enabled: newEnabled });
    await loadRules();
    statusMessage.value = newEnabled ? "转发规则已启用" : "转发规则已禁用";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e: any) {
    statusMessage.value = `操作失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    mutating.value = false;
  }
}

async function batchEnable() {
  mutating.value = true;
  try {
    for (const id of selectedIds.value) {
      const rule = rules.value.find((r) => r.id === id);
      if (rule && !rule.enabled) {
        await invoke("enable_forward_rule", { ruleId: rule.id, enabled: true });
      }
    }
    await loadRules();
    statusMessage.value = "已启用选中的转发规则";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e: any) {
    statusMessage.value = `批量启用失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    mutating.value = false;
  }
}

async function batchDisable() {
  mutating.value = true;
  try {
    for (const id of selectedIds.value) {
      const rule = rules.value.find((r) => r.id === id);
      if (rule && rule.enabled) {
        await invoke("enable_forward_rule", { ruleId: rule.id, enabled: false });
      }
    }
    await loadRules();
    statusMessage.value = "已禁用选中的转发规则";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e: any) {
    statusMessage.value = `批量禁用失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    mutating.value = false;
  }
}

async function batchDelete() {
  mutating.value = true;
  try {
    const ids = Array.from(selectedIds.value);
    for (const id of ids) {
      await invoke("delete_forward_rule", { ruleId: id });
    }
    selectedIds.value = new Set();
    editorForm.value = createEmptyRule();
    showEditor.value = false;
    await loadRules();
    statusMessage.value = "已删除选中的转发规则";
    messageType.value = "success";
    notifyDataChanged();
  } catch (e: any) {
    statusMessage.value = `批量删除失败：${String(e)}`;
    messageType.value = "error";
  } finally {
    mutating.value = false;
  }
}

function startEdit(rule?: ForwardRule) {
  const form = rule ? toForm(rule) : createEmptyRule();
  editorForm.value = form;
  showEditor.value = true;
  selectedIds.value = form.id ? new Set([form.id]) : new Set();
}

function cancelEdit() {
  showEditor.value = false;
}

const statusClass = (status: string): string => {
  switch (status) {
    case "正常":
      return "badge-success";
    case "冲突":
    case "错误":
      return "badge-error";
    case "未连接":
      return "badge-warning";
    case "已禁用":
      return "badge-disabled";
    default:
      return "badge-success";
  }
};

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
          <CirclePlus :size="15" :stroke-width="2.2" />
          新增规则
        </button>
        <button
          class="btn btn-secondary"
          type="button"
          :disabled="selectedCount === 0 || loading || mutating"
          @click="batchEnable"
        >
          <Play :size="15" :stroke-width="2.2" />
          启用
        </button>
        <button
          class="btn btn-secondary"
          type="button"
          :disabled="selectedCount === 0 || loading || mutating"
          @click="batchDisable"
        >
          <Pause :size="15" :stroke-width="2.2" />
          禁用
        </button>
        <button
          class="btn btn-secondary"
          type="button"
          :disabled="selectedCount === 0 || loading || mutating"
          @click="batchDelete"
        >
          <Trash2 :size="15" :stroke-width="2.2" />
          删除
        </button>
      </div>
    </header>

    <p v-if="statusMessage" class="status-message" :class="`msg-${messageType}`">
      {{ statusMessage }}
    </p>

    <div class="table-wrapper">
      <table class="rules-table">
        <colgroup>
          <col class="col-check" />
          <col class="col-enabled" />
          <col class="col-protocol" />
          <col class="col-listen-addr" />
          <col class="col-listen-port" />
          <col class="col-target-ip" />
          <col class="col-target-port" />
          <col class="col-remark" />
          <col class="col-status" />
          <col class="col-actions" />
        </colgroup>
        <thead>
          <tr>
            <th class="check-cell">
              <input
                type="checkbox"
                :checked="allSelected"
                @change="toggleSelectAll(($event.target as HTMLInputElement).checked)"
              />
            </th>
            <th>启用</th>
            <th>协议</th>
            <th>监听地址</th>
            <th>监听端口</th>
            <th>目标设备 IP</th>
            <th>目标端口</th>
            <th>备注</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="rules.length === 0" class="empty-row">
            <td colspan="10">暂无转发规则</td>
          </tr>
          <tr
            v-for="rule in rules"
            :key="rule.id"
            :class="{ selected: isSelected(rule.id) }"
          >
            <td class="check-cell">
              <input
                type="checkbox"
                :checked="selectedIds.has(rule.id)"
                @change="toggleSelectRule(rule.id)"
              />
            </td>
            <td>
              <label
                class="toggle-switch"
                :aria-label="`${rule.remark || '转发规则'} 启用状态`"
              >
                <input
                  type="checkbox"
                  :checked="rule.enabled"
                  :disabled="mutating"
                  @change="toggleRuleEnabled(rule)"
                />
                <span class="toggle-slider"></span>
              </label>
            </td>
            <td>{{ rule.protocol }}</td>
            <td>{{ rule.listen_addr }}</td>
            <td>{{ rule.listen_port }}</td>
            <td>{{ rule.target_ip }}</td>
            <td>{{ rule.target_port }}</td>
            <td>{{ rule.remark || "-" }}</td>
            <td>
              <span class="status-pill" :class="statusClass(rule.status)">
                {{ rule.status || "正常" }}
              </span>
            </td>
            <td>
              <div class="row-actions">
                <button class="icon-action edit" type="button" title="编辑" @click="startEdit(rule)">
                  <Pencil :size="15" :stroke-width="2.1" />
                </button>
                <button class="icon-action delete" type="button" title="删除" @click="deleteRule(rule.id)">
                  <Trash2 :size="15" :stroke-width="2.1" />
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <section v-if="showEditor" class="editor-section">
      <h3>{{ editorTitle }}</h3>
      <div class="editor-grid">
        <label>
          <span>外部协议</span>
          <select v-model="editorForm.protocol">
            <option value="TCP">TCP</option>
            <option value="UDP">UDP</option>
            <option value="TCP+UDP">TCP+UDP</option>
          </select>
        </label>
        <label>
          <span>监听 IP</span>
          <input v-model="editorForm.listen_addr" type="text" placeholder=":: 或留空" />
        </label>
        <label>
          <span>监听端口</span>
          <input
            v-model.trim="editorForm.listen_port"
            type="text"
            inputmode="numeric"
            placeholder="80;443;1000-1003"
            title="支持单端口、分号分隔和范围，例如 80;443;1000-1003"
          />
        </label>
        <label>
          <span>目标设备 IP</span>
          <input v-model="editorForm.target_ip" type="text" placeholder="目标 IP 地址" />
        </label>
        <label>
          <span>目标端口</span>
          <input
            v-model.trim="editorForm.target_port"
            type="text"
            inputmode="numeric"
            placeholder="80 或 2000-2003"
            title="写一个端口表示所有监听端口转到同一目标端口；也可写同等数量的分号/范围表达式"
          />
        </label>
        <label>
          <span>转发模式</span>
          <select v-model="editorForm.mode">
            <option value="nat">NAT（默认）</option>
            <option value="forward">透明转发</option>
          </select>
        </label>
        <label>
          <span>备注</span>
          <input v-model="editorForm.remark" type="text" />
        </label>
      </div>
      <div class="editor-actions">
        <button class="btn btn-primary" type="button" :disabled="saving" @click="saveRule">
          {{ saving ? "保存中..." : "保存" }}
        </button>
        <button class="btn btn-secondary" type="button" :disabled="saving" @click="cancelEdit">
          取消
        </button>
      </div>
    </section>

    <footer class="panel-footer">
      <Info class="footer-icon footer-icon-info" :size="18" :stroke-width="2.2" />
      <span>监听 IP 为空或 :: 表示监听所有 IPv4/IPv6 地址；监听端口支持 80;443;1000-1003。</span>
      <span class="rule-summary">启用：{{ enabledCount }} / {{ rules.length }}</span>
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

.rules-panel {
  height: 100%;
}

.panel-header {
  height: 56px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 0 14px 0 26px;
  border-bottom: 1px solid #e1e8f2;
}

.panel-header h2 {
  font-size: 18px;
  font-weight: 800;
  color: #151922;
  white-space: nowrap;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
}

.btn {
  height: 38px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 0 12px;
  border-radius: 5px;
  border: 1px solid transparent;
  font-size: 13px;
  font-weight: 700;
  white-space: nowrap;
}

.btn svg,
.icon-action svg {
  display: block;
}

.btn:disabled {
  opacity: 0.48;
  cursor: not-allowed;
}

.btn-primary {
  color: #ffffff;
  background: var(--color-primary, #2563eb);
  border-color: var(--color-primary, #2563eb);
}

.btn-secondary {
  color: #5c6675;
  background: #ffffff;
  border-color: #d7e0eb;
}

.plus-icon,
.play-icon,
.pause-icon,
.trash-icon {
  position: relative;
  width: 15px;
  height: 15px;
  display: inline-block;
}

.plus-icon::before,
.plus-icon::after,
.pause-icon::before,
.pause-icon::after,
.trash-icon::before,
.trash-icon::after {
  content: "";
  position: absolute;
  display: block;
}

.plus-icon {
  border: 1.8px solid currentColor;
  border-radius: 50%;
}

.plus-icon::before {
  left: 3px;
  right: 3px;
  top: 6px;
  height: 1.8px;
  background: currentColor;
}

.plus-icon::after {
  top: 3px;
  bottom: 3px;
  left: 6px;
  width: 1.8px;
  background: currentColor;
}

.play-icon::before {
  content: "";
  position: absolute;
  left: 4px;
  top: 2px;
  width: 0;
  height: 0;
  border-top: 5px solid transparent;
  border-bottom: 5px solid transparent;
  border-left: 8px solid currentColor;
}

.pause-icon::before,
.pause-icon::after {
  top: 2px;
  width: 3px;
  height: 11px;
  background: currentColor;
}

.pause-icon::before {
  left: 4px;
}

.pause-icon::after {
  right: 4px;
}

.trash-icon::before {
  left: 4px;
  top: 5px;
  width: 8px;
  height: 9px;
  border: 1.7px solid currentColor;
  border-top: 0;
  border-radius: 0 0 2px 2px;
}

.trash-icon::after {
  left: 3px;
  top: 2px;
  width: 10px;
  height: 2px;
  background: currentColor;
  box-shadow: 3px -2px 0 -0.5px currentColor;
}

.status-message {
  margin: 8px 14px 0;
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

.table-wrapper {
  flex: 1 1 auto;
  overflow: auto;
  min-height: 0;
}

.rules-table {
  width: 100%;
  min-width: 0;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 13px;
}

.rules-table th,
.rules-table td {
  height: 40px;
  padding: 0 8px;
  border-right: 1px solid #e6edf5;
  border-bottom: 1px solid #e1e8f2;
  white-space: nowrap;
  text-align: left;
  vertical-align: middle;
}

.rules-table th {
  height: 40px;
  color: #202532;
  font-weight: 800;
  background: #f8fafc;
}

.rules-table th:last-child,
.rules-table td:last-child {
  border-right: 0;
}

.rules-table tr.selected td {
  background: #eef6ff;
  box-shadow: inset 0 0 0 1px rgba(37, 99, 235, 0.06);
}

.rules-table .empty-row td {
  height: 112px;
  color: #8a94a6;
  text-align: center;
  font-weight: 600;
  background: #ffffff;
}

.check-cell {
  width: 36px;
  text-align: center !important;
}

.col-check { width: 36px; }
.col-enabled { width: 58px; }
.col-protocol { width: 64px; }
.col-listen-addr { width: 90px; }
.col-listen-port { width: 86px; }
.col-target-ip { width: 128px; }
.col-target-port { width: 86px; }
.col-remark { width: 102px; }
.col-status { width: 76px; }
.col-actions { width: 72px; }

input[type="checkbox"] {
  width: 14px;
  height: 14px;
  accent-color: var(--color-primary, #2563eb);
}

.toggle-switch {
  position: relative;
  display: inline-block;
  width: 31px;
  height: 18px;
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
  left: 3px;
  top: 3px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: #ffffff;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.2);
  transition: transform 0.15s ease;
}

.toggle-switch input:checked + .toggle-slider {
  background: var(--color-primary, #2563eb);
}

.toggle-switch input:checked + .toggle-slider::before {
  transform: translateX(13px);
}

.status-pill {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 40px;
  height: 22px;
  padding: 0 10px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 800;
}

.badge-success {
  color: #15803d;
  background: #e8f8ee;
  border: 1px solid #c8edd5;
}

.badge-warning {
  color: #b45309;
  background: #fff7e6;
  border: 1px solid #f7dfaa;
}

.badge-error {
  color: #b91c1c;
  background: #fee2e2;
  border: 1px solid #fecaca;
}

.badge-disabled {
  color: #64748b;
  background: #f1f5f9;
  border: 1px solid #e2e8f0;
}

.row-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.icon-action {
  position: relative;
  width: 22px;
  height: 22px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 0;
  background: transparent;
  color: #4b5563;
}

.icon-action span,
.icon-action span::before,
.icon-action span::after {
  content: "";
  position: absolute;
  display: block;
}

.icon-action.edit span {
  left: 5px;
  top: 10px;
  width: 13px;
  height: 3px;
  background: currentColor;
  border-radius: 999px;
  transform: rotate(-45deg);
}

.icon-action.edit span::before {
  right: -3px;
  top: 0;
  width: 3px;
  height: 3px;
  background: currentColor;
}

.icon-action.delete span {
  left: 6px;
  top: 8px;
  width: 10px;
  height: 11px;
  border: 1.7px solid currentColor;
  border-top: 0;
  border-radius: 0 0 2px 2px;
}

.icon-action.delete span::before {
  left: -2px;
  top: -4px;
  width: 12px;
  height: 2px;
  background: currentColor;
}

.editor-section {
  padding: 14px 14px 12px;
  border: 1px solid #b9d5ff;
  border-left: 0;
  border-right: 0;
  background: #f7fbff;
  box-shadow: inset 0 1px 0 #d7e8ff;
}

.editor-section h3 {
  margin-bottom: 10px;
  font-size: 13px;
  font-weight: 800;
  color: #202532;
}

.editor-grid {
  display: grid;
  grid-template-columns: 92px 138px 190px 148px 120px 148px minmax(160px, 1fr);
  gap: 8px 10px;
  align-items: end;
}

.editor-grid label {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.editor-grid span {
  font-size: 12px;
  color: #4f5968;
  font-weight: 700;
}

.editor-grid input,
.editor-grid select {
  width: 100%;
  height: 32px;
  border: 1px solid #d7e0eb;
  border-radius: 5px;
  background: #ffffff;
  color: #202532;
  padding: 0 10px;
  outline: none;
}

.editor-actions {
  display: flex;
  justify-content: flex-end;
  gap: 12px;
  margin-top: 12px;
}

.panel-footer {
  flex: 0 0 auto;
  min-height: 32px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 16px;
  color: #5d6b7d;
  font-size: 12px;
  border-top: 1px solid #e6edf5;
  background: #f7fbff;
}

.footer-icon {
  flex: 0 0 auto;
  display: block;
}

.footer-icon-info {
  color: #2563eb;
}

.rule-summary {
  margin-left: auto;
  color: #7b8798;
}

</style>
