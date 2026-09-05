import { useEffect, useState } from "react";
import {
  ArrowLeftRight,
  ArrowUpRight,
  Check,
  ChevronRight,
  Cloud,
  Copy,
  FolderOpen,
  Globe,
  Info,
  MonitorSmartphone,
  RefreshCw,
  ShieldCheck,
  Waypoints,
  Workflow,
} from "lucide-react";
import { invokeCommand } from "../lib/tauri";
import type {
  AppPage,
  DeviceDdnsConfig,
  LogEntry,
  RuntimeStatus,
  WebDavConfigView,
} from "../types";
import { formatSyncTime } from "../utils/time";
import { statusTone } from "../utils/presentation";
import PageHeading from "./PageHeading";

interface NetworkOverviewProps {
  status: RuntimeStatus;
  available: boolean;
  devices: { online: number; total: number; available: boolean };
  bindings: DeviceDdnsConfig[] | null;
  onNavigate: (page: AppPage) => void;
  onRefresh: () => Promise<void>;
}

export default function NetworkOverview({
  status,
  available,
  devices,
  bindings,
  onNavigate,
  onRefresh,
}: NetworkOverviewProps) {
  const [webdav, setWebdav] = useState<WebDavConfigView | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [logsAvailable, setLogsAvailable] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [copyMessage, setCopyMessage] = useState("");
  async function loadServices() {
    const [service, entries] = await Promise.allSettled([
      invokeCommand<WebDavConfigView>("get_webdav_config"),
      invokeCommand<LogEntry[]>("get_recent_logs"),
    ]);
    setWebdav(service.status === "fulfilled" ? service.value : null);
    setLogs(
      entries.status === "fulfilled"
        ? entries.value.slice().reverse().slice(0, 3)
        : [],
    );
    setLogsAvailable(entries.status === "fulfilled");
  }
  useEffect(() => {
    void loadServices();
    const timer = window.setInterval(loadServices, 30000);
    window.addEventListener("homenet:logs-refresh", loadServices);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("homenet:logs-refresh", loadServices);
    };
  }, []);
  async function refresh() {
    setRefreshing(true);
    try {
      await Promise.all([onRefresh(), loadServices()]);
    } finally {
      setRefreshing(false);
    }
  }
  async function copyAddress() {
    try {
      await navigator.clipboard.writeText(status.public_ipv6);
      setCopyMessage("IPv6 地址已复制");
    } catch {
      setCopyMessage("复制失败，请选中地址手动复制");
    }
  }
  const hasV4 = available && Boolean(status.public_ipv4.trim());
  const hasV6 = available && Boolean(status.public_ipv6.trim());
  const enabledBindings = bindings?.filter((binding) => binding.enabled).length;
  const services = [
    {
      name: "阿里云 DDNS",
      icon: Cloud,
      page: "devices" as const,
      description: bindings
        ? `${enabledBindings} 个设备绑定已启用 · 最近同步 ${formatSyncTime(status.last_update_time, "暂无")}`
        : "暂时无法读取绑定信息",
      state: available ? status.ddns_status || "未连接" : "无法读取",
    },
    {
      name: "反向代理",
      icon: ShieldCheck,
      page: "proxy" as const,
      description: available
        ? `${status.reverse_proxy_rule_count} 个域名入口 · ${status.enabled_reverse_proxy_rule_count} 条已启用`
        : "暂时无法读取代理状态",
      state: available
        ? status.enabled_reverse_proxy_rule_count > 0
          ? "已启用"
          : "未启用"
        : "无法读取",
    },
    {
      name: "WebDAV 文件服务",
      icon: FolderOpen,
      page: "webdav" as const,
      description: webdav
        ? `${webdav.status.endpoint || "尚未启动"} · ${webdav.accounts.filter((account) => account.enabled).length} 个已启用账号`
        : "暂时无法读取服务状态",
      state: !webdav
        ? "无法读取"
        : webdav.status.state === "running"
          ? "运行中"
          : webdav.status.state === "error"
            ? "异常"
            : "已停用",
    },
  ];
  const stats = [
    {
      label: "在线设备",
      value: devices.available ? devices.online : "--",
      total: devices.available ? devices.total : undefined,
      caption: devices.available
        ? `${devices.total - devices.online} 台设备离线`
        : "等待设备扫描",
      icon: MonitorSmartphone,
      page: "devices" as const,
    },
    {
      label: "DDNS 绑定",
      value: bindings?.length ?? "--",
      caption: bindings ? `${enabledBindings} 个绑定已启用` : "暂时无法读取",
      icon: Cloud,
      page: "devices" as const,
    },
    {
      label: "端口转发",
      value: available ? status.enabled_rule_count : "--",
      total: available ? status.rule_count : undefined,
      caption: available
        ? `${status.enabled_rule_count} 条已启用`
        : "等待服务连接",
      icon: ArrowLeftRight,
      page: "forward" as const,
    },
    {
      label: "反向代理",
      value: available ? status.enabled_reverse_proxy_rule_count : "--",
      total: available ? status.reverse_proxy_rule_count : undefined,
      caption: available
        ? `${status.enabled_reverse_proxy_rule_count} 条已启用`
        : "等待服务连接",
      icon: Workflow,
      page: "proxy" as const,
    },
  ];
  return (
    <>
      <PageHeading
        eyebrow="WORKSPACE / OVERVIEW"
        title="网络总览"
        description="连接、设备与服务，一目了然。"
        actions={
          <button
            className="btn btn-secondary"
            disabled={refreshing}
            onClick={refresh}
          >
            <RefreshCw size={15} className={refreshing ? "spinning" : ""} />
            刷新状态
          </button>
        }
      />
      {!available ? (
        <p className="status-message msg-error" role="status">
          暂时无法连接本机服务，请稍后刷新。
        </p>
      ) : null}
      <section className="network-hero" aria-label="公网与本机服务连接概览">
        <div className="network-addresses">
          <div className="connection-state">
            <span
              className={`status-dot ${hasV4 || hasV6 ? "" : "is-muted"}`}
            />
            <span>
              {hasV4 || hasV6 ? "公网地址已获取" : "等待获取公网地址"}
            </span>
            <small>
              {hasV4 && hasV6
                ? "双栈网络"
                : hasV4
                  ? "IPv4 网络"
                  : hasV6
                    ? "IPv6 网络"
                    : "未连接"}
            </small>
          </div>
          <div className="address-label">
            <span>公网 IPv4</span>
            <span className="badge">IPv4</span>
          </div>
          <p className="public-address ipv4-address">
            {hasV4 ? status.public_ipv4 : "--"}
          </p>
          <div className="ipv6-address-row">
            <div>
              <div className="address-label">公网 IPv6</div>
              <p className="public-address ipv6-address">
                {hasV6 ? status.public_ipv6 : "--"}
              </p>
            </div>
            <button
              className="icon-action"
              onClick={copyAddress}
              disabled={!hasV6}
              aria-label="复制公网 IPv6 地址"
            >
              <Copy size={16} />
            </button>
          </div>
          {copyMessage ? (
            <p className="field-hint" role="status">
              {copyMessage}
            </p>
          ) : null}
        </div>
        <div className="network-topology">
          <div className="topology-caption">
            <span>连接概览</span>
            <span>
              {hasV4 && hasV6
                ? "IPv4 + IPv6"
                : hasV6
                  ? "IPv6"
                  : hasV4
                    ? "IPv4"
                    : "等待连接"}
            </span>
          </div>
          <div className={`topology-nodes ${!available ? "is-muted" : ""}`}>
            <div className="topology-node">
              <div className="topology-icon">
                <Globe />
              </div>
              <span>公网入口</span>
            </div>
            <div className="topology-line" />
            <div className="topology-node is-center">
              <div className="topology-icon">
                <Waypoints />
              </div>
              <span>HomeNet</span>
            </div>
            <div className="topology-line" />
            <div className="topology-node">
              <div className="topology-icon">
                <MonitorSmartphone />
              </div>
              <span>本地设备</span>
            </div>
          </div>
          <p className="topology-description">域名解析 · 端口转发 · 反向代理</p>
        </div>
      </section>
      <div className="overview-stats">
        {stats.map(({ label, value, total, caption, icon: Icon, page }) => (
          <button
            className="overview-stat"
            key={label}
            onClick={() => onNavigate(page)}
          >
            <div className="stat-label">
              {label}
              <Icon size={17} />
            </div>
            <div className="stat-value">
              {value}
              {total !== undefined ? <small>/ {total}</small> : null}
            </div>
            <p>{caption}</p>
          </button>
        ))}
      </div>
      <div className="overview-bottom">
        <section className="panel">
          <header className="panel-header">
            <h2>连接服务</h2>
            <span className="text-muted">3 项服务</span>
          </header>
          {services.map(({ name, description, icon: Icon, state, page }) => (
            <button
              key={name}
              className="service-row"
              onClick={() => onNavigate(page)}
            >
              <span className="service-icon">
                <Icon size={19} />
              </span>
              <span className="service-copy">
                <strong>{name}</strong>
                <small>{description}</small>
              </span>
              <span className={`state-label tone-${statusTone(state)}`}>
                <span className="status-dot" />
                {state}
              </span>
              <ChevronRight size={15} />
            </button>
          ))}
        </section>
        <section className="panel">
          <header className="panel-header">
            <h2>最近动态</h2>
            <button className="text-button" onClick={() => onNavigate("logs")}>
              全部日志
              <ArrowUpRight size={14} />
            </button>
          </header>
          <div className="recent-events">
            {logs.length ? (
              logs.map((entry) => (
                <div className="recent-event" key={entry.id}>
                  <span
                    className={`event-mark tone-${statusTone(entry.level.toUpperCase() === "INFO" ? "成功" : entry.level)}`}
                  >
                    {entry.level.toUpperCase() === "INFO" ? (
                      <Check size={12} />
                    ) : (
                      <Info size={12} />
                    )}
                  </span>
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
                {logsAvailable ? "暂无运行日志" : "暂时无法读取日志"}
              </div>
            )}
          </div>
        </section>
      </div>
    </>
  );
}
