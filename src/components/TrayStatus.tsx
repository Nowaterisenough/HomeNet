import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowLeftRight,
  ArrowUpRight,
  Check,
  ChevronRight,
  Cloud,
  Copy,
  FolderOpen,
  RefreshCw,
  ScrollText,
  Settings,
  Waypoints,
  Workflow,
} from "lucide-react";
import { currentTauriWindow, invokeCommand } from "../lib/tauri";
import { useTheme } from "../hooks/useTheme";
import type { AppPage, ServiceSummary, TraySnapshot } from "../types";
import { formatUptime, statusTone } from "../utils/presentation";
import { formatSyncTime } from "../utils/time";

type TrayTab = "overview" | "services" | "logs";
const tabs = [
  { id: "overview", label: "网络总览" },
  { id: "services", label: "连接服务" },
  { id: "logs", label: "最近动态" },
] as const;

function ServiceRow({
  service,
  onOpen,
}: {
  service: ServiceSummary;
  onOpen: (page: AppPage) => void;
}) {
  const Icon =
    service.page === "devices"
      ? Cloud
      : service.page === "forward"
        ? ArrowLeftRight
        : service.page === "proxy"
          ? Workflow
          : FolderOpen;
  return (
    <button className="tray-service" onClick={() => onOpen(service.page)}>
      <span className="tray-service-icon">
        <Icon size={18} />
      </span>
      <span className="tray-service-copy">
        <strong>{service.name}</strong>
        <small>{service.detail}</small>
        <span
          className={`state-label tone-${service.enabled ? statusTone(service.status) : "neutral"}`}
        >
          <span className="status-dot" />
          {service.status}
        </span>
      </span>
      <ChevronRight size={14} />
    </button>
  );
}

export default function TrayStatus() {
  useTheme();
  const [tab, setTab] = useState<TrayTab>("overview");
  const [snapshot, setSnapshot] = useState<TraySnapshot | null>(null);
  const [active, setActive] = useState(!currentTauriWindow());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [actionError, setActionError] = useState("");
  const [updatedAt, setUpdatedAt] = useState("");
  const [copied, setCopied] = useState("");
  const [onlyEnabled, setOnlyEnabled] = useState(false);
  const busy = useRef(false);
  const alive = useRef(false);

  const refresh = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    setLoading(true);
    try {
      const next = await invokeCommand<TraySnapshot>("get_tray_snapshot");
      if (!alive.current) return;
      setSnapshot(next);
      setUpdatedAt(new Date().toLocaleTimeString("zh-CN", { hour12: false }));
      setError("");
    } catch {
      if (alive.current)
        setError("暂时无法读取状态，请重试。已有数据为上次刷新结果。");
    } finally {
      busy.current = false;
      if (alive.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    const appWindow = currentTauriWindow();
    let disposed = false;
    let unlisten: (() => void) | undefined;
    if (appWindow) {
      void appWindow
        .onFocusChanged(({ payload }) => {
          if (!disposed) setActive(payload);
        })
        .then(async (stop) => {
          if (disposed) {
            stop();
            return;
          }
          unlisten = stop;
          const visible = await appWindow.isVisible();
          if (!disposed) setActive(visible);
        })
        .catch(() => {
          if (!disposed) setError("无法监听窗口状态，请手动刷新。");
        });
    }
    function escape(event: KeyboardEvent) {
      if (event.key === "Escape") {
        void appWindow
          ?.hide()
          .catch(() => setActionError("收起失败，请点击窗口外部重试。"));
      }
    }
    document.addEventListener("keydown", escape);
    return () => {
      alive.current = false;
      disposed = true;
      unlisten?.();
      document.removeEventListener("keydown", escape);
    };
  }, []);

  useEffect(() => {
    if (!active) return;
    void refresh();
    const timer = window.setInterval(refresh, 15000);
    return () => window.clearInterval(timer);
  }, [active, refresh]);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(""), 2000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  async function openPage(page: AppPage = "overview") {
    setActionError("");
    try {
      await invokeCommand("open_main_window", { page });
    } catch {
      setActionError("无法打开主窗口，请通过托盘菜单重试。");
    }
  }

  async function copyAddress(value: string) {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(value);
    } catch {
      setActionError("复制失败，请选中地址手动复制。");
    }
  }

  async function quit() {
    try {
      await invokeCommand("quit_app");
    } catch {
      setActionError("退出失败，请通过托盘菜单重试。");
    }
  }

  const status = snapshot?.runtime;
  const enabledServices =
    snapshot?.services.filter((service) => service.enabled) ?? [];
  const displayedServices = onlyEnabled
    ? enabledServices
    : (snapshot?.services ?? []);
  const metrics = [
    {
      label: "设备 DDNS",
      value: snapshot?.enabled_binding_count,
      total: snapshot?.binding_count,
      page: "devices" as const,
    },
    {
      label: "端口转发",
      value: status?.enabled_rule_count,
      total: status?.rule_count,
      page: "forward" as const,
    },
    {
      label: "反向代理",
      value: status?.enabled_reverse_proxy_rule_count,
      total: status?.reverse_proxy_rule_count,
      page: "proxy" as const,
    },
  ];

  return (
    <div className="tray-shell">
      <header className="tray-header">
        <div className="app-brand">
          <Waypoints size={22} />
          <strong>HomeNet</strong>
        </div>
        <div className="titlebar-actions">
          <button
            className="chrome-action"
            aria-label="刷新状态"
            title="刷新状态"
            disabled={loading}
            onClick={refresh}
          >
            <RefreshCw size={16} className={loading ? "spinning" : ""} />
          </button>
          <button
            className="chrome-action"
            aria-label="打开主窗口"
            title="打开主窗口"
            onClick={() => openPage()}
          >
            <ArrowUpRight size={18} />
          </button>
          <button
            className="chrome-action"
            aria-label="偏好设置"
            title="偏好设置"
            onClick={() => openPage("settings")}
          >
            <Settings size={16} />
          </button>
        </div>
      </header>
      <div className="tray-tabs" role="tablist" aria-label="状态视图">
        {tabs.map((item) => (
          <button
            key={item.id}
            id={`tab-${item.id}`}
            role="tab"
            aria-selected={tab === item.id}
            aria-controls="tray-panel"
            tabIndex={tab === item.id ? 0 : -1}
            onClick={() => setTab(item.id)}
            onKeyDown={(event) => {
              if (
                !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)
              )
                return;
              event.preventDefault();
              const index = tabs.findIndex((entry) => entry.id === tab);
              const next =
                event.key === "Home"
                  ? 0
                  : event.key === "End"
                    ? tabs.length - 1
                    : (index +
                        (event.key === "ArrowRight" ? 1 : -1) +
                        tabs.length) %
                      tabs.length;
              setTab(tabs[next].id);
              document.getElementById(`tab-${tabs[next].id}`)?.focus();
            }}
          >
            {item.label}
          </button>
        ))}
      </div>
      <main
        className="tray-content"
        id="tray-panel"
        role="tabpanel"
        aria-labelledby={`tab-${tab}`}
        tabIndex={0}
      >
        <div className="tray-connection">
          <span>
            <span
              className={`status-dot ${!snapshot || error ? "is-muted" : ""}`}
            />
            {error
              ? "状态未更新"
              : snapshot
                ? "本机服务已连接"
                : loading
                  ? "正在读取状态"
                  : "等待连接"}
          </span>
          <small>
            {loading
              ? "刷新中…"
              : updatedAt
                ? `${updatedAt} 更新`
                : "每 15 秒刷新"}
          </small>
        </div>
        {error || actionError ? (
          <p className="status-message msg-error" role="status">
            {actionError || error}
          </p>
        ) : null}
        {tab === "overview" ? (
          <>
            <section className="tray-metrics" aria-label="运行概览">
              {metrics.map((metric) => (
                <button
                  key={metric.label}
                  onClick={() => openPage(metric.page)}
                >
                  <span>{metric.label}</span>
                  <strong>
                    {metric.value ?? "—"}
                    <small>
                      {metric.total === undefined ? "" : ` / ${metric.total}`}
                    </small>
                  </strong>
                  <small>已启用 / 全部</small>
                </button>
              ))}
            </section>
            <section className="panel tray-addresses" aria-label="网络地址">
              <header className="tray-section-header">
                <h2>网络地址</h2>
                <span className="badge badge-neutral">
                  {status?.public_ipv4 && status?.public_ipv6
                    ? "双栈"
                    : status?.public_ipv6
                      ? "IPv6"
                      : status?.public_ipv4
                        ? "IPv4"
                        : "等待获取"}
                </span>
              </header>
              {[
                ["公网 IPv4", status?.public_ipv4],
                ["公网 IPv6", status?.public_ipv6],
              ].map(([label, address]) => (
                <div className="tray-address" key={label}>
                  <div>
                    <small>{label}</small>
                    <p className="mono">{address || "尚未获取"}</p>
                  </div>
                  <button
                    className="chrome-action"
                    aria-label={`复制${label}`}
                    title={`复制${label}`}
                    disabled={!address}
                    onClick={() => address && copyAddress(address)}
                  >
                    {copied === address ? (
                      <Check size={14} />
                    ) : (
                      <Copy size={14} />
                    )}
                  </button>
                </div>
              ))}
              <p className="tray-copy-feedback" role="status">
                {copied ? "地址已复制" : ""}
              </p>
            </section>
            <section className="panel tray-summary">
              <button onClick={() => openPage("devices")}>
                <Cloud size={17} />
                <span>DDNS 状态</span>
                <small
                  className={`tone-${statusTone(status?.ddns_status || "")}`}
                >
                  {status?.ddns_status || "尚未获取"}
                </small>
                <ChevronRight size={13} />
              </button>
              <button onClick={() => setTab("services")}>
                <Waypoints size={17} />
                <span>连接服务</span>
                <small>
                  {snapshot ? `${enabledServices.length} 项已启用` : "尚未获取"}
                </small>
                <ChevronRight size={13} />
              </button>
              <div>
                <RefreshCw size={16} />
                <span>持续运行</span>
                <small>{status ? formatUptime(status.uptime) : "—"}</small>
              </div>
            </section>
            <p className="tray-note">关闭主窗口后，网络服务仍在后台运行。</p>
          </>
        ) : tab === "services" ? (
          <>
            <div className="tray-filter-row">
              <div className="filter-tabs">
                <button
                  aria-pressed={!onlyEnabled}
                  onClick={() => setOnlyEnabled(false)}
                >
                  全部 {snapshot?.services.length ?? ""}
                </button>
                <button
                  aria-pressed={onlyEnabled}
                  onClick={() => setOnlyEnabled(true)}
                >
                  已启用 {snapshot ? enabledServices.length : ""}
                </button>
              </div>
              <span>
                点击管理 <ArrowUpRight size={12} />
              </span>
            </div>
            <section className="panel">
              {displayedServices.length ? (
                displayedServices.map((service) => (
                  <ServiceRow
                    key={service.id}
                    service={service}
                    onOpen={openPage}
                  />
                ))
              ) : (
                <div className="empty-state">
                  {snapshot ? "暂无已启用的服务" : "等待服务状态"}
                </div>
              )}
            </section>
          </>
        ) : (
          <section className="panel">
            <header className="tray-section-header">
              <h2>最近日志</h2>
              <button className="text-button" onClick={() => openPage("logs")}>
                查看全部 <ArrowUpRight size={13} />
              </button>
            </header>
            {snapshot?.logs.length ? (
              snapshot.logs.map((entry) => (
                <div className="tray-log" key={entry.id}>
                  <ScrollText
                    size={15}
                    className={`level-${entry.level.toLowerCase()}`}
                  />
                  <div>
                    <div className="event-heading">
                      <strong>{entry.module}</strong>
                      <time>{formatSyncTime(entry.time)}</time>
                    </div>
                    <p>{entry.message}</p>
                  </div>
                </div>
              ))
            ) : (
              <div className="empty-state">
                {snapshot ? "暂无运行日志" : "等待日志数据"}
              </div>
            )}
          </section>
        )}
      </main>
      <footer className="tray-footer">
        <span>{status ? `v${status.version}` : "HomeNet"}</span>
        <button onClick={() => openPage()}>打开 HomeNet</button>
        <button title="退出应用并停止所有网络服务" onClick={quit}>
          退出
        </button>
      </footer>
    </div>
  );
}
