import { useEffect, useMemo, useState } from "react";
import {
  Activity,
  Copy,
  FolderOpen,
  KeyRound,
  Pencil,
  Settings2,
  ShieldCheck,
  Trash2,
  UserPlus,
  Users,
} from "lucide-react";
import EditorDrawer from "./EditorDrawer";
import { invokeCommand } from "../lib/tauri";
import type {
  SaveWebDavConfigRequest,
  WebDavAccountView,
  WebDavConfigView,
  WebDavRuntimeStatus,
} from "../types";
import { formatSyncTime } from "../utils/time";

type AccountAccess = "read-write" | "read-only";

interface EditableAccount {
  id: string;
  username: string;
  access: AccountAccess;
  enabled: boolean;
  password: string;
  passwordConfigured: boolean;
  lastUsed: string;
}

interface WebDavSettings {
  enabled: boolean;
  rootPath: string;
  listenAddress: string;
  listenPort: string;
  globalReadOnly: boolean;
}

interface AccountForm extends EditableAccount {}

const emptyStatus: WebDavRuntimeStatus = {
  state: "disabled",
  message: "WebDAV 服务尚未配置",
  endpoint: "",
  active_since: "",
};

const initialSettings: WebDavSettings = {
  enabled: false,
  rootPath: "",
  listenAddress: "127.0.0.1",
  listenPort: "19080",
  globalReadOnly: false,
};

function accountFromView(account: WebDavAccountView): EditableAccount {
  return {
    id: account.id,
    username: account.username,
    access: account.read_only ? "read-only" : "read-write",
    enabled: account.enabled,
    password: "",
    passwordConfigured: account.password_configured,
    lastUsed: account.last_used,
  };
}

function emptyAccount(): AccountForm {
  return {
    id: `new-${Date.now()}`,
    username: "",
    access: "read-write",
    enabled: true,
    password: "",
    passwordConfigured: false,
    lastUsed: "",
  };
}

function settingsFromConfig(config: WebDavConfigView): WebDavSettings {
  return {
    enabled: config.enabled,
    rootPath: config.root_path,
    listenAddress: config.listen_addr || "127.0.0.1",
    listenPort: String(config.listen_port || 19080),
    globalReadOnly: config.global_read_only,
  };
}

function parsePort(value: string): number | null {
  const port = Number(value);
  return Number.isInteger(port) && port >= 1 && port <= 65535 ? port : null;
}

function accountLastUsed(account: EditableAccount): string {
  if (!account.enabled) return "账号已停用";
  if (!account.lastUsed.trim()) return "尚未使用";
  return `最后使用 ${formatSyncTime(account.lastUsed)}`;
}

export default function WebDavPanel({
  onOpenProxy,
}: {
  onOpenProxy: () => void;
}) {
  const [settings, setSettings] = useState<WebDavSettings>(initialSettings);
  const [draft, setDraft] = useState<WebDavSettings>(initialSettings);
  const [status, setStatus] = useState<WebDavRuntimeStatus>(emptyStatus);
  const [accounts, setAccounts] = useState<EditableAccount[]>([]);
  const [draftAccounts, setDraftAccounts] = useState<EditableAccount[]>([]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<"service" | "accounts">("service");
  const [selectedAccountId, setSelectedAccountId] = useState("");
  const [accountForm, setAccountForm] = useState<AccountForm>(emptyAccount);
  const [message, setMessage] = useState("");
  const [messageType, setMessageType] = useState<"success" | "error">(
    "success",
  );
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [toggling, setToggling] = useState(false);

  const activeAccounts = useMemo(
    () => accounts.filter((account) => account.enabled),
    [accounts],
  );
  const activeDraftAccounts = useMemo(
    () => draftAccounts.filter((account) => account.enabled),
    [draftAccounts],
  );
  const endpoint =
    status.endpoint ||
    `http://${settings.listenAddress.includes(":") ? `[${settings.listenAddress}]` : settings.listenAddress}:${settings.listenPort}/`;
  const serviceStatus =
    status.state === "running"
      ? "运行中"
      : status.state === "error"
        ? "异常"
        : "已停用";

  function applyConfig(config: WebDavConfigView) {
    const nextSettings = settingsFromConfig(config);
    const nextAccounts = config.accounts.map(accountFromView);
    setSettings(nextSettings);
    setDraft(nextSettings);
    setStatus(config.status || emptyStatus);
    setAccounts(nextAccounts);
    setDraftAccounts(nextAccounts);
  }

  function showMessage(text: string, type: "success" | "error" = "success") {
    setMessage(text);
    setMessageType(type);
    window.setTimeout(() => setMessage(""), 4200);
  }

  async function loadConfig() {
    setLoading(true);
    try {
      applyConfig(await invokeCommand<WebDavConfigView>("get_webdav_config"));
    } catch (error) {
      setStatus({ ...emptyStatus, state: "error", message: String(error) });
      showMessage(`读取 WebDAV 配置失败：${String(error)}`, "error");
    } finally {
      setLoading(false);
    }
  }

  function openSettings(tab: "service" | "accounts" = "service") {
    setMessage("");
    setDraft(settings);
    setDraftAccounts(accounts.map((account) => ({ ...account, password: "" })));
    const first = accounts[0];
    setSelectedAccountId(first?.id || "");
    setAccountForm(first ? { ...first, password: "" } : emptyAccount());
    setActiveTab(tab);
    setSettingsOpen(true);
  }

  async function saveSettings() {
    const listenPort = parsePort(draft.listenPort);
    if (!draft.rootPath.trim() && draft.enabled) {
      showMessage("请选择共享目录", "error");
      return;
    }
    if (listenPort === null) {
      showMessage("端口需为 1-65535 的数字", "error");
      return;
    }
    const payload: SaveWebDavConfigRequest = {
      enabled: draft.enabled,
      root_path: draft.rootPath.trim(),
      listen_addr: draft.listenAddress.trim() || "127.0.0.1",
      listen_port: listenPort,
      global_read_only: draft.globalReadOnly,
      accounts: draftAccounts.map((account) => ({
        id: account.id.startsWith("new-") ? "" : account.id,
        username: account.username.trim(),
        password: account.password,
        read_only: account.access === "read-only",
        enabled: account.enabled,
      })),
    };

    setSaving(true);
    try {
      const saved = await invokeCommand<WebDavConfigView>(
        "save_webdav_config",
        { request: payload },
      );
      applyConfig(saved);
      setSettingsOpen(false);
      showMessage(
        saved.status.state === "running"
          ? "WebDAV 服务配置已保存并生效"
          : saved.status.message,
      );
      notifyDataChanged();
    } catch (error) {
      showMessage(`保存 WebDAV 配置失败：${String(error)}`, "error");
    } finally {
      setSaving(false);
    }
  }

  async function toggleService(enabled: boolean) {
    setToggling(true);
    try {
      const saved = await invokeCommand<WebDavConfigView>(
        "set_webdav_enabled",
        { enabled },
      );
      applyConfig(saved);
      showMessage(enabled ? "WebDAV 服务已启用" : "WebDAV 服务已停用");
      notifyDataChanged();
    } catch (error) {
      showMessage(`切换 WebDAV 服务失败：${String(error)}`, "error");
    } finally {
      setToggling(false);
    }
  }

  async function chooseDirectory() {
    try {
      const selected = await invokeCommand<string | null>(
        "select_webdav_directory",
      );
      if (selected) setDraft((current) => ({ ...current, rootPath: selected }));
    } catch (error) {
      showMessage(`选择共享目录失败：${String(error)}`, "error");
    }
  }

  async function copyEndpoint() {
    try {
      await navigator.clipboard.writeText(endpoint);
      showMessage("访问地址已复制");
    } catch {
      showMessage(`访问地址：${endpoint}`);
    }
  }

  async function testConnection() {
    if (status.state !== "running" || testing) return;
    setTesting(true);
    try {
      const result = await invokeCommand<string>("test_webdav_service");
      showMessage(result);
      notifyDataChanged();
    } catch (error) {
      showMessage(`连接测试失败：${String(error)}`, "error");
    } finally {
      setTesting(false);
    }
  }

  function notifyDataChanged() {
    window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  }

  function selectAccount(account: EditableAccount) {
    setSelectedAccountId(account.id);
    setAccountForm({ ...account });
  }

  function startNewAccount() {
    const account = emptyAccount();
    setSelectedAccountId(account.id);
    setAccountForm(account);
  }

  function saveAccount() {
    const username = accountForm.username.trim();
    if (!username) {
      showMessage("请输入账号名称", "error");
      return;
    }
    if (
      !accountForm.passwordConfigured &&
      accountForm.enabled &&
      accountForm.password.length < 8
    ) {
      showMessage("新账号密码至少需要 8 位", "error");
      return;
    }
    if (accountForm.password && accountForm.password.length < 8) {
      showMessage("WebDAV 密码至少需要 8 位", "error");
      return;
    }
    if (
      draftAccounts.some(
        (account) =>
          account.id !== accountForm.id &&
          account.username.toLowerCase() === username.toLowerCase(),
      )
    ) {
      showMessage("账号名称已存在", "error");
      return;
    }

    const savedAccount: EditableAccount = {
      ...accountForm,
      username,
      passwordConfigured:
        accountForm.passwordConfigured || Boolean(accountForm.password),
    };
    setDraftAccounts((current) => {
      const exists = current.some((account) => account.id === savedAccount.id);
      return exists
        ? current.map((account) =>
            account.id === savedAccount.id ? savedAccount : account,
          )
        : [...current, savedAccount];
    });
    setSelectedAccountId(savedAccount.id);
    setAccountForm(savedAccount);
    showMessage(
      accountForm.id.startsWith("new-")
        ? "账号已加入待保存列表"
        : "账号修改已加入待保存列表",
    );
  }

  function deleteAccount(account: EditableAccount) {
    const next = draftAccounts.filter((item) => item.id !== account.id);
    setDraftAccounts(next);
    const fallback = next[0];
    setSelectedAccountId(fallback?.id || "");
    setAccountForm(fallback ? { ...fallback, password: "" } : emptyAccount());
    showMessage(`账号 ${account.username} 将在保存服务后删除`);
  }

  useEffect(() => {
    if (settingsOpen) return;
    void loadConfig();
    const timer = window.setInterval(() => void loadConfig(), 30000);
    return () => window.clearInterval(timer);
  }, [settingsOpen]);

  return (
    <>
      {message && !settingsOpen ? (
        <p className={`status-message msg-${messageType}`} role="status">
          {message}
        </p>
      ) : null}
      <div className="webdav-page-grid">
        <section className="panel webdav-panel">
          <header className="panel-header">
            <h2>WebDAV 服务</h2>
            <div className="toolbar">
              <span
                className={`state-label ${status.state === "running" ? "tone-success" : status.state === "error" ? "tone-error" : "tone-neutral"}`}
                title={status.message}
              >
                <span className="status-dot" />
                {serviceStatus}
              </span>
              <label className="toggle-switch">
                <input
                  type="checkbox"
                  aria-label="启用 WebDAV 服务"
                  checked={settings.enabled}
                  disabled={loading || toggling || saving}
                  onChange={(event) => toggleService(event.target.checked)}
                />
                <span className="toggle-slider" />
              </label>
            </div>
          </header>
          <div className="webdav-service-summary">
            <div className="binding-preview">
              <span className="text-muted">访问地址</span>
              <p className="mono">{endpoint}</p>
            </div>
            <dl className="detail-list">
              <div>
                <dt>共享目录</dt>
                <dd className="mono">
                  {settings.rootPath || "尚未选择共享目录"}
                </dd>
              </div>
              <div>
                <dt>监听地址</dt>
                <dd className="mono">{settings.listenAddress}</dd>
              </div>
              <div>
                <dt>监听端口</dt>
                <dd className="mono">{settings.listenPort}</dd>
              </div>
              <div>
                <dt>服务模式</dt>
                <dd>{settings.globalReadOnly ? "全局只读" : "按账号权限"}</dd>
              </div>
            </dl>
            <div className="toolbar">
              <button className="btn btn-secondary" onClick={copyEndpoint}>
                <Copy size={14} />
                复制地址
              </button>
              <button
                className="btn btn-secondary"
                disabled={status.state !== "running" || testing}
                onClick={testConnection}
              >
                <Activity size={14} />
                {testing ? "正在测试" : "测试连接"}
              </button>
              <button
                className="btn btn-primary"
                disabled={loading || toggling}
                onClick={() => openSettings()}
              >
                <Settings2 size={14} />
                服务设置
              </button>
            </div>
          </div>
        </section>
        <div>
          <section className="panel">
            <header className="panel-header">
              <h2>
                访问账号 <small>{activeAccounts.length} 已启用</small>
              </h2>
              <button
                className="text-button"
                disabled={loading || toggling}
                onClick={() => openSettings("accounts")}
              >
                <Users size={14} />
                账号管理
              </button>
            </header>
            {!accounts.length ? (
              <div className="empty-state">暂无访问账号，请先添加账号。</div>
            ) : (
              accounts.map((account) => (
                <button
                  type="button"
                  className="service-row"
                  key={account.id}
                  disabled={loading || toggling}
                  onClick={() => {
                    openSettings("accounts");
                    selectAccount(account);
                  }}
                >
                  <span className="webdav-account-avatar">
                    {account.username.slice(0, 1).toUpperCase()}
                  </span>
                  <span className="service-copy">
                    <strong>{account.username}</strong>
                    <small>
                      {account.access === "read-write"
                        ? "读写权限"
                        : "只读权限"}{" "}
                      · {accountLastUsed(account)}
                    </small>
                  </span>
                  <span
                    className={`badge ${account.enabled ? "" : "badge-neutral"}`}
                  >
                    {account.enabled ? "已启用" : "已停用"}
                  </span>
                  <Pencil size={14} />
                </button>
              ))
            )}
          </section>
          <div className="webdav-security-note">
            <ShieldCheck size={20} />
            <div>
              <strong>需要从公网访问？</strong>
              <p>使用 HTTPS 反向代理提供加密入口，并保留本机监听地址。</p>
              <button className="text-button" onClick={onOpenProxy}>
                配置反向代理
              </button>
            </div>
          </div>
        </div>
      </div>

      {settingsOpen ? (
        <EditorDrawer
          title="WebDAV 服务设置"
          onClose={() => setSettingsOpen(false)}
          busy={saving}
          wide
          footer={
            <>
              <button
                className="btn btn-secondary"
                disabled={saving}
                onClick={() => setSettingsOpen(false)}
              >
                取消
              </button>
              <button
                className="btn btn-primary"
                disabled={saving}
                onClick={saveSettings}
              >
                {saving ? "正在保存" : "保存服务"}
              </button>
            </>
          }
        >
          <nav className="webdav-tabs" aria-label="WebDAV 设置分类">
            <button
              className={activeTab === "service" ? "is-active" : ""}
              type="button"
              onClick={() => setActiveTab("service")}
            >
              <Settings2 size={13} />
              服务设置
            </button>
            <button
              className={activeTab === "accounts" ? "is-active" : ""}
              type="button"
              onClick={() => setActiveTab("accounts")}
            >
              <Users size={13} />
              账号管理<span>{draftAccounts.length}</span>
            </button>
          </nav>

          {message ? (
            <p
              role="status"
              className={`webdav-modal-message is-${messageType}`}
            >
              {message}
            </p>
          ) : null}

          {activeTab === "service" ? (
            <div className="webdav-service-editor">
              <div className="webdav-editor-grid">
                <label className="field-label">
                  <span>共享目录</span>
                  <div className="field-control webdav-path-control">
                    <input
                      value={draft.rootPath}
                      placeholder="选择需要共享的本地目录"
                      onChange={(event) =>
                        setDraft((current) => ({
                          ...current,
                          rootPath: event.target.value,
                        }))
                      }
                    />
                    <button
                      className="btn btn-secondary"
                      type="button"
                      onClick={chooseDirectory}
                    >
                      <FolderOpen size={12} />
                      选择
                    </button>
                    <small>所有账号访问同一个根目录，符号链接不会暴露。</small>
                  </div>
                </label>
                <label className="field-label">
                  <span>监听范围</span>
                  <div className="field-control">
                    <select
                      value={draft.listenAddress}
                      onChange={(event) =>
                        setDraft((current) => ({
                          ...current,
                          listenAddress: event.target.value,
                        }))
                      }
                    >
                      <option value="127.0.0.1">仅本机（推荐）</option>
                      <option value="0.0.0.0">局域网 IPv4</option>
                      <option value="::">全部 IPv6 接口</option>
                    </select>
                    <small>
                      公网访问建议保持仅本机，通过 HTTPS 反向代理接入。
                    </small>
                  </div>
                </label>
                <label className="field-label">
                  <span>监听端口</span>
                  <div className="field-control">
                    <input
                      value={draft.listenPort}
                      inputMode="numeric"
                      onChange={(event) =>
                        setDraft((current) => ({
                          ...current,
                          listenPort: event.target.value.trim(),
                        }))
                      }
                    />
                    <small>可配置 1-65535，默认使用 19080。</small>
                  </div>
                </label>
                <label className="field-label">
                  <span>服务模式</span>
                  <div className="field-control">
                    <select
                      value={draft.globalReadOnly ? "read-only" : "account"}
                      onChange={(event) =>
                        setDraft((current) => ({
                          ...current,
                          globalReadOnly: event.target.value === "read-only",
                        }))
                      }
                    >
                      <option value="account">按账号权限</option>
                      <option value="read-only">全局只读</option>
                    </select>
                    <small>全局只读开启后，读写账号也不能修改文件。</small>
                  </div>
                </label>
              </div>
              <div className="webdav-security-note">
                <ShieldCheck size={17} strokeWidth={2.1} />
                <div>
                  <strong>推荐使用 HTTPS</strong>
                  <p>
                    Basic Auth 在 HTTP
                    下不会加密密码。外网访问请将本服务接入现有 HTTPS 反向代理。
                  </p>
                </div>
              </div>
            </div>
          ) : (
            <div className="webdav-account-editor">
              <section className="webdav-account-list">
                <header>
                  <div>
                    <strong>访问账号</strong>
                    <span>
                      {activeDraftAccounts.length} 个启用，共{" "}
                      {draftAccounts.length} 个
                    </span>
                  </div>
                  <button
                    className="btn btn-primary"
                    type="button"
                    onClick={startNewAccount}
                  >
                    <UserPlus size={12} strokeWidth={2.2} />
                    添加账号
                  </button>
                </header>
                <div className="webdav-account-rows">
                  {draftAccounts.length === 0 ? (
                    <p className="webdav-empty-accounts">
                      暂无账号，请先添加访问账号。
                    </p>
                  ) : null}
                  {draftAccounts.map((account) => (
                    <button
                      key={account.id}
                      className={`webdav-account-row ${selectedAccountId === account.id ? "is-selected" : ""}`}
                      type="button"
                      onClick={() => selectAccount(account)}
                    >
                      <span
                        className={`webdav-account-avatar ${account.enabled ? "" : "is-disabled"}`}
                      >
                        {account.username.slice(0, 1).toUpperCase()}
                      </span>
                      <span className="webdav-account-main">
                        <strong>{account.username}</strong>
                        <small>{accountLastUsed(account)}</small>
                      </span>
                      <span
                        className={`webdav-access-label is-${account.access}`}
                      >
                        {account.access === "read-write" ? "读写" : "只读"}
                      </span>
                      {!account.enabled ? (
                        <span className="webdav-disabled-label">已停用</span>
                      ) : null}
                    </button>
                  ))}
                </div>
              </section>

              <section className="webdav-account-form">
                <header>
                  <div>
                    {accountForm.id.startsWith("new-") ? (
                      <UserPlus size={14} />
                    ) : (
                      <Pencil size={14} />
                    )}
                    <strong>
                      {accountForm.id.startsWith("new-")
                        ? "添加账号"
                        : "编辑账号"}
                    </strong>
                  </div>
                  {!accountForm.id.startsWith("new-") ? (
                    <button
                      className="icon-action webdav-delete-account"
                      type="button"
                      title="删除账号"
                      onClick={() => deleteAccount(accountForm)}
                    >
                      <Trash2 size={14} />
                    </button>
                  ) : null}
                </header>
                <div className="webdav-account-fields">
                  <label className="field-label">
                    <span>账号名称</span>
                    <input
                      value={accountForm.username}
                      placeholder="例如 backup"
                      onChange={(event) =>
                        setAccountForm((current) => ({
                          ...current,
                          username: event.target.value,
                        }))
                      }
                    />
                  </label>
                  <label className="field-label">
                    <span>
                      {accountForm.passwordConfigured ? "重设密码" : "登录密码"}
                    </span>
                    <div className="webdav-password-control">
                      <KeyRound size={13} />
                      <input
                        value={accountForm.password}
                        type="password"
                        placeholder={
                          accountForm.passwordConfigured
                            ? "留空则保持原密码"
                            : "至少 8 位"
                        }
                        onChange={(event) =>
                          setAccountForm((current) => ({
                            ...current,
                            password: event.target.value,
                          }))
                        }
                      />
                    </div>
                  </label>
                  <label className="field-label">
                    <span>访问权限</span>
                    <select
                      value={accountForm.access}
                      onChange={(event) =>
                        setAccountForm((current) => ({
                          ...current,
                          access: event.target.value as AccountAccess,
                        }))
                      }
                    >
                      <option value="read-write">可读写</option>
                      <option value="read-only">只读</option>
                    </select>
                  </label>
                  <div className="webdav-account-enabled">
                    <span>
                      <strong>启用此账号</strong>
                      <small>停用后会立即拒绝新的认证请求。</small>
                    </span>
                    <label className="toggle-switch">
                      <input
                        type="checkbox"
                        aria-label="启用此账号"
                        checked={accountForm.enabled}
                        onChange={(event) =>
                          setAccountForm((current) => ({
                            ...current,
                            enabled: event.target.checked,
                          }))
                        }
                      />
                      <span className="toggle-slider" />
                    </label>
                  </div>
                </div>
                <button
                  className="btn btn-primary webdav-save-account"
                  type="button"
                  onClick={saveAccount}
                >
                  保存账号修改
                </button>
              </section>
            </div>
          )}

          <p className="form-note">
            <ShieldCheck size={14} />
            密码仅保存 Argon2id 哈希，界面不会回显。
          </p>
        </EditorDrawer>
      ) : null}
    </>
  );
}
