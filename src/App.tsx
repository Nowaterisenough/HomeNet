import { MouseEvent, useEffect, useMemo, useState } from "react";
import { Menu } from "lucide-react";
import DeviceDdnsPanel from "./components/DeviceDdnsPanel";
import ForwardRulesPanel from "./components/ForwardRulesPanel";
import LogPanel from "./components/LogPanel";
import ReverseProxyPanel from "./components/ReverseProxyPanel";
import RuntimeSettingsPanel from "./components/RuntimeSettingsPanel";
import StatusCard from "./components/StatusCard";
import {
  closeCurrentWindow,
  invokeCommand,
  minimizeCurrentWindow,
  startCurrentWindowDrag,
  toggleMaximizeCurrentWindow,
} from "./lib/tauri";
import type { AppUpdateResult, LanDevice, RuntimeStatus } from "./types";

const emptyStatus: RuntimeStatus = {
  version: "",
  public_ipv4: "",
  public_ipv6: "",
  ddns_status: "",
  last_update_time: "",
  rule_count: 0,
  enabled_rule_count: 0,
  reverse_proxy_rule_count: 0,
  enabled_reverse_proxy_rule_count: 0,
  uptime: 0,
};

function displayValue(value: string): string {
  return value?.trim() ? value : "--";
}

function ipv6PrefixSubtitle(value: string): string {
  const parts = value.split(":");
  if (!value.trim() || parts.length < 4) {
    return "前缀：--";
  }
  return `前缀：${parts.slice(0, 4).join(":")}::/64`;
}

export default function App() {
  const [statusData, setStatusData] = useState<RuntimeStatus>({ ...emptyStatus });
  const [deviceSummary, setDeviceSummary] = useState({ online: 0, total: 0 });
  const [autoStartEnabled, setAutoStartEnabled] = useState(false);
  const [autoStartSaving, setAutoStartSaving] = useState(false);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateStatusMessage, setUpdateStatusMessage] = useState("");
  const [updateStatusType, setUpdateStatusType] = useState<"normal" | "success" | "error">(
    "normal",
  );

  const disabledRuleCount = Math.max(0, statusData.rule_count - statusData.enabled_rule_count);
  const disabledReverseProxyCount = Math.max(
    0,
    statusData.reverse_proxy_rule_count - statusData.enabled_reverse_proxy_rule_count,
  );

  function ddnsStatusType(): "normal" | "warning" | "error" | "success" {
    if (statusData.ddns_status === "运行中") return "success";
    if (statusData.ddns_status === "未连接") return "warning";
    return "normal";
  }

  const statusCards = useMemo(
    () => [
      {
        title: "公网 IPv4",
        value: displayValue(statusData.public_ipv4),
        subtitle: "运营商：中国电信",
        icon: "globe",
        status: "normal" as const,
      },
      {
        title: "公网 IPv6",
        value: statusData.public_ipv6.trim() ? `${displayValue(statusData.public_ipv6)}/64` : "--",
        subtitle: ipv6PrefixSubtitle(statusData.public_ipv6),
        icon: "ipv6",
        status: "normal" as const,
      },
      {
        title: "DDNS 状态",
        value: statusData.ddns_status || "未连接",
        subtitle: `最后同步：${statusData.last_update_time || "暂无"}`,
        icon: "shield",
        status: ddnsStatusType(),
      },
      {
        title: "转发规则数",
        value: String(statusData.rule_count),
        subtitle: `启用：${statusData.enabled_rule_count}  禁用：${disabledRuleCount}`,
        icon: "rules",
        status: "normal" as const,
      },
      {
        title: "反向代理数",
        value: String(statusData.reverse_proxy_rule_count),
        subtitle: `启用：${statusData.enabled_reverse_proxy_rule_count}  禁用：${disabledReverseProxyCount}`,
        icon: "proxy",
        status: "normal" as const,
      },
      {
        title: "在线设备 / 发现设备",
        value: `${deviceSummary.online} / ${deviceSummary.total}`,
        subtitle: "局域网扫描结果",
        icon: "devices",
        status: "normal" as const,
      },
    ],
    [deviceSummary.online, deviceSummary.total, disabledReverseProxyCount, disabledRuleCount, statusData],
  );

  async function loadRuntimeStatus() {
    try {
      const data = await invokeCommand<RuntimeStatus>("get_runtime_status");
      setStatusData({ ...emptyStatus, ...data });
    } catch {
      setStatusData({ ...emptyStatus });
    }
  }

  async function loadDeviceSummary() {
    try {
      const devices = await invokeCommand<LanDevice[]>("list_lan_devices");
      setDeviceSummary({
        online: devices.filter((device) => device.online).length,
        total: devices.length,
      });
    } catch {
      setDeviceSummary({ online: 0, total: 0 });
    }
  }

  async function loadAutoStart() {
    try {
      setAutoStartEnabled(await invokeCommand<boolean>("get_auto_start"));
    } catch {
      setAutoStartEnabled(false);
    }
  }

  function notifyLogsChanged() {
    window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
  }

  async function toggleAutoStart(enabled: boolean) {
    const previous = autoStartEnabled;
    setAutoStartEnabled(enabled);
    setAutoStartSaving(true);

    try {
      await invokeCommand("set_auto_start", { enabled });
      notifyLogsChanged();
    } catch (error) {
      setAutoStartEnabled(previous);
      console.warn("设置开机自启动失败:", error);
    } finally {
      setAutoStartSaving(false);
    }
  }

  async function installAppUpdate() {
    if (updateChecking) return;
    setUpdateChecking(true);
    setUpdateStatusType("normal");
    setUpdateStatusMessage("正在检查更新");

    try {
      const result = await invokeCommand<AppUpdateResult>("install_app_update");
      setUpdateStatusMessage(result.message);
      setUpdateStatusType(result.status === "installed" ? "success" : "normal");
      notifyLogsChanged();
    } catch (error) {
      setUpdateStatusMessage(`检查更新失败：${String(error)}`);
      setUpdateStatusType("error");
    } finally {
      setUpdateChecking(false);
    }
  }

  async function startWindowDrag(event: MouseEvent<HTMLElement>) {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, input, select, textarea, a, label")) return;
    try {
      await startCurrentWindowDrag();
    } catch (error) {
      console.warn("启动窗口拖动失败:", error);
    }
  }

  useEffect(() => {
    loadRuntimeStatus();
    loadDeviceSummary();
    loadAutoStart();
    window.addEventListener("homenet:refresh-status", loadRuntimeStatus);
    window.addEventListener("homenet:devices-refresh", loadDeviceSummary);

    const runtimeTimer = window.setInterval(loadRuntimeStatus, 10000);
    const deviceTimer = window.setInterval(loadDeviceSummary, 30000);

    return () => {
      window.removeEventListener("homenet:refresh-status", loadRuntimeStatus);
      window.removeEventListener("homenet:devices-refresh", loadDeviceSummary);
      window.clearInterval(runtimeTimer);
      window.clearInterval(deviceTimer);
    };
  }, []);

  return (
    <div className="window-frame">
      <header className="titlebar" onMouseDown={startWindowDrag}>
        <div className="titlebar-left">
          <button className="menu-button" type="button" aria-label="菜单" onMouseDown={(event) => event.stopPropagation()}>
            <Menu size={21} strokeWidth={2.1} />
          </button>
          <h1>HomeNet</h1>
          {updateStatusMessage ? (
            <span className={`update-status update-${updateStatusType}`}>{updateStatusMessage}</span>
          ) : null}
        </div>

        <div className="window-controls">
          <button
            className="window-control control-minimize"
            type="button"
            aria-label="最小化"
            onClick={minimizeCurrentWindow}
            onMouseDown={(event) => event.stopPropagation()}
          />
          <button
            className="window-control control-maximize"
            type="button"
            aria-label="最大化"
            onClick={toggleMaximizeCurrentWindow}
            onMouseDown={(event) => event.stopPropagation()}
          />
          <button
            className="window-control control-close"
            type="button"
            aria-label="关闭"
            onClick={closeCurrentWindow}
            onMouseDown={(event) => event.stopPropagation()}
          />
        </div>
      </header>

      <main className="main-content">
        <section className="section-cards" aria-label="运行状态">
          {statusCards.map((card) => (
            <StatusCard key={card.title} {...card} />
          ))}
        </section>

        <section className="dashboard-grid" aria-label="网络配置">
          <DeviceDdnsPanel className="device-ddns-column" />
          <div className="routing-column">
            <ForwardRulesPanel />
            <ReverseProxyPanel />
          </div>
        </section>

        <section className="bottom-grid" aria-label="日志与设置">
          <LogPanel />
          <RuntimeSettingsPanel
            uptime={statusData.uptime}
            version={statusData.version || "--"}
            autoStartEnabled={autoStartEnabled}
            autoStartSaving={autoStartSaving}
            updateChecking={updateChecking}
            onToggleAutostart={toggleAutoStart}
            onCheckUpdate={installAppUpdate}
          />
        </section>
      </main>
    </div>
  );
}
