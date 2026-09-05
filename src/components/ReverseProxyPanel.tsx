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
import type { DdnsConfig, ReverseProxyRule } from "../types";

interface ReverseProxyForm
  extends Omit<ReverseProxyRule, "listen_port" | "backend_port"> {
  listen_port: string;
  backend_port: string;
}

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
    backend_port:
      Number(rule.backend_port) || (protocol === "HTTPS" ? 443 : 80),
    tls: rule.tls || (protocol === "HTTPS" ? "passthrough" : "off"),
    certificate: rule.certificate || (protocol === "HTTPS" ? "backend" : ""),
    acme_email: rule.acme_email || "",
    acme_dns_provider: rule.acme_dns_provider || "aliyun",
    acme_access_key_id: rule.acme_access_key_id || "",
    acme_access_key_secret: rule.acme_access_key_secret || "",
    acme_dns_domain: rule.acme_dns_domain || "",
    acme_directory_url:
      rule.acme_directory_url ||
      "https://acme-v02.api.letsencrypt.org/directory",
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

function cleanDomainPart(value: string): string {
  return value.trim().replace(/^\.+|\.+$/g, "");
}

function ddnsHostFromConfig(config: DdnsConfig): string {
  const domain = cleanDomainPart(config.domain);
  const subDomain = cleanDomainPart(config.sub_domain);
  if (!domain) return "";
  return subDomain ? `${subDomain}.${domain}` : domain;
}

function createEmptyRule(
  defaultDomain = "",
  defaultDnsDomain = "",
): ReverseProxyForm {
  return {
    id: "",
    enabled: true,
    protocol: "HTTP",
    domain: defaultDomain,
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
    acme_dns_domain: defaultDnsDomain,
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

export default function ReverseProxyPanel() {
  const [rules, setRulesState] = useState<ReverseProxyRule[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [editor, setEditor] = useState<ReverseProxyForm>(createEmptyRule);
  const [ddnsHost, setDdnsHost] = useState("");
  const [ddnsBaseDomain, setDdnsBaseDomain] = useState("");
  const [statusMessage, setStatusMessage] = useState("");
  const [messageType, setMessageType] = useState<"info" | "success" | "error">(
    "info",
  );
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [issuingCertificate, setIssuingCertificate] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);

  const selectedCount = selectedIds.size;
  const selectedRule = useMemo(
    () => rules.find((rule) => selectedIds.has(rule.id)),
    [rules, selectedIds],
  );
  const editorTitle = useMemo(() => {
    if (!editor.id) return "新增反向代理";
    const index = rules.findIndex((rule) => rule.id === editor.id);
    return `编辑反向代理（第 ${index >= 0 ? index + 1 : 1} 行）`;
  }, [editor.id, rules]);
  const ddnsHostHint = ddnsHost
    ? `默认使用本机 DDNS：${ddnsHost}；仅在其它 Host/CNAME 也指向本机时手动覆盖。`
    : "未读取到本机 DDNS 域名，请先配置 DDNS 或手动填写完整 Host。";

  function setRules(nextRules: ReverseProxyRule[]) {
    const normalized = nextRules.map(normalizeRule);
    const previousSelected = Array.from(selectedIds).find((id) =>
      normalized.some((rule) => rule.id === id),
    );
    const selected = normalized.find((rule) => rule.id === previousSelected);

    setRulesState(normalized);
    setSelectedIds(selected ? new Set([selected.id]) : new Set());
    setEditor(
      selected ? toForm(selected) : createEmptyRule(ddnsHost, ddnsBaseDomain),
    );
  }

  async function loadRules() {
    setLoading(true);
    try {
      const data = await invokeCommand<ReverseProxyRule[]>(
        "list_reverse_proxy_rules",
      );
      setRules(data);
      setStatusMessage("");
    } catch (error) {
      setRulesState([]);
      setSelectedIds(new Set());
      setEditor(createEmptyRule(ddnsHost, ddnsBaseDomain));
      setStatusMessage(`读取反向代理规则失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setLoading(false);
    }
  }

  async function loadDdnsHost() {
    try {
      const config = await invokeCommand<DdnsConfig>("get_ddns_config");
      const host = ddnsHostFromConfig(config);
      const baseDomain = cleanDomainPart(config.domain);
      setDdnsHost(host);
      setDdnsBaseDomain(baseDomain);
      if (!host && !baseDomain) return;

      setEditor((current) => {
        if (current.id || current.domain.trim()) return current;
        return {
          ...current,
          domain: host,
          acme_dns_domain: current.acme_dns_domain || baseDomain,
        };
      });
    } catch {
      setDdnsHost("");
      setDdnsBaseDomain("");
    }
  }

  function notifyDataChanged() {
    window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
    window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
  }

  function buildPayload(): ReverseProxyRule | null {
    const listenPort = parsePort(editor.listen_port);
    const backendPort = parsePort(editor.backend_port);
    const matchedDomain = editor.domain.trim() || ddnsHost;
    if (!matchedDomain) {
      setStatusMessage("请填写匹配域名");
      setMessageType("error");
      return null;
    }
    if (!editor.backend_ip.trim()) {
      setStatusMessage("请填写后端地址");
      setMessageType("error");
      return null;
    }
    if (listenPort === null || backendPort === null) {
      setStatusMessage("端口需为 1-65535 的数字");
      setMessageType("error");
      return null;
    }

    const protocol = normalizeProtocol(editor.protocol);
    return {
      id: editor.id,
      enabled: editor.enabled,
      protocol,
      domain: matchedDomain,
      listen_addr: editor.listen_addr.trim() || "::",
      listen_port: listenPort,
      backend_ip: editor.backend_ip.trim(),
      backend_port: backendPort,
      tls: editor.tls || (protocol === "HTTPS" ? "passthrough" : "off"),
      certificate: editor.certificate.trim(),
      acme_email: editor.acme_email.trim(),
      acme_dns_provider: editor.acme_dns_provider.trim() || "aliyun",
      acme_access_key_id: editor.acme_access_key_id.trim(),
      acme_access_key_secret: editor.acme_access_key_secret.trim(),
      acme_dns_domain: editor.acme_dns_domain.trim() || ddnsBaseDomain,
      acme_directory_url:
        editor.acme_directory_url.trim() ||
        "https://acme-v02.api.letsencrypt.org/directory",
      certificate_path: editor.certificate_path.trim(),
      private_key_path: editor.private_key_path.trim(),
      certificate_expires_at: editor.certificate_expires_at,
      certificate_last_issued_at: editor.certificate_last_issued_at,
      certificate_last_error: editor.certificate_last_error,
      remark: editor.remark.trim(),
      status: editor.status || "正常",
    };
  }

  function applySavedRule(rule: ReverseProxyRule) {
    const saved = normalizeRule(rule);
    const next = rules.slice();
    const index = next.findIndex((item) => item.id === saved.id);
    if (index >= 0) next[index] = saved;
    else next.push(saved);
    setRulesState(next);
    setSelectedIds(new Set([saved.id]));
    setEditor(toForm(saved));
  }

  async function saveRule() {
    const payload = buildPayload();
    if (!payload) return;

    setSaving(true);
    try {
      const saved = await invokeCommand<ReverseProxyRule>(
        "save_reverse_proxy_rule",
        { rule: payload },
      );
      applySavedRule(saved);
      setStatusMessage("反向代理规则已保存");
      setMessageType("success");
      setEditorOpen(false);
      notifyDataChanged();
    } catch (error) {
      setStatusMessage(`保存反向代理规则失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setSaving(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  async function issueCertificate() {
    let payload = buildPayload();
    if (!payload) return;
    if (payload.tls !== "auto") {
      setStatusMessage("请选择 TLS 自动证书后再申请");
      setMessageType("error");
      return;
    }

    setIssuingCertificate(true);
    try {
      payload = await invokeCommand<ReverseProxyRule>(
        "save_reverse_proxy_rule",
        { rule: payload },
      );
      applySavedRule(payload);
      const issued = await invokeCommand<ReverseProxyRule>(
        "issue_reverse_proxy_certificate",
        {
          ruleId: payload.id,
        },
      );
      applySavedRule(issued);
      setStatusMessage("自动证书已申请或续期");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      setStatusMessage(`自动证书申请失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setIssuingCertificate(false);
      window.setTimeout(() => setStatusMessage(""), 5200);
    }
  }

  function startEdit(rule?: ReverseProxyRule) {
    setEditor(rule ? toForm(rule) : createEmptyRule(ddnsHost, ddnsBaseDomain));
    setSelectedIds(rule?.id ? new Set([rule.id]) : new Set());
    setEditorOpen(true);
  }

  function cancelEdit() {
    setEditorOpen(false);
    setEditor(
      selectedRule
        ? toForm(selectedRule)
        : createEmptyRule(ddnsHost, ddnsBaseDomain),
    );
  }

  function toggleSelectRule(rule: ReverseProxyRule, checked: boolean) {
    const next = new Set(selectedIds);
    if (checked) {
      next.add(rule.id);
      setEditor(toForm(rule));
    } else {
      next.delete(rule.id);
    }
    setSelectedIds(next);
  }

  async function deleteRule(id: string) {
    setMutating(true);
    try {
      await invokeCommand("delete_reverse_proxy_rule", { ruleId: id });
      setRules(rules.filter((rule) => rule.id !== id));
      setStatusMessage("反向代理规则已删除");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      setStatusMessage(`删除反向代理规则失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setMutating(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  async function setRuleEnabled(rule: ReverseProxyRule, enabled: boolean) {
    const previous = rule.enabled;
    setRulesState((current) =>
      current.map((item) =>
        item.id === rule.id ? { ...item, enabled } : item,
      ),
    );
    setMutating(true);
    try {
      await invokeCommand("enable_reverse_proxy_rule", {
        ruleId: rule.id,
        enabled,
      });
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
      setStatusMessage(`切换反向代理规则失败：${String(error)}`);
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
        await invokeCommand("enable_reverse_proxy_rule", {
          ruleId: id,
          enabled,
        });
      }
      setRulesState((current) =>
        current.map((rule) =>
          selectedIds.has(rule.id)
            ? { ...rule, enabled, status: enabled ? "正常" : "已停用" }
            : rule,
        ),
      );
      setStatusMessage(enabled ? "已启用选中反代" : "已禁用选中反代");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      await loadRules();
      setStatusMessage(`批量更新反向代理失败：${String(error)}`);
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
        await invokeCommand("delete_reverse_proxy_rule", { ruleId: id });
      }
      setRules(rules.filter((rule) => !selectedIds.has(rule.id)));
      setStatusMessage("已删除选中反代");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      await loadRules();
      setStatusMessage(`批量删除反向代理失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setMutating(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  function updateEditor<K extends keyof ReverseProxyForm>(
    key: K,
    value: ReverseProxyForm[K],
  ) {
    setEditor((current) => ({ ...current, [key]: value }));
  }

  useEffect(() => {
    loadRules();
    loadDdnsHost();
  }, []);

  return (
    <>
      <PageHeading
        eyebrow="SERVICES / REVERSE PROXY"
        title="反向代理"
        description="统一域名入口，让服务安全连接。"
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
              新增代理
            </button>
          </>
        }
      />
      <section className="panel reverse-panel">
        <header className="panel-header">
          <h2>
            {selectedCount ? `已选择 ${selectedCount} 条代理` : "域名入口"}
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
          <table className="proxy-table">
            <thead>
              <tr>
                <th className="selection-cell">选择</th>
                <th>域名 / 服务</th>
                <th>连接路径</th>
                <th>TLS / 证书</th>
                <th>状态</th>
                <th>启用</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {!rules.length ? (
                <tr>
                  <td className="empty-cell" colSpan={7}>
                    {loading ? "正在读取反向代理规则" : "暂无反向代理规则"}
                  </td>
                </tr>
              ) : (
                rules.map((rule) => (
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
                        aria-label={`选择 ${rule.domain}`}
                        checked={selectedIds.has(rule.id)}
                        disabled={mutating}
                        onChange={(event) =>
                          toggleSelectRule(rule, event.target.checked)
                        }
                      />
                    </td>
                    <td>
                      <strong>{rule.domain}</strong>
                      <small>
                        {rule.protocol}
                        {rule.remark ? ` · ${rule.remark}` : ""}
                      </small>
                    </td>
                    <td className="route-cell">
                      <span className="mono">
                        {formatEndpoint(rule.listen_addr, rule.listen_port)}
                      </span>
                      <small className="mono">
                        <span className="route-arrow">↳</span>
                        {formatEndpoint(rule.backend_ip, rule.backend_port)}
                      </small>
                    </td>
                    <td>
                      <span className="badge badge-neutral">
                        {(
                          {
                            auto: "自动证书",
                            manual: "手动证书",
                            passthrough: "HTTPS 透传",
                            off: "未启用 TLS",
                          } as Record<string, string>
                        )[rule.tls] || rule.tls}
                      </span>
                      <small>
                        {rule.certificate_expires_at
                          ? `到期 ${rule.certificate_expires_at.split("T")[0]}`
                          : rule.certificate || "--"}
                      </small>
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
                          aria-label={`启用 ${rule.domain}`}
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
                          aria-label={`编辑 ${rule.domain}`}
                          disabled={mutating}
                          onClick={() => startEdit(rule)}
                        >
                          <Pencil size={15} />
                        </button>
                        <button
                          className="icon-action danger-text"
                          aria-label={`删除 ${rule.domain}`}
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
          <span>共 {rules.length} 条代理</span>
          <span>{rules.filter((rule) => rule.enabled).length} 条已启用</span>
        </footer>
      </section>
      <p className="form-note">
        <Info size={16} />
        HTTP 按 Host 转发；HTTPS 支持 SNI 透传与自动 / 手动证书。
      </p>
      {editorOpen ? (
        <EditorDrawer
          title={editorTitle}
          onClose={cancelEdit}
          busy={saving || issuingCertificate}
          wide
          footer={
            <>
              <button
                className="btn btn-secondary"
                type="button"
                disabled={saving || issuingCertificate || editor.tls !== "auto"}
                onClick={issueCertificate}
              >
                申请/续期
              </button>
              <button
                className="btn btn-primary"
                type="button"
                disabled={saving || issuingCertificate}
                onClick={saveRule}
              >
                保存代理
              </button>
              <button
                className="btn btn-secondary"
                type="button"
                disabled={saving || issuingCertificate}
                onClick={cancelEdit}
              >
                取消
              </button>
            </>
          }
        >
          {statusMessage ? (
            <p className={`status-message msg-${messageType}`} role="status">
              {statusMessage}
            </p>
          ) : null}
          <div className="editor-section">
            <div className="proxy-editor-grid">
              <h3 className="form-section-title">域名入口</h3>
              <label className="field-label">
                <span>协议</span>
                <div className="field-control">
                  <select
                    value={editor.protocol}
                    onChange={(event) =>
                      updateEditor("protocol", event.target.value)
                    }
                  >
                    <option>HTTP</option>
                    <option>HTTPS</option>
                  </select>
                  <small className="field-hint">选择外部访问使用的协议。</small>
                </div>
              </label>
              <label className="field-label">
                <span>匹配域名</span>
                <div className="field-control">
                  <div
                    className={`domain-match-control ${ddnsHost ? "" : "domain-match-control-single"}`.trim()}
                  >
                    <input
                      value={editor.domain}
                      placeholder={ddnsHost || "home.example.com"}
                      onChange={(event) =>
                        updateEditor("domain", event.target.value)
                      }
                    />
                    {ddnsHost ? (
                      <button
                        className="domain-fill-button"
                        type="button"
                        onClick={() => updateEditor("domain", ddnsHost)}
                      >
                        DDNS
                      </button>
                    ) : null}
                  </div>
                  <small className="field-hint">{ddnsHostHint}</small>
                </div>
              </label>
              <label className="field-label">
                <span>监听地址</span>
                <div className="field-control">
                  <input
                    value={editor.listen_addr}
                    onChange={(event) =>
                      updateEditor("listen_addr", event.target.value)
                    }
                  />
                  <small className="field-hint">
                    默认 ::，表示监听本机全部地址。
                  </small>
                </div>
              </label>
              <label className="field-label">
                <span>监听端口</span>
                <div className="field-control">
                  <input
                    value={editor.listen_port}
                    type="text"
                    inputMode="numeric"
                    onChange={(event) =>
                      updateEditor("listen_port", event.target.value.trim())
                    }
                  />
                  <small className="field-hint">
                    外部访问入口端口，HTTP 常用 80。
                  </small>
                </div>
              </label>
              <h3 className="form-section-title">后端服务</h3>
              <label className="field-label">
                <span>后端地址</span>
                <div className="field-control">
                  <input
                    value={editor.backend_ip}
                    onChange={(event) =>
                      updateEditor("backend_ip", event.target.value)
                    }
                  />
                  <small className="field-hint">
                    填写内网后端服务 IP 或主机名。
                  </small>
                </div>
              </label>
              <label className="field-label">
                <span>后端端口</span>
                <div className="field-control">
                  <input
                    value={editor.backend_port}
                    type="text"
                    inputMode="numeric"
                    onChange={(event) =>
                      updateEditor("backend_port", event.target.value.trim())
                    }
                  />
                  <small className="field-hint">后端服务实际监听端口。</small>
                </div>
              </label>
              <h3 className="form-section-title">TLS 与证书</h3>
              <label className="field-label">
                <span>TLS</span>
                <div className="field-control">
                  <select
                    value={editor.tls}
                    onChange={(event) =>
                      updateEditor("tls", event.target.value)
                    }
                  >
                    <option value="off">关闭</option>
                    <option value="passthrough">HTTPS 透传</option>
                    <option value="auto">自动证书</option>
                    <option value="manual">手动证书</option>
                  </select>
                  <small className="field-hint">
                    按后端证书能力选择 TLS 处理方式。
                  </small>
                </div>
              </label>
              <label className="field-label">
                <span>证书配置</span>
                <div className="field-control">
                  <input
                    value={editor.certificate}
                    placeholder="可留空"
                    onChange={(event) =>
                      updateEditor("certificate", event.target.value)
                    }
                  />
                  <small className="field-hint">
                    HTTPS 透传可留空；终止 TLS 时填写证书说明或路径。
                  </small>
                </div>
              </label>
              {editor.tls === "auto" ? (
                <>
                  <label className="field-label">
                    <span>ACME 邮箱</span>
                    <div className="field-control">
                      <input
                        value={editor.acme_email}
                        type="email"
                        onChange={(event) =>
                          updateEditor("acme_email", event.target.value)
                        }
                      />
                      <small className="field-hint">
                        用于接收证书签发和过期通知。
                      </small>
                    </div>
                  </label>
                  <label className="field-label">
                    <span>DNS 主域名</span>
                    <div className="field-control">
                      <input
                        value={editor.acme_dns_domain}
                        placeholder="example.com"
                        onChange={(event) =>
                          updateEditor("acme_dns_domain", event.target.value)
                        }
                      />
                      <small className="field-hint">
                        填写 DNS 验证所在主域名。
                      </small>
                    </div>
                  </label>
                  <label className="field-label">
                    <span>DNS 服务商</span>
                    <div className="field-control">
                      <select
                        value={editor.acme_dns_provider}
                        onChange={(event) =>
                          updateEditor("acme_dns_provider", event.target.value)
                        }
                      >
                        <option value="aliyun">阿里云</option>
                      </select>
                      <small className="field-hint">
                        当前支持阿里云 DNS 验证。
                      </small>
                    </div>
                  </label>
                  <label className="field-label">
                    <span>AccessKey ID</span>
                    <div className="field-control">
                      <input
                        value={editor.acme_access_key_id}
                        onChange={(event) =>
                          updateEditor("acme_access_key_id", event.target.value)
                        }
                      />
                      <small className="field-hint">
                        用于自动创建 DNS 验证记录。
                      </small>
                    </div>
                  </label>
                  <label className="field-label">
                    <span>AccessKey Secret</span>
                    <div className="field-control">
                      <input
                        value={editor.acme_access_key_secret}
                        type="password"
                        onChange={(event) =>
                          updateEditor(
                            "acme_access_key_secret",
                            event.target.value,
                          )
                        }
                      />
                      <small className="field-hint">
                        仅用于证书申请时访问 DNS API。
                      </small>
                    </div>
                  </label>
                  <label className="field-label">
                    <span>到期时间</span>
                    <div className="field-control">
                      <input
                        value={editor.certificate_expires_at || "未申请"}
                        readOnly
                      />
                      <small className="field-hint">
                        最近一次证书签发后的到期时间。
                      </small>
                    </div>
                  </label>
                  <label className="field-label wide-field">
                    <span>ACME 地址</span>
                    <div className="field-control">
                      <input
                        value={editor.acme_directory_url}
                        onChange={(event) =>
                          updateEditor("acme_directory_url", event.target.value)
                        }
                      />
                      <small className="field-hint">
                        默认使用 Let's Encrypt 正式环境。
                      </small>
                    </div>
                  </label>
                  {editor.certificate_last_error ? (
                    <label className="field-label wide-field">
                      <span>证书错误</span>
                      <div className="field-control">
                        <input value={editor.certificate_last_error} readOnly />
                        <small className="field-hint">
                          显示最近一次自动证书申请失败原因。
                        </small>
                      </div>
                    </label>
                  ) : null}
                </>
              ) : null}
              {editor.tls === "manual" ? (
                <>
                  <label className="field-label">
                    <span>证书文件</span>
                    <div className="field-control">
                      <input
                        value={editor.certificate_path}
                        placeholder="fullchain.pem"
                        onChange={(event) =>
                          updateEditor("certificate_path", event.target.value)
                        }
                      />
                      <small className="field-hint">
                        填写 PEM 证书链文件路径。
                      </small>
                    </div>
                  </label>
                  <label className="field-label">
                    <span>私钥文件</span>
                    <div className="field-control">
                      <input
                        value={editor.private_key_path}
                        placeholder="private-key.pem"
                        onChange={(event) =>
                          updateEditor("private_key_path", event.target.value)
                        }
                      />
                      <small className="field-hint">
                        填写与证书匹配的私钥文件路径。
                      </small>
                    </div>
                  </label>
                </>
              ) : null}
            </div>
            <div className="proxy-editor-footer">
              <label className="field-label proxy-remark">
                <span>备注</span>
                <div className="field-control">
                  <input
                    value={editor.remark}
                    onChange={(event) =>
                      updateEditor("remark", event.target.value)
                    }
                  />
                  <small className="field-hint">
                    可填写用途、服务名或维护说明。
                  </small>
                </div>
              </label>
            </div>
          </div>
          <label className="setting-toggle">
            <span>启用此代理</span>
            <input
              type="checkbox"
              checked={editor.enabled}
              onChange={(event) =>
                updateEditor("enabled", event.target.checked)
              }
            />
          </label>
        </EditorDrawer>
      ) : null}
    </>
  );
}
