import { useEffect, useMemo, useState } from "react";
import {
  CirclePlus,
  Info,
  Pause,
  Pencil,
  Play,
  RefreshCw,
  Trash2,
} from "lucide-react";
import EditorDrawer from "./EditorDrawer";
import PageHeading from "./PageHeading";
import { formatEndpoint, statusTone } from "../utils/presentation";
import { invokeCommand } from "../lib/tauri";
import type { ForwardRule } from "../types";

interface ForwardRuleForm
  extends Omit<ForwardRule, "listen_port" | "target_port"> {
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
  const [editorForm, setEditorForm] =
    useState<ForwardRuleForm>(createEmptyRule);
  const [statusMessage, setStatusMessage] = useState("");
  const [messageType, setMessageType] = useState<"info" | "success" | "error">(
    "info",
  );
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const [filter, setFilter] = useState<"all" | "enabled" | "disabled">("all");

  const selectedCount = selectedIds.size;
  const selectedRule = useMemo(
    () => rules.find((rule) => selectedIds.has(rule.id)),
    [rules, selectedIds],
  );
  const editorTitle = useMemo(() => {
    if (!editorForm.id) return "新增转发规则";
    return "编辑转发规则";
  }, [editorForm.id, rules]);

  function setRules(nextRules: ForwardRule[]) {
    const normalized = nextRules.map(normalizeRule);
    const previousSelected = Array.from(selectedIds).find((id) =>
      normalized.some((rule) => rule.id === id),
    );
    const selected = normalized.find((rule) => rule.id === previousSelected);

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
      const saved = await invokeCommand<ForwardRule>("save_forward_rule", {
        rule: payload,
      });
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
      current.map((item) =>
        item.id === rule.id ? { ...item, enabled } : item,
      ),
    );
    setMutating(true);
    try {
      await invokeCommand("enable_forward_rule", { ruleId: rule.id, enabled });
      setRulesState((current) =>
        current.map((item) =>
          item.id === rule.id
            ? { ...item, status: enabled ? "正常" : "已停用" }
            : item,
        ),
      );
      notifyDataChanged();
    } catch (error) {
      setRulesState((current) =>
        current.map((item) =>
          item.id === rule.id ? { ...item, enabled: previous } : item,
        ),
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
        current.map((rule) =>
          selectedIds.has(rule.id)
            ? { ...rule, enabled, status: enabled ? "正常" : "已停用" }
            : rule,
        ),
      );
      setStatusMessage(enabled ? "已启用选中规则" : "已禁用选中规则");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      await loadRules();
      setStatusMessage(`批量更新转发规则失败：${String(error)}`);
      setMessageType("error");
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
      await loadRules();
      setStatusMessage(`批量删除转发规则失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setMutating(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  function updateEditor<K extends keyof ForwardRuleForm>(
    key: K,
    value: ForwardRuleForm[K],
  ) {
    setEditorForm((current) => ({ ...current, [key]: value }));
  }

  useEffect(() => {
    loadRules();
  }, []);

  const visibleRules = rules.filter(
    (rule) =>
      filter === "all" || (filter === "enabled" ? rule.enabled : !rule.enabled),
  );
  return (
    <>
      <PageHeading
        eyebrow="SERVICES / PORT FORWARDING"
        title="端口转发"
        description="让外部连接，准确抵达你的本地服务。"
        actions={
          <>
            <button
              className="btn btn-secondary"
              disabled={loading || mutating}
              onClick={loadRules}
            >
              <RefreshCw size={14} className={loading ? "spinning" : ""} />
              刷新
            </button>
            <button
              className="btn btn-primary"
              disabled={mutating}
              onClick={() => startEdit()}
            >
              <CirclePlus size={15} />
              新增规则
            </button>
          </>
        }
      />
      <div className="filter-tabs" aria-label="转发规则状态筛选">
        {(
          [
            ["all", "全部规则"],
            ["enabled", "已启用"],
            ["disabled", "已停用"],
          ] as const
        ).map(([value, label]) => (
          <button
            type="button"
            key={value}
            aria-pressed={filter === value}
            onClick={() => setFilter(value)}
          >
            {label}
            <small>
              {value === "all"
                ? rules.length
                : rules.filter((rule) =>
                    value === "enabled" ? rule.enabled : !rule.enabled,
                  ).length}
            </small>
          </button>
        ))}
      </div>
      <section className="panel rules-panel">
        <header className="panel-header">
          <h2>
            {selectedCount ? `已选择 ${selectedCount} 条规则` : "转发规则"}
          </h2>
          <div className="toolbar">
            <button
              className="btn btn-secondary"
              disabled={!selectedCount || loading || mutating}
              onClick={() => batchEnable(true)}
            >
              <Play size={13} />
              启用
            </button>
            <button
              className="btn btn-secondary"
              disabled={!selectedCount || loading || mutating}
              onClick={() => batchEnable(false)}
            >
              <Pause size={13} />
              停用
            </button>
            <button
              className="btn btn-secondary"
              disabled={!selectedCount || loading || mutating}
              onClick={batchDelete}
            >
              <Trash2 size={13} />
              删除
            </button>
          </div>
        </header>
        {statusMessage && !editorOpen ? (
          <p className={`status-message msg-${messageType}`} role="status">
            {statusMessage}
          </p>
        ) : null}
        <div className="table-wrap">
          <table className="rules-table">
            <thead>
              <tr>
                <th className="selection-cell">选择</th>
                <th>规则 / 服务</th>
                <th>连接路径</th>
                <th>协议</th>
                <th>状态</th>
                <th>启用</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {!visibleRules.length ? (
                <tr>
                  <td className="empty-cell" colSpan={7}>
                    {loading ? "正在读取转发规则" : "暂无匹配的转发规则"}
                  </td>
                </tr>
              ) : (
                visibleRules.map((rule) => (
                  <tr
                    key={rule.id}
                    className={
                      selectedIds.has(rule.id) ? "selected" : undefined
                    }
                  >
                    <td>
                      <input
                        className="row-check"
                        type="checkbox"
                        aria-label={`选择 ${rule.remark || rule.listen_port}`}
                        checked={selectedIds.has(rule.id)}
                        disabled={mutating}
                        onChange={(event) =>
                          toggleSelectRule(rule, event.target.checked)
                        }
                      />
                    </td>
                    <td>
                      <strong>
                        {rule.remark || `端口 ${rule.listen_port}`}
                      </strong>
                      <small>
                        {rule.mode === "relay" ? "普通转发" : rule.mode}
                      </small>
                    </td>
                    <td className="route-cell">
                      <span className="mono">
                        {formatEndpoint(rule.listen_addr, rule.listen_port)}
                      </span>
                      <small className="mono">
                        <span className="route-arrow">↳</span>
                        {formatEndpoint(rule.target_ip, rule.target_port)}
                      </small>
                    </td>
                    <td>
                      <span className="badge badge-neutral">
                        {rule.protocol}
                      </span>
                    </td>
                    <td>
                      <span
                        className={`state-label tone-${rule.enabled ? statusTone(rule.status) : "neutral"}`}
                      >
                        <span className="status-dot" />
                        {rule.enabled ? rule.status || "已启用" : "已停用"}
                      </span>
                    </td>
                    <td>
                      <label className="toggle-switch">
                        <input
                          type="checkbox"
                          aria-label={`启用 ${rule.remark || rule.listen_port}`}
                          checked={rule.enabled}
                          disabled={mutating}
                          onChange={(event) =>
                            setRuleEnabled(rule, event.target.checked)
                          }
                        />
                        <span className="toggle-slider" />
                      </label>
                    </td>
                    <td>
                      <div className="row-actions">
                        <button
                          className="icon-action"
                          aria-label={`编辑 ${rule.remark || rule.listen_port}`}
                          disabled={mutating}
                          onClick={() => startEdit(rule)}
                        >
                          <Pencil size={15} />
                        </button>
                        <button
                          className="icon-action danger-text"
                          aria-label={`删除 ${rule.remark || rule.listen_port}`}
                          disabled={mutating}
                          onClick={() => deleteRule(rule.id)}
                        >
                          <Trash2 size={15} />
                        </button>
                      </div>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
        <footer className="list-footer">
          <span>共 {visibleRules.length} 条规则</span>
          <span>IPv4 / IPv6 双栈支持</span>
        </footer>
      </section>
      <p className="form-note">
        <Info size={16} />
        公网访问还需确保路由器和防火墙允许对应端口。
      </p>
      {editorOpen ? (
        <EditorDrawer
          title={editorTitle}
          onClose={cancelEdit}
          busy={saving}
          footer={
            <>
              <button
                className="btn btn-secondary"
                disabled={saving}
                onClick={cancelEdit}
              >
                取消
              </button>
              <button
                className="btn btn-primary"
                disabled={saving}
                onClick={saveRule}
              >
                {saving ? "正在保存" : "保存规则"}
              </button>
            </>
          }
        >
          {statusMessage ? (
            <p className={`status-message msg-${messageType}`} role="status">
              {statusMessage}
            </p>
          ) : null}
          <div className="forward-editor-grid">
            <label className="field-label wide-field">
              <span>规则备注</span>
              <input
                value={editorForm.remark}
                placeholder="例如 NAS 管理"
                onChange={(event) => updateEditor("remark", event.target.value)}
              />
            </label>
            <h3 className="form-section-title">外部入口</h3>
            <label className="field-label">
              <span>传输协议</span>
              <select
                value={editorForm.protocol}
                onChange={(event) =>
                  updateEditor("protocol", event.target.value)
                }
              >
                {IMPLEMENTED_PROTOCOLS.map((protocol) => (
                  <option key={protocol}>{protocol}</option>
                ))}
              </select>
            </label>
            <label className="field-label">
              <span>转发模式</span>
              <select
                value={editorForm.mode}
                onChange={(event) => updateEditor("mode", event.target.value)}
              >
                <option value="relay">普通转发</option>
              </select>
            </label>
            <label className="field-label">
              <span>监听地址</span>
              <input
                value={editorForm.listen_addr}
                onChange={(event) =>
                  updateEditor("listen_addr", event.target.value)
                }
              />
              <small className="field-hint">默认 ::，监听本机全部地址</small>
            </label>
            <label className="field-label">
              <span>监听端口</span>
              <input
                value={editorForm.listen_port}
                inputMode="numeric"
                onChange={(event) =>
                  updateEditor("listen_port", event.target.value.trim())
                }
              />
              <small className="field-hint">端口范围 1-65535</small>
            </label>
            <h3 className="form-section-title">目标服务</h3>
            <label className="field-label">
              <span>目标设备 IP</span>
              <input
                value={editorForm.target_ip}
                onChange={(event) =>
                  updateEditor("target_ip", event.target.value)
                }
              />
            </label>
            <label className="field-label">
              <span>目标端口</span>
              <input
                value={editorForm.target_port}
                inputMode="numeric"
                onChange={(event) =>
                  updateEditor("target_port", event.target.value.trim())
                }
              />
            </label>
            <div className="binding-preview wide-field">
              <span className="text-muted">连接路径预览</span>
              <p className="mono">
                {formatEndpoint(
                  editorForm.listen_addr || "::",
                  editorForm.listen_port || "--",
                )}
              </p>
              <p className="mono">
                <span className="route-arrow">↳</span>
                {formatEndpoint(
                  editorForm.target_ip || "目标 IP",
                  editorForm.target_port || "--",
                )}
              </p>
            </div>
            <label className="setting-toggle wide-field">
              <span>启用此规则</span>
              <input
                type="checkbox"
                checked={editorForm.enabled}
                onChange={(event) =>
                  updateEditor("enabled", event.target.checked)
                }
              />
            </label>
          </div>
          <p className="form-note">
            <Info size={15} />
            保存后，已启用的规则会立即开始监听。
          </p>
        </EditorDrawer>
      ) : null}
    </>
  );
}
