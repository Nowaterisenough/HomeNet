import { useEffect, useMemo, useState } from "react";
import { CirclePlus, Pause, Pencil, Play, Trash2 } from "lucide-react";
import { useDraggableModal } from "../hooks/useDraggableModal";
import { invokeCommand } from "../lib/tauri";
import type { ForwardRule } from "../types";

interface ForwardRuleForm extends Omit<ForwardRule, "listen_port" | "target_port"> {
  listen_port: string;
  target_port: string;
}

const IMPLEMENTED_PROTOCOLS = ["TCP", "UDP", "TCP+UDP"] as const;

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

export default function ForwardRulesPanel() {
  const [rules, setRulesState] = useState<ForwardRule[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [editorForm, setEditorForm] = useState<ForwardRuleForm>(createEmptyRule);
  const [statusMessage, setStatusMessage] = useState("");
  const [messageType, setMessageType] = useState<"info" | "success" | "error">("info");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const { modalStyle, resetModalPosition, startModalDrag } = useDraggableModal();

  const selectedCount = selectedIds.size;
  const selectedRule = useMemo(
    () => rules.find((rule) => selectedIds.has(rule.id)),
    [rules, selectedIds],
  );
  const editorTitle = useMemo(() => {
    if (!editorForm.id) return "新增转发规则";
    const index = rules.findIndex((rule) => rule.id === editorForm.id);
    return `编辑转发规则（第 ${index >= 0 ? index + 1 : 1} 行）`;
  }, [editorForm.id, rules]);

  function setRules(nextRules: ForwardRule[]) {
    const normalized = nextRules.map(normalizeRule);
    const previousSelected = Array.from(selectedIds).find((id) =>
      normalized.some((rule) => rule.id === id),
    );
    const selected = normalized.find((rule) => rule.id === previousSelected) ?? normalized[0];

    setRulesState(normalized);
    setSelectedIds(selected ? new Set([selected.id]) : new Set());
    setEditorForm(selected ? toForm(selected) : createEmptyRule());
  }

  async function loadRules() {
    setLoading(true);
    try {
      const data = await invokeCommand<ForwardRule[]>("list_forward_rules");
      setRules(data);
      setStatusMessage("");
    } catch (error) {
      setRulesState([]);
      setSelectedIds(new Set());
      setEditorForm(createEmptyRule());
      setStatusMessage(`读取转发规则失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setLoading(false);
    }
  }

  function notifyDataChanged() {
    window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
    window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
  }

  function buildRulePayload(): ForwardRule | null {
    const listenPort = parsePort(editorForm.listen_port);
    const targetPort = parsePort(editorForm.target_port);

    if (!editorForm.target_ip.trim()) {
      setStatusMessage("请填写目标设备 IP");
      setMessageType("error");
      return null;
    }
    if (listenPort === null || targetPort === null) {
      setStatusMessage("端口需为 1-65535 的数字");
      setMessageType("error");
      return null;
    }

    return {
      id: editorForm.id,
      enabled: editorForm.enabled,
      protocol: normalizeProtocol(editorForm.protocol),
      listen_addr: editorForm.listen_addr.trim() || "::",
      listen_port: listenPort,
      target_ip: editorForm.target_ip.trim(),
      target_port: targetPort,
      mode: editorForm.mode.trim() || "relay",
      remark: editorForm.remark.trim(),
      status: editorForm.status || "正常",
    };
  }

  function applySavedRule(rule: ForwardRule) {
    const saved = normalizeRule(rule);
    const next = rules.slice();
    const index = next.findIndex((item) => item.id === saved.id);
    if (index >= 0) next[index] = saved;
    else next.push(saved);
    setRulesState(next);
    setSelectedIds(new Set([saved.id]));
    setEditorForm(toForm(saved));
  }

  async function saveRule() {
    const payload = buildRulePayload();
    if (!payload) return;

    setSaving(true);
    try {
      const saved = await invokeCommand<ForwardRule>("save_forward_rule", { rule: payload });
      applySavedRule(saved);
      setStatusMessage("转发规则已保存");
      setMessageType("success");
      setEditorOpen(false);
      notifyDataChanged();
    } catch (error) {
      setStatusMessage(`保存转发规则失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setSaving(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  function startEdit(rule?: ForwardRule) {
    setEditorForm(rule ? toForm(rule) : createEmptyRule());
    setSelectedIds(rule?.id ? new Set([rule.id]) : new Set());
    resetModalPosition();
    setEditorOpen(true);
  }

  function cancelEdit() {
    setEditorOpen(false);
    setEditorForm(selectedRule ? toForm(selectedRule) : createEmptyRule());
  }

  function toggleSelectRule(rule: ForwardRule, checked: boolean) {
    const next = new Set(selectedIds);
    if (checked) {
      next.add(rule.id);
      setEditorForm(toForm(rule));
    } else {
      next.delete(rule.id);
    }
    setSelectedIds(next);
  }

  async function deleteRule(id: string) {
    setMutating(true);
    try {
      await invokeCommand("delete_forward_rule", { ruleId: id });
      setRules(rules.filter((rule) => rule.id !== id));
      setStatusMessage("转发规则已删除");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      setStatusMessage(`删除转发规则失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setMutating(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  async function setRuleEnabled(rule: ForwardRule, enabled: boolean) {
    const previous = rule.enabled;
    setRulesState((current) =>
      current.map((item) => (item.id === rule.id ? { ...item, enabled } : item)),
    );
    setMutating(true);
    try {
      await invokeCommand("enable_forward_rule", { ruleId: rule.id, enabled });
      notifyDataChanged();
    } catch (error) {
      setRulesState((current) =>
        current.map((item) => (item.id === rule.id ? { ...item, enabled: previous } : item)),
      );
      setStatusMessage(`切换转发规则失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setMutating(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  async function batchEnable(enabled: boolean) {
    setMutating(true);
    try {
      for (const id of Array.from(selectedIds)) {
        await invokeCommand("enable_forward_rule", { ruleId: id, enabled });
      }
      setRulesState((current) =>
        current.map((rule) => (selectedIds.has(rule.id) ? { ...rule, enabled } : rule)),
      );
      setStatusMessage(enabled ? "已启用选中规则" : "已禁用选中规则");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      setStatusMessage(`批量更新转发规则失败：${String(error)}`);
      setMessageType("error");
      await loadRules();
    } finally {
      setMutating(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  async function batchDelete() {
    setMutating(true);
    try {
      for (const id of Array.from(selectedIds)) {
        await invokeCommand("delete_forward_rule", { ruleId: id });
      }
      setRules(rules.filter((rule) => !selectedIds.has(rule.id)));
      setStatusMessage("已删除选中规则");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      setStatusMessage(`批量删除转发规则失败：${String(error)}`);
      setMessageType("error");
      await loadRules();
    } finally {
      setMutating(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  function updateEditor<K extends keyof ForwardRuleForm>(key: K, value: ForwardRuleForm[K]) {
    setEditorForm((current) => ({ ...current, [key]: value }));
  }

  useEffect(() => {
    loadRules();
  }, []);

  return (
    <section className="panel rules-panel">
      <header className="panel-header">
        <h2>IPv6/IPv4 转发规则</h2>
        <div className="toolbar">
          <button className="btn btn-primary" type="button" onClick={() => startEdit()}>
            <CirclePlus size={13} strokeWidth={2.2} />
            新增规则
          </button>
          <button
            className="btn btn-secondary"
            type="button"
            disabled={selectedCount === 0 || loading || mutating}
            onClick={() => batchEnable(true)}
          >
            <Play size={13} strokeWidth={2.2} />
            启用
          </button>
          <button
            className="btn btn-secondary"
            type="button"
            disabled={selectedCount === 0 || loading || mutating}
            onClick={() => batchEnable(false)}
          >
            <Pause size={13} strokeWidth={2.2} />
            禁用
          </button>
          <button
            className="btn btn-secondary"
            type="button"
            disabled={selectedCount === 0 || loading || mutating}
            onClick={batchDelete}
          >
            <Trash2 size={13} strokeWidth={2.2} />
            删除
          </button>
        </div>
      </header>

      {statusMessage ? <p className={`status-message msg-${messageType}`}>{statusMessage}</p> : null}

      <div className="table-wrap">
        <table className="rules-table">
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
            {rules.length === 0 ? (
              <tr>
                <td className="empty-cell" colSpan={10}>
                  {loading ? "正在读取转发规则" : "暂无转发规则"}
                </td>
              </tr>
            ) : (
              rules.map((rule) => (
                <tr key={rule.id} className={selectedIds.has(rule.id) ? "selected" : undefined}>
                  <td>
                    <label className="toggle-switch">
                      <input
                        type="checkbox"
                        checked={rule.enabled}
                        disabled={mutating}
                        onChange={(event) => setRuleEnabled(rule, event.target.checked)}
                      />
                      <span className="toggle-slider" />
                    </label>
                  </td>
                  <td>{rule.protocol}</td>
                  <td>{rule.listen_addr}</td>
                  <td>{rule.listen_port}</td>
                  <td>{rule.target_ip}</td>
                  <td>{rule.target_port}</td>
                  <td>{rule.mode}</td>
                  <td>{rule.remark || "-"}</td>
                  <td>{rule.status || "-"}</td>
                  <td>
                    <div className="row-actions">
                      <input
                        className="row-check"
                        type="checkbox"
                        checked={selectedIds.has(rule.id)}
                        onChange={(event) => toggleSelectRule(rule, event.target.checked)}
                      />
                      <button className="icon-action" type="button" title="编辑" onClick={() => startEdit(rule)}>
                        <Pencil size={13} strokeWidth={2.1} />
                      </button>
                      <button className="icon-action" type="button" title="删除" onClick={() => deleteRule(rule.id)}>
                        <Trash2 size={13} strokeWidth={2.1} />
                      </button>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {editorOpen ? (
        <div className="modal-backdrop" onClick={(event) => event.currentTarget === event.target && cancelEdit()}>
          <section className="modal-dialog forward-modal" style={modalStyle}>
            <header className="modal-header draggable-header" onPointerDown={startModalDrag}>
              <h3>{editorTitle}</h3>
              <button
                className="btn btn-secondary"
                type="button"
                onPointerDown={(event) => event.stopPropagation()}
                onClick={cancelEdit}
              >
                关闭
              </button>
            </header>
            <div className="editor-section">
              <div className="forward-editor-grid">
                <label className="field-label">
                  <span>协议</span>
                  <div className="field-control">
                    <select value={editorForm.protocol} onChange={(event) => updateEditor("protocol", event.target.value)}>
                      {IMPLEMENTED_PROTOCOLS.map((protocol) => (
                        <option key={protocol} value={protocol}>
                          {protocol}
                        </option>
                      ))}
                    </select>
                    <small className="field-hint">选择需要转发的传输协议。</small>
                  </div>
                </label>
                <label className="field-label">
                  <span>目标设备 IP</span>
                  <div className="field-control">
                    <input value={editorForm.target_ip} onChange={(event) => updateEditor("target_ip", event.target.value)} />
                    <small className="field-hint">填写内网目标设备的 IPv4 或 IPv6 地址。</small>
                  </div>
                </label>
                <label className="field-label">
                  <span>监听地址</span>
                  <div className="field-control">
                    <input value={editorForm.listen_addr} onChange={(event) => updateEditor("listen_addr", event.target.value)} />
                    <small className="field-hint">默认 ::，表示监听本机全部地址。</small>
                  </div>
                </label>
                <label className="field-label">
                  <span>目标端口</span>
                  <div className="field-control">
                    <input
                      value={editorForm.target_port}
                      type="text"
                      inputMode="numeric"
                      onChange={(event) => updateEditor("target_port", event.target.value.trim())}
                    />
                    <small className="field-hint">目标设备上实际服务端口。</small>
                  </div>
                </label>
                <label className="field-label">
                  <span>监听端口</span>
                  <div className="field-control">
                    <input
                      value={editorForm.listen_port}
                      type="text"
                      inputMode="numeric"
                      onChange={(event) => updateEditor("listen_port", event.target.value.trim())}
                    />
                    <small className="field-hint">外部访问入口端口，范围 1-65535。</small>
                  </div>
                </label>
                <label className="field-label">
                  <span>转发模式</span>
                  <div className="field-control">
                    <select value={editorForm.mode} onChange={(event) => updateEditor("mode", event.target.value)}>
                      <option value="relay">普通 TCP/UDP 转发</option>
                    </select>
                    <small className="field-hint">保持普通转发即可，适合常规服务映射。</small>
                  </div>
                </label>
                <label className="field-label forward-remark">
                  <span>备注</span>
                  <div className="field-control">
                    <input value={editorForm.remark} onChange={(event) => updateEditor("remark", event.target.value)} />
                    <small className="field-hint">可填写用途、设备名或服务名，便于后续识别。</small>
                  </div>
                </label>
              </div>
              <div className="editor-actions editor-actions-right">
                <button className="btn btn-primary" type="button" disabled={saving} onClick={saveRule}>
                  保存规则
                </button>
                <button className="btn btn-secondary" type="button" disabled={saving} onClick={cancelEdit}>
                  取消
                </button>
              </div>
            </div>
          </section>
        </div>
      ) : null}
    </section>
  );
}
