import { MouseEvent, useEffect, useRef, useState } from "react";
import DeviceDdnsPanel from "./components/DeviceDdnsPanel";
import ForwardRulesPanel from "./components/ForwardRulesPanel";
import LogPanel from "./components/LogPanel";
import ReverseProxyPanel from "./components/ReverseProxyPanel";
import RuntimeSettingsPanel from "./components/RuntimeSettingsPanel";
import NetworkOverview from "./components/NetworkOverview";
import PageHeading from "./components/PageHeading";
import {
  ArrowLeftRight,
  ChevronRight,
  FolderOpen,
  House,
  LayoutDashboard,
  Minus,
  MonitorSmartphone,
  ScrollText,
  Server,
  SlidersHorizontal,
  Square,
  Waypoints,
  Workflow,
  X,
} from "lucide-react";
import UpdateConfirmDialog from "./components/UpdateConfirmDialog";
import WebDavPanel from "./components/WebDavPanel";
import {
  closeCurrentWindow,
  invokeCommand,
  minimizeCurrentWindow,
  startCurrentWindowDrag,
  toggleMaximizeCurrentWindow,
} from "./lib/tauri";
import type {
  AppPage,
  AppUpdateResult,
  DeviceDdnsConfig,
  LanDevice,
  RuntimeStatus,
  ThemeMode,
} from "./types";
import { formatUptime } from "./utils/presentation";

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

const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

const navigation = [
  {
    id: "overview",
    title: "网络总览",
    icon: LayoutDashboard,
    group: "工作空间",
  },
  { id: "devices", title: "设备与 DDNS", icon: MonitorSmartphone },
  { id: "forward", title: "端口转发", icon: ArrowLeftRight, group: "连接服务" },
  { id: "proxy", title: "反向代理", icon: Workflow },
  { id: "webdav", title: "文件服务", icon: FolderOpen },
  { id: "logs", title: "运行日志", icon: ScrollText, group: "系统" },
  { id: "settings", title: "偏好设置", icon: SlidersHorizontal },
] as const;

function initialTheme(): ThemeMode {
  try {
    const saved = localStorage.getItem("homenet:theme");
    return saved === "light" || saved === "dark" ? saved : "system";
  } catch {
    return "system";
  }
}

export default function App() {
  const [statusData, setStatusData] = useState<RuntimeStatus>({
    ...emptyStatus,
  });
  const [page, setPage] = useState<AppPage>("overview");
  const [theme, setTheme] = useState<ThemeMode>(initialTheme);
  const [runtimeAvailable, setRuntimeAvailable] = useState(false);
  const [lastRefresh, setLastRefresh] = useState("");
  const [deviceSummary, setDeviceSummary] = useState({
    online: 0,
    total: 0,
    available: false,
  });
  const [bindings, setBindings] = useState<DeviceDdnsConfig[] | null>(null);
  const [autoStartEnabled, setAutoStartEnabled] = useState(false);
  const [autoStartSaving, setAutoStartSaving] = useState(false);
  const [autoUpdateCheckEnabled, setAutoUpdateCheckEnabled] = useState(false);
  const [autoUpdateCheckSaving, setAutoUpdateCheckSaving] = useState(false);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateInstalling, setUpdateInstalling] = useState(false);
  const [availableUpdate, setAvailableUpdate] =
    useState<AppUpdateResult | null>(null);
  const [updateToConfirm, setUpdateToConfirm] =
    useState<AppUpdateResult | null>(null);
  const [updateStatusMessage, setUpdateStatusMessage] = useState("");
  const [updateStatusType, setUpdateStatusType] = useState<
    "normal" | "success" | "error"
  >("normal");
  const updateBusyRef = useRef(false);

  async function loadRuntimeStatus() {
    try {
      const data = await invokeCommand<RuntimeStatus>("get_runtime_status");
      setStatusData({ ...emptyStatus, ...data });
      setRuntimeAvailable(true);
      setLastRefresh(new Date().toLocaleTimeString("zh-CN", { hour12: false }));
    } catch {
      setRuntimeAvailable(false);
    }
  }

  async function loadDeviceSummary() {
    const [devices, configs] = await Promise.allSettled([
      invokeCommand<LanDevice[]>("list_lan_devices"),
      invokeCommand<DeviceDdnsConfig[]>("list_device_ddns_configs"),
    ]);
    setDeviceSummary(
      devices.status === "fulfilled"
        ? {
            online: devices.value.filter((device) => device.online).length,
            total: devices.value.length,
            available: true,
          }
        : { online: 0, total: 0, available: false },
    );
    setBindings(configs.status === "fulfilled" ? configs.value : null);
  }

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem("homenet:theme", theme);
    } catch {
      /* The selected theme still applies when storage is unavailable. */
    }
  }, [theme]);

  async function loadAutoStart() {
    try {
      setAutoStartEnabled(await invokeCommand<boolean>("get_auto_start"));
    } catch {
      setAutoStartEnabled(false);
    }
  }

  async function loadAutoUpdateCheck() {
    try {
      setAutoUpdateCheckEnabled(
        await invokeCommand<boolean>("get_auto_update_check"),
      );
    } catch {
      setAutoUpdateCheckEnabled(false);
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
      setUpdateStatusMessage(`设置开机自启动失败：${String(error)}`);
      setUpdateStatusType("error");
    } finally {
      setAutoStartSaving(false);
    }
  }

  async function toggleAutoUpdateCheck(enabled: boolean) {
    const previous = autoUpdateCheckEnabled;
    setAutoUpdateCheckEnabled(enabled);
    setAutoUpdateCheckSaving(true);

    try {
      await invokeCommand("set_auto_update_check", { enabled });
      notifyLogsChanged();
    } catch (error) {
      setAutoUpdateCheckEnabled(previous);
      setUpdateStatusMessage(`设置自动检查更新失败：${String(error)}`);
      setUpdateStatusType("error");
    } finally {
      setAutoUpdateCheckSaving(false);
    }
  }

  async function checkAppUpdate(
    automatic = false,
  ): Promise<AppUpdateResult | null> {
    if (updateBusyRef.current) return null;
    updateBusyRef.current = true;
    setUpdateChecking(true);
    setUpdateStatusType("normal");
    if (!automatic) setUpdateStatusMessage("正在检查更新");

    try {
      const result = await invokeCommand<AppUpdateResult>("check_app_update");
      if (result.status === "available") {
        setAvailableUpdate(result);
        setUpdateStatusMessage(
          automatic
            ? `发现新版本 ${result.latest_version}，请手动确认安装`
            : result.message,
        );
        setUpdateStatusType("normal");
      } else {
        setAvailableUpdate(null);
        if (!automatic) {
          setUpdateStatusMessage(result.message);
          setUpdateStatusType("success");
        }
      }
      notifyLogsChanged();
      return result;
    } catch (error) {
      if (!automatic) {
        setUpdateStatusMessage(`检查更新失败：${String(error)}`);
        setUpdateStatusType("error");
      }
      return null;
    } finally {
      setUpdateChecking(false);
      updateBusyRef.current = false;
    }
  }

  async function installAvailableUpdate(update: AppUpdateResult) {
    if (updateBusyRef.current) return;

    updateBusyRef.current = true;
    setUpdateInstalling(true);
    setUpdateStatusType("normal");
    setUpdateStatusMessage(
      update.verification === "signed"
        ? `正在下载并验证 ${update.latest_version}`
        : `正在从 GitHub 下载 ${update.latest_version}`,
    );
    try {
      const result = await invokeCommand<AppUpdateResult>(
        "install_app_update",
        {
          expectedVersion: update.latest_version,
        },
      );
      setAvailableUpdate(null);
      setUpdateStatusMessage(result.message);
      setUpdateStatusType("success");
      notifyLogsChanged();
    } catch (error) {
      setUpdateStatusMessage(`安装更新失败：${String(error)}`);
      setUpdateStatusType("error");
    } finally {
      setUpdateInstalling(false);
      updateBusyRef.current = false;
    }
  }

  function requestUpdateInstallation(update: AppUpdateResult) {
    if (updateBusyRef.current) return;
    setUpdateToConfirm(update);
  }

  async function confirmUpdateInstallation() {
    const update = updateToConfirm;
    if (!update || updateBusyRef.current) return;
    setUpdateToConfirm(null);
    await installAvailableUpdate(update);
  }

  async function handleUpdateAction() {
    if (availableUpdate) {
      requestUpdateInstallation(availableUpdate);
      return;
    }
    const result = await checkAppUpdate(false);
    if (result?.status === "available") {
      requestUpdateInstallation(result);
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
    loadAutoUpdateCheck();
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

  useEffect(() => {
    if (!autoUpdateCheckEnabled) return;
    void checkAppUpdate(true);
    const timer = window.setInterval(
      () => void checkAppUpdate(true),
      UPDATE_CHECK_INTERVAL_MS,
    );
    return () => window.clearInterval(timer);
  }, [autoUpdateCheckEnabled]);

  const currentPage = navigation.find((item) => item.id === page)!;
  const pageContent =
    page === "overview" ? (
      <NetworkOverview
        status={statusData}
        available={runtimeAvailable}
        devices={deviceSummary}
        bindings={bindings}
        onNavigate={setPage}
        onRefresh={async () => {
          await Promise.all([loadRuntimeStatus(), loadDeviceSummary()]);
        }}
      />
    ) : page === "devices" ? (
      <DeviceDdnsPanel />
    ) : page === "forward" ? (
      <ForwardRulesPanel />
    ) : page === "proxy" ? (
      <ReverseProxyPanel />
    ) : page === "webdav" ? (
      <>
        <PageHeading
          eyebrow="SERVICES / WEBDAV"
          title="文件服务"
          description="将本地目录，分享给受信任的设备。"
        />
        <WebDavPanel onOpenProxy={() => setPage("proxy")} />
      </>
    ) : page === "logs" ? (
      <>
        <PageHeading
          eyebrow="SYSTEM / ACTIVITY"
          title="运行日志"
          description="每一次连接与变化，都有迹可循。"
        />
        <LogPanel />
      </>
    ) : (
      <>
        <PageHeading
          eyebrow="SYSTEM / PREFERENCES"
          title="偏好设置"
          description="按照你的习惯，让 HomeNet 安静运行。"
        />
        <RuntimeSettingsPanel
          uptime={statusData.uptime}
          version={statusData.version || "--"}
          autoStartEnabled={autoStartEnabled}
          autoStartSaving={autoStartSaving}
          autoUpdateCheckEnabled={autoUpdateCheckEnabled}
          autoUpdateCheckSaving={autoUpdateCheckSaving}
          updateChecking={updateChecking}
          updateInstalling={updateInstalling}
          availableUpdateVersion={availableUpdate?.latest_version || ""}
          onToggleAutostart={toggleAutoStart}
          onToggleAutoUpdateCheck={toggleAutoUpdateCheck}
          onUpdateAction={handleUpdateAction}
          theme={theme}
          onThemeChange={setTheme}
        />
      </>
    );

  return (
    <div className="window-frame">
      <aside className="app-sidebar">
        <div className="app-brand" onMouseDown={startWindowDrag}>
          <Waypoints aria-hidden="true" />
          <div>
            <strong>
              HomeNet<span>.</span>
            </strong>
            <small>PERSONAL NETWORK</small>
          </div>
        </div>
        <nav className="app-navigation" aria-label="主导航">
          {navigation.map((item) => (
            <div className="nav-item" key={item.id}>
              {"group" in item ? (
                <p className="nav-group">{item.group}</p>
              ) : null}
              <button
                type="button"
                aria-label={item.title}
                title={item.title}
                aria-current={page === item.id ? "page" : undefined}
                onClick={() => setPage(item.id)}
              >
                <item.icon size={18} />
                <span>{item.title}</span>
                {item.id === "devices" && deviceSummary.available ? (
                  <small>{deviceSummary.total}</small>
                ) : item.id === "forward" && runtimeAvailable ? (
                  <small>{statusData.enabled_rule_count}</small>
                ) : item.id === "proxy" && runtimeAvailable ? (
                  <small>{statusData.enabled_reverse_proxy_rule_count}</small>
                ) : null}
              </button>
            </div>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="local-service">
            <Server size={28} />
            <div>
              <strong>
                {runtimeAvailable ? "本机服务运行中" : "本机服务未连接"}
              </strong>
              <small>
                {runtimeAvailable
                  ? `已运行 ${formatUptime(statusData.uptime)}`
                  : "请检查服务状态"}
              </small>
            </div>
          </div>
          <div className="sidebar-version">
            <span>HomeNet</span>
            <span>{statusData.version ? `v${statusData.version}` : "--"}</span>
          </div>
        </div>
      </aside>
      <div className="app-workspace">
        <header className="titlebar" onMouseDown={startWindowDrag}>
          <div className="titlebar-left">
            <House size={15} />
            <span className="breadcrumb-home">个人网络</span>
            <ChevronRight size={14} />
            <span className="breadcrumb-current">{currentPage.title}</span>
          </div>
          {updateStatusMessage ? (
            <button
              type="button"
              role="status"
              title={updateStatusMessage}
              className={`update-status update-${updateStatusType}`}
              onClick={() => setPage("settings")}
            >
              {updateStatusMessage}
            </button>
          ) : null}
          <div className="window-controls">
            <button
              className="window-control"
              type="button"
              aria-label="最小化"
              onClick={minimizeCurrentWindow}
            >
              <Minus size={15} />
            </button>
            <button
              className="window-control"
              type="button"
              aria-label="最大化"
              onClick={toggleMaximizeCurrentWindow}
            >
              <Square size={13} />
            </button>
            <button
              className="window-control control-close"
              type="button"
              aria-label="关闭"
              onClick={closeCurrentWindow}
            >
              <X size={16} />
            </button>
          </div>
        </header>
        <main className="main-content" key={page}>
          {pageContent}
        </main>
        <footer className="app-footer">
          <span>
            <span
              className={`status-dot ${runtimeAvailable ? "" : "is-muted"}`}
            />
            {runtimeAvailable ? "服务运行中" : "服务未连接"}
            {lastRefresh ? ` · 最近刷新 ${lastRefresh}` : ""}
          </span>
          <span>所有配置保存在本机</span>
        </footer>
      </div>
      {updateToConfirm ? (
        <UpdateConfirmDialog
          update={updateToConfirm}
          onCancel={() => setUpdateToConfirm(null)}
          onConfirm={confirmUpdateInstallation}
        />
      ) : null}
    </div>
  );
}
