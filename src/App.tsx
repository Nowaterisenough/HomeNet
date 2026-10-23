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
  FolderOpen,
  LayoutDashboard,
  Minus,
  MonitorSmartphone,
  ScrollText,
  RefreshCw,
  SlidersHorizontal,
  Square,
  Workflow,
  X,
} from "lucide-react";
import UpdateConfirmDialog from "./components/UpdateConfirmDialog";
import WebDavPanel from "./components/WebDavPanel";
import {
  closeCurrentWindow,
  currentTauriWindow,
  invokeCommand,
  minimizeCurrentWindow,
  startCurrentWindowDrag,
  toggleMaximizeCurrentWindow,
} from "./lib/tauri";
import type { AppPage, AppUpdateResult, RuntimeStatus } from "./types";
import { formatUptime } from "./utils/presentation";
import { useTheme } from "./hooks/useTheme";
import { useDeviceInventory } from "./hooks/useDeviceInventory";
import HomeNetIcon from "./components/HomeNetIcon";

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
    title: "总览",
    icon: LayoutDashboard,
  },
  { id: "devices", title: "设备与 DDNS", icon: MonitorSmartphone },
  { id: "forward", title: "端口转发", icon: ArrowLeftRight },
  { id: "proxy", title: "反向代理", icon: Workflow },
  { id: "webdav", title: "文件服务", icon: FolderOpen },
  { id: "logs", title: "日志", icon: ScrollText },
] as const;

export default function App() {
  const [statusData, setStatusData] = useState<RuntimeStatus>({
    ...emptyStatus,
  });
  const [page, setPage] = useState<AppPage>("overview");
  const [theme, setTheme] = useTheme();
  const [refreshing, setRefreshing] = useState(false);
  const [runtimeAvailable, setRuntimeAvailable] = useState(false);
  const [lastRefresh, setLastRefresh] = useState("");
  const inventory = useDeviceInventory();
  const deviceSummary = {
    online: inventory.devices.filter((device) => device.online).length,
    total: inventory.devices.length,
    available: inventory.devicesLoaded || inventory.configsLoaded,
  };
  const bindings = inventory.configsLoaded ? inventory.configs : null;
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

  useEffect(() => {
    const appWindow = currentTauriWindow();
    if (!appWindow) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    async function navigateFromTray() {
      const target = await invokeCommand<AppPage | null>("take_main_page");
      if (!disposed && target) setPage(target);
    }
    void appWindow
      .listen("homenet:navigate", () => {
        void navigateFromTray().catch(console.warn);
      })
      .then((stop) => {
        if (disposed) stop();
        else {
          unlisten = stop;
          void navigateFromTray().catch(console.warn);
        }
      })
      .catch(console.warn);
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

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
    loadAutoStart();
    loadAutoUpdateCheck();
    window.addEventListener("homenet:refresh-status", loadRuntimeStatus);

    const runtimeTimer = window.setInterval(loadRuntimeStatus, 10000);

    return () => {
      window.removeEventListener("homenet:refresh-status", loadRuntimeStatus);
      window.clearInterval(runtimeTimer);
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

  async function refreshWorkspace() {
    if (refreshing) return;
    setRefreshing(true);
    try {
      await Promise.all([loadRuntimeStatus(), inventory.refresh()]);
      notifyLogsChanged();
    } finally {
      setRefreshing(false);
    }
  }

  const isMac = /Mac/.test(navigator.platform);
  const windowControls = (
    <div className={`window-controls${isMac ? " window-controls-macos" : ""}`}>
      <button
        className="window-control control-minimize"
        type="button"
        aria-label="最小化"
        onClick={minimizeCurrentWindow}
      >
        <Minus size={14} />
      </button>
      <button
        className="window-control control-maximize"
        type="button"
        aria-label="最大化"
        onClick={toggleMaximizeCurrentWindow}
      >
        <Square size={12} />
      </button>
      <button
        className="window-control control-close"
        type="button"
        aria-label="关闭"
        title="关闭窗口，后台服务继续运行"
        onClick={closeCurrentWindow}
      >
        <X size={14} />
      </button>
    </div>
  );
  const pageContent =
    page === "overview" ? (
      <NetworkOverview
        status={statusData}
        available={runtimeAvailable}
        devices={deviceSummary}
        bindings={bindings}
        onNavigate={setPage}
        onRefresh={async () => {
          await Promise.all([loadRuntimeStatus(), inventory.refresh()]);
        }}
      />
    ) : page === "devices" ? (
      <DeviceDdnsPanel inventory={inventory} />
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
      <header className="titlebar" onMouseDown={startWindowDrag}>
        {isMac ? windowControls : null}
        <div className="app-brand">
          <HomeNetIcon size={24} />
          <strong>HomeNet</strong>
        </div>
        <nav className="app-navigation" aria-label="主导航">
          {navigation.map((item) => (
            <button
              key={item.id}
              type="button"
              aria-label={item.title}
              title={item.title}
              aria-current={page === item.id ? "page" : undefined}
              onClick={() => setPage(item.id)}
            >
              <item.icon size={16} />
              <span>{item.title}</span>
            </button>
          ))}
        </nav>
        <div className="titlebar-actions">
          <button
            className="chrome-action"
            aria-label="刷新状态"
            title="刷新状态"
            disabled={refreshing}
            onClick={refreshWorkspace}
          >
            <RefreshCw size={16} className={refreshing ? "spinning" : ""} />
          </button>
          <button
            className="chrome-action"
            aria-label="偏好设置"
            title="偏好设置"
            aria-current={page === "settings" ? "page" : undefined}
            onClick={() => setPage("settings")}
          >
            <SlidersHorizontal size={17} />
          </button>
        </div>
        {!isMac ? windowControls : null}
      </header>
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
        <span>
          {runtimeAvailable
            ? `已运行 ${formatUptime(statusData.uptime)} · v${statusData.version}`
            : "所有配置保存在本机"}
        </span>
      </footer>
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
