import { ChangeEvent, useEffect, useMemo, useRef, useState } from "react";
import {
  Cloud,
  CloudOff,
  ChevronLeft,
  ChevronRight,
  Globe,
  Monitor,
  Info,
  Pencil,
  RefreshCw,
  Save,
  Search,
  Trash2,
  Wifi,
  WifiOff,
  Zap,
} from "lucide-react";
import EditorDrawer from "./EditorDrawer";
import PageHeading from "./PageHeading";
import { statusTone } from "../utils/presentation";
import { invokeCommand } from "../lib/tauri";
import type { DeviceDdnsConfig, LanDevice } from "../types";
import { formatSyncTime } from "../utils/time";
import type { DeviceInventory } from "../hooks/useDeviceInventory";
import {
  deviceRowId,
  isGeneratedLocalMachineConfig,
  matchingDeviceForConfig,
} from "../utils/devices";

interface DeviceDdnsPanelProps {
  inventory: DeviceInventory;
  className?: string;
}

interface DeviceRow {
  id: string;
  name: string;
  nativeName: string;
  mac: string;
  online: boolean;
  configured: boolean;
  enabled: boolean;
  domain: string;
  lastSync: string;
  boundIp: string;
  raw: LanDevice;
  config: DeviceDdnsConfig | null;
}

const defaultConfig: DeviceDdnsConfig = {
  enabled: false,
  provider: "aliyun",
  access_key_id: "",
  access_key_secret: "",
  domain: "",
  sub_domain: "",
  record_type: "AAAA",
  ttl: 600,
  interval_minutes: 10,
  ip_candidate_index: 1,
  device_id: "",
  device_mac: "",
  device_name: "",
  selected_ipv6: "",
  selected_ip: "",
  last_update_time: "",
  last_result: "",
  last_online: false,
};

function uniqueStrings(values: string[]): string[] {
  return values.reduce<string[]>((result, value) => {
    const trimmed = value.trim();
    if (trimmed && !result.includes(trimmed)) result.push(trimmed);
    return result;
  }, []);
}

function displayName(device: LanDevice, index: number): string {
  return device.hostname || device.display_name || `LAN-DEVICE-${index + 1}`;
}

function firstGlobalIpv6(device: LanDevice): string {
  return device.global_ipv6[0] || "";
}

function recordTypeForConfig(config: DeviceDdnsConfig | null): "A" | "AAAA" {
  return config?.record_type.trim().toUpperCase() === "A" ? "A" : "AAAA";
}

function configuredIpForConfig(config: DeviceDdnsConfig | null): string {
  if (!config) return "";
  const recordType = recordTypeForConfig(config);
  if (recordType === "A") return config.selected_ip.trim();
  return (config.selected_ip || config.selected_ipv6).trim();
}

function availableIpsForRecordType(
  device: LanDevice,
  recordType: "A" | "AAAA",
): string[] {
  if (!device.online) return [];
  if (recordType === "A") {
    return uniqueStrings(device.ipv4);
  }
  return uniqueStrings(device.global_ipv6.filter((ip) => ip.includes(":")));
}

function currentIpForConfig(
  config: DeviceDdnsConfig | null,
  device: LanDevice,
): string {
  const recordType = recordTypeForConfig(config);
  const available = availableIpsForRecordType(device, recordType);
  if (recordType === "AAAA") {
    const candidateIndex = Math.max(
      1,
      Math.floor(Number(config?.ip_candidate_index) || 1),
    );
    return available[candidateIndex - 1] || "";
  }
  const configured = configuredIpForConfig(config);
  if (configured && available.includes(configured)) return configured;
  return available[0] || "";
}

function normalizeConfig(
  data: Partial<DeviceDdnsConfig> | null | undefined,
): DeviceDdnsConfig {
  return {
    ...defaultConfig,
    ...data,
    enabled: data?.enabled ?? defaultConfig.enabled,
    provider: data?.provider || defaultConfig.provider,
    access_key_id: data?.access_key_id || "",
    access_key_secret: data?.access_key_secret || "",
    domain: data?.domain || "",
    sub_domain: data?.sub_domain || "",
    record_type: data?.record_type?.trim().toUpperCase() === "A" ? "A" : "AAAA",
    ttl: Number(data?.ttl) || defaultConfig.ttl,
    interval_minutes:
      Number(data?.interval_minutes) || defaultConfig.interval_minutes,
    ip_candidate_index: Math.max(
      1,
      Math.floor(Number(data?.ip_candidate_index) || 1),
    ),
    device_id: data?.device_id || "",
    device_mac: data?.device_mac || "",
    device_name: data?.device_name || "",
    selected_ipv6: data?.selected_ipv6 || "",
    selected_ip: data?.selected_ip || data?.selected_ipv6 || "",
    last_update_time: data?.last_update_time || "",
    last_result: data?.last_result || "",
    last_online: data?.last_online ?? defaultConfig.last_online,
  };
}

function configForDevice(
  device: LanDevice,
  configs: DeviceDdnsConfig[],
  devices: LanDevice[],
): DeviceDdnsConfig | null {
  return (
    configs.find(
      (config) => matchingDeviceForConfig(config, devices) === device,
    ) ?? null
  );
}

function selectedIpForDraft(
  config: DeviceDdnsConfig | null,
  device: LanDevice,
): string {
  return (
    currentIpForConfig(config, device) || configuredIpForConfig(config) || "-"
  );
}

function boundIpForConfig(
  config: DeviceDdnsConfig | null,
  device: LanDevice,
): string {
  if (!config) return "-";
  return (
    (device.online
      ? currentIpForConfig(config, device)
      : configuredIpForConfig(config)) || "-"
  );
}

function configuredDomain(config: DeviceDdnsConfig | null): string {
  const domain = config?.domain.trim() ?? "";
  const sub = config?.sub_domain.trim() ?? "";
  if (!domain) return "-";
  return sub ? `${sub}.${domain}` : domain;
}

function mapLanDevice(
  device: LanDevice,
  index: number,
  configs: DeviceDdnsConfig[],
  devices: LanDevice[],
): DeviceRow {
  const config = configForDevice(device, configs, devices);
  const nativeName = displayName(device, index);
  const configuredName =
    config && !isGeneratedLocalMachineConfig(config)
      ? config.device_name.trim()
      : "";
  return {
    id: deviceRowId(device),
    name: configuredName || nativeName,
    nativeName,
    mac: device.mac || "-",
    online: device.online,
    configured: Boolean(config),
    enabled: Boolean(config?.enabled),
    domain: configuredDomain(config),
    lastSync: formatSyncTime(config?.last_update_time || ""),
    boundIp: boundIpForConfig(config, device),
    raw: device,
    config,
  };
}

function prioritizeEnabledDeviceRows(rows: DeviceRow[]): DeviceRow[] {
  const enabledRows: DeviceRow[] = [];
  const remainingRows: DeviceRow[] = [];

  for (const row of rows) {
    (row.enabled ? enabledRows : remainingRows).push(row);
  }

  return [...enabledRows, ...remainingRows];
}

function buildDraft(
  row: DeviceRow,
  configs: DeviceDdnsConfig[],
): DeviceDdnsConfig {
  const existing = row.config;
  const template = existing ?? configs[0] ?? defaultConfig;
  const recordType =
    existing?.record_type?.trim().toUpperCase() === "A" ? "A" : "AAAA";
  const selectedIp = selectedIpForDraft(existing, row.raw);
  const normalizedSelectedIp = selectedIp === "-" ? "" : selectedIp;
  const configuredName =
    existing && !isGeneratedLocalMachineConfig(existing)
      ? existing.device_name
      : "";
  return normalizeConfig({
    ...template,
    enabled: existing?.enabled ?? true,
    device_id: row.raw.id,
    device_mac: row.raw.mac,
    device_name: configuredName || row.name || row.nativeName,
    record_type: recordType,
    ip_candidate_index: Math.max(
      1,
      Math.floor(Number(existing?.ip_candidate_index) || 1),
    ),
    selected_ip: normalizedSelectedIp,
    selected_ipv6: recordType === "AAAA" ? normalizedSelectedIp : "",
    last_update_time: existing?.last_update_time || "",
    last_result: existing?.last_result || "",
    last_online: existing?.last_online ?? false,
    sub_domain: existing?.sub_domain || "",
  });
}

export default function DeviceDdnsPanel({
  inventory,
  className = "",
}: DeviceDdnsPanelProps) {
  const devices = inventory.devices;
  const configs = useMemo(
    () => inventory.configs.map(normalizeConfig),
    [inventory.configs],
  );
  const [selectedId, setSelectedId] = useState("");
  const [draft, setDraft] = useState<DeviceDdnsConfig | null>(null);
  const [configDialogOpen, setConfigDialogOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const loading = inventory.loading;
  const [mutating, setMutating] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [statusMessage, setStatusMessage] = useState("");
  const [messageType, setMessageType] = useState<"info" | "success" | "error">(
    "info",
  );
  const [page, setPage] = useState(0);
  const lastScan =
    inventory.lastScan === null
      ? ""
      : new Date(inventory.lastScan).toLocaleTimeString("zh-CN", {
          hour12: false,
        });
  const selectionRef = useRef({ selectedId, configDialogOpen });
  selectionRef.current = { selectedId, configDialogOpen };

  const rows = useMemo(
    () =>
      prioritizeEnabledDeviceRows(
        devices.map((device, index) =>
          mapLanDevice(device, index, configs, devices),
        ),
      ),
    [configs, devices],
  );
  const selectedRow = useMemo(
    () => rows.find((row) => row.id === selectedId) ?? null,
    [rows, selectedId],
  );
  const filteredDevices = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return rows;
    return rows.filter((device) =>
      [
        device.name,
        device.nativeName,
        device.mac,
        device.domain,
        device.boundIp,
        ...device.raw.ipv4,
        ...device.raw.ipv6,
        ...device.raw.global_ipv6,
      ].some((value) => value.toLowerCase().includes(query)),
    );
  }, [rows, searchQuery]);
  const availableIpOptions = useMemo(() => {
    if (!selectedRow || !draft) return [];
    return availableIpsForRecordType(
      selectedRow.raw,
      recordTypeForConfig(draft),
    );
  }, [draft, selectedRow]);
  const selectedIpv6Candidate = useMemo(() => {
    if (!draft || recordTypeForConfig(draft) !== "AAAA") return "";
    const candidateIndex = Math.max(
      1,
      Math.floor(Number(draft.ip_candidate_index) || 1),
    );
    return availableIpOptions[candidateIndex - 1] || "";
  }, [availableIpOptions, draft]);
  const previewRows = useMemo(() => {
    const domain = draft?.domain.trim() ?? "";
    const sub = draft?.sub_domain.trim() ?? "";
    if (!selectedRow || !domain) return [];
    return [[deviceDisplayNameForDraft(), sub ? `${sub}.${domain}` : domain]];
  }, [draft, selectedRow]);

  function deviceDisplayNameForDraft(): string {
    if (!selectedRow || !draft) return "";
    return draft.device_name.trim() || selectedRow.nativeName;
  }

  function syncDraftWithSelection(
    nextConfigs = configs,
    nextRows = rows,
    nextSelectedId = selectedId,
  ) {
    const row = nextRows.find((item) => item.id === nextSelectedId) ?? null;
    setDraft(row ? buildDraft(row, nextConfigs) : null);
  }

  async function loadConfigsData(): Promise<DeviceDdnsConfig[]> {
    return (await inventory.refreshConfigs()).map(normalizeConfig);
  }

  async function loadDevices() {
    await inventory.refresh();
  }

  useEffect(() => {
    const selection = selectionRef.current;
    const current = rows.find((row) => row.id === selection.selectedId);
    const next =
      current ?? rows.find((row) => row.configured) ?? rows[0] ?? null;
    setSelectedId(next?.id ?? "");
    if (!selection.configDialogOpen) {
      setDraft(next ? buildDraft(next, configs) : null);
    } else if (!current) {
      setConfigDialogOpen(false);
      setDraft(next ? buildDraft(next, configs) : null);
    }
  }, [rows, configs]);

  function selectRow(row: DeviceRow) {
    setSelectedId(row.id);
    if (!configDialogOpen) {
      setDraft(buildDraft(row, configs));
    }
  }

  function openConfigDialog(row: DeviceRow) {
    setSelectedId(row.id);
    setDraft(buildDraft(row, configs));
    setConfigDialogOpen(true);
  }

  function closeConfigDialog() {
    setConfigDialogOpen(false);
    syncDraftWithSelection();
  }

  function notifyDataChanged() {
    window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
    window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
  }

  function validateDraft(requireEnabled: boolean): string {
    if (!selectedRow || !draft) return "请选择要配置的局域网设备";
    if (!selectedRow.raw.mac.trim()) return "绑定设备必须有 MAC 地址";
    if (requireEnabled && !draft.enabled) return "请先启用同步";
    if (requireEnabled && !selectedRow.online)
      return "设备离线，等待上线后再同步";
    if (!draft.access_key_id.trim() || !draft.access_key_secret.trim()) {
      return "请填写完整的 AccessKey ID 和 Secret";
    }
    if (!draft.domain.trim()) return "请填写主域名";
    if (!["A", "AAAA"].includes(draft.record_type.trim().toUpperCase())) {
      return "记录类型仅支持 A 或 AAAA";
    }
    if (
      selectedRow.online &&
      draft.record_type.trim().toUpperCase() === "AAAA"
    ) {
      if (availableIpOptions.length === 0)
        return "当前设备没有可用于 DDNS 的稳定公网 IPv6";
      const candidateIndex = Math.max(
        1,
        Math.floor(Number(draft.ip_candidate_index) || 1),
      );
      if (candidateIndex > availableIpOptions.length) {
        return `当前只有 ${availableIpOptions.length} 个可用于 DDNS 的公网 IPv6 候选`;
      }
    }
    if (draft.sub_domain.trim().includes(","))
      return "每台设备请填写一个独立子域名";
    return "";
  }

  function buildPayload(): DeviceDdnsConfig | null {
    if (!selectedRow || !draft) return null;
    const recordType =
      draft.record_type.trim().toUpperCase() === "A" ? "A" : "AAAA";
    const candidateIndex = Math.max(
      1,
      Math.floor(Number(draft.ip_candidate_index) || 1),
    );
    const selectedIp = !selectedRow.online
      ? configuredIpForConfig(draft)
      : (recordType === "A"
          ? draft.selected_ip || selectedRow.raw.ipv4[0]
          : availableIpsForRecordType(selectedRow.raw, "AAAA")[
              candidateIndex - 1
            ] || firstGlobalIpv6(selectedRow.raw)) || "";
    return {
      ...draft,
      provider: draft.provider || defaultConfig.provider,
      sub_domain: draft.sub_domain.trim(),
      record_type: recordType,
      ttl: Number(draft.ttl) || defaultConfig.ttl,
      interval_minutes:
        Number(draft.interval_minutes) || defaultConfig.interval_minutes,
      ip_candidate_index: candidateIndex,
      device_id: selectedRow.raw.id,
      device_mac: selectedRow.raw.mac,
      device_name: deviceDisplayNameForDraft(),
      selected_ip: selectedIp,
      selected_ipv6: recordType === "AAAA" ? selectedIp : "",
    };
  }

  async function saveSelectedConfig(
    showSuccess = true,
  ): Promise<DeviceDdnsConfig | null> {
    const error = validateDraft(false);
    if (error) {
      setStatusMessage(error);
      setMessageType("error");
      return null;
    }

    const payload = buildPayload();
    if (!payload) return null;

    setMutating(true);
    try {
      await invokeCommand("save_device_ddns_config", { config: payload });
      const nextConfigs = await loadConfigsData();
      const nextRows = devices.map((device, index) =>
        mapLanDevice(device, index, nextConfigs, devices),
      );
      syncDraftWithSelection(nextConfigs, nextRows);
      if (showSuccess) {
        setStatusMessage("设备 DDNS 绑定已保存");
        setMessageType("success");
        setConfigDialogOpen(false);
      }
      notifyDataChanged();
      return payload;
    } catch (error) {
      setStatusMessage(`保存设备 DDNS 绑定失败：${String(error)}`);
      setMessageType("error");
      return null;
    } finally {
      setMutating(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  async function unbindDevice(row: DeviceRow) {
    setMutating(true);
    try {
      await invokeCommand("delete_device_ddns_config", {
        deviceId: row.raw.id,
        deviceMac: row.raw.mac,
      });
      const nextConfigs = await loadConfigsData();
      const nextRows = devices.map((device, index) =>
        mapLanDevice(device, index, nextConfigs, devices),
      );
      syncDraftWithSelection(
        nextConfigs,
        nextRows,
        selectedId === row.id ? row.id : selectedId,
      );
      setStatusMessage("设备 DDNS 绑定已解除");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      setStatusMessage(`解除设备 DDNS 绑定失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setMutating(false);
      window.setTimeout(() => setStatusMessage(""), 4200);
    }
  }

  async function syncNow() {
    const error = validateDraft(true);
    if (error) {
      setStatusMessage(error);
      setMessageType("error");
      return;
    }

    setSyncing(true);
    try {
      const payload = await saveSelectedConfig(false);
      if (!payload) return;
      const result = await invokeCommand<string>("trigger_device_ddns_update", {
        config: payload,
      });
      const nextConfigs = await loadConfigsData();
      const nextRows = devices.map((device, index) =>
        mapLanDevice(device, index, nextConfigs, devices),
      );
      syncDraftWithSelection(nextConfigs, nextRows);
      setStatusMessage(result || "设备 DDNS 已同步");
      setMessageType("success");
      notifyDataChanged();
    } catch (error) {
      setStatusMessage(`设备 DDNS 同步失败：${String(error)}`);
      setMessageType("error");
    } finally {
      setSyncing(false);
      window.setTimeout(() => setStatusMessage(""), 5200);
    }
  }

  function updateDraft<K extends keyof DeviceDdnsConfig>(
    key: K,
    value: DeviceDdnsConfig[K],
  ) {
    setDraft((current) => (current ? { ...current, [key]: value } : current));
  }

  function updateIpv6CandidateIndex(value: number) {
    const candidateIndex = Math.max(1, Math.floor(Number(value) || 1));
    const selectedIp =
      availableIpOptions[candidateIndex - 1] ||
      (!selectedRow?.online ? configuredIpForConfig(draft) : "");
    setDraft((current) =>
      current
        ? {
            ...current,
            ip_candidate_index: candidateIndex,
            selected_ip: selectedIp,
            selected_ipv6: selectedIp,
          }
        : current,
    );
  }

  function onDraftCheckbox(
    key: keyof DeviceDdnsConfig,
    event: ChangeEvent<HTMLInputElement>,
  ) {
    updateDraft(key, event.target.checked as never);
  }

  const pageCount = Math.max(1, Math.ceil(filteredDevices.length / 6));
  const currentPage = Math.min(page, pageCount - 1);
  const visibleDevices = filteredDevices.slice(
    currentPage * 6,
    currentPage * 6 + 6,
  );
  const selectedStatus = !selectedRow?.configured
    ? "未绑定"
    : !selectedRow.enabled
      ? "已停用"
      : selectedRow.config?.last_result || "等待首次同步";

  return (
    <div className={`device-ddns-stack ${className}`.trim()}>
      <PageHeading
        eyebrow="WORKSPACE / DEVICES"
        title="设备与 DDNS"
        description="为每一台设备，找到回家的地址。"
        actions={
          <button
            className="btn btn-primary"
            type="button"
            disabled={loading || mutating || syncing}
            onClick={() => loadDevices()}
          >
            <RefreshCw size={15} className={loading ? "spinning" : ""} />
            刷新设备
          </button>
        }
      />
      {(inventory.deviceError || inventory.configError) && !configDialogOpen ? (
        <p className="status-message msg-error" role="status">
          {inventory.deviceError || inventory.configError}
        </p>
      ) : null}
      {statusMessage && !configDialogOpen ? (
        <p className={`status-message msg-${messageType}`} role="status">
          {statusMessage}
        </p>
      ) : null}
      <div className="device-layout">
        <div className="device-list-column">
          <label className="search-box">
            <Search size={16} />
            <input
              aria-label="搜索设备"
              type="search"
              placeholder="搜索设备名称、IP、域名或 MAC"
              value={searchQuery}
              onChange={(event) => {
                setSearchQuery(event.target.value);
                setPage(0);
              }}
            />
          </label>
          <section className="panel devices-list-panel">
            <header className="panel-header">
              <h2>
                局域网设备 <small>{rows.length}</small>
              </h2>
              <span className="text-muted">
                {rows.filter((row) => row.online).length} 在线 ·{" "}
                {rows.filter((row) => row.configured).length} 已绑定
              </span>
            </header>
            <div className="device-list">
              {!visibleDevices.length ? (
                <div className="empty-state">
                  {loading
                    ? "正在扫描局域网设备"
                    : searchQuery
                      ? "没有匹配的设备"
                      : "暂无局域网设备，请尝试刷新"}
                </div>
              ) : (
                visibleDevices.map((device) => (
                  <button
                    type="button"
                    key={device.id}
                    className={`device-list-row ${selectedId === device.id ? "is-selected" : ""}`}
                    aria-pressed={selectedId === device.id}
                    onClick={() => selectRow(device)}
                  >
                    <span className="device-avatar">
                      <Monitor size={20} />
                    </span>
                    <span className="device-list-copy">
                      <strong>{device.name}</strong>
                      <small className="mono">
                        {device.raw.ipv4[0] ||
                          device.raw.global_ipv6[0] ||
                          device.mac}
                      </small>
                    </span>
                    <span className="device-status-icons">
                      <span
                        className={`state-label ${device.online ? "tone-success" : "tone-neutral"}`}
                      >
                        {device.online ? (
                          <Wifi size={13} />
                        ) : (
                          <WifiOff size={13} />
                        )}
                        {device.online ? "在线" : "离线"}
                      </span>
                      <span
                        className={`badge ${device.enabled ? "" : "badge-neutral"}`}
                      >
                        {device.configured ? (
                          <Cloud size={12} />
                        ) : (
                          <CloudOff size={12} />
                        )}
                        {device.enabled
                          ? "DDNS 已启用"
                          : device.configured
                            ? "DDNS 已停用"
                            : "未绑定"}
                      </span>
                    </span>
                  </button>
                ))
              )}
            </div>
            <footer className="list-footer">
              <span>
                {loading
                  ? lastScan
                    ? "后台刷新中，显示上次结果"
                    : "正在扫描局域网设备"
                  : lastScan
                    ? `最近扫描 ${lastScan} · 每 60 秒刷新`
                    : "等待扫描"}
              </span>
              <div className="toolbar">
                <span>
                  {currentPage + 1} / {pageCount}
                </span>
                <button
                  className="icon-action"
                  type="button"
                  aria-label="上一页设备"
                  disabled={currentPage === 0}
                  onClick={() => setPage(currentPage - 1)}
                >
                  <ChevronLeft size={15} />
                </button>
                <button
                  className="icon-action"
                  type="button"
                  aria-label="下一页设备"
                  disabled={currentPage === pageCount - 1}
                  onClick={() => setPage(currentPage + 1)}
                >
                  <ChevronRight size={15} />
                </button>
              </div>
            </footer>
          </section>
        </div>
        <section className="panel device-detail" aria-label="所选设备详情">
          {selectedRow ? (
            <>
              <header className="device-detail-header">
                <span className="device-avatar">
                  <Monitor size={23} />
                </span>
                <div>
                  <h2>{selectedRow.name}</h2>
                  <p>
                    最后发现：
                    {formatSyncTime(selectedRow.raw.last_seen, "暂无")}
                  </p>
                </div>
                <span
                  className={`state-label ${selectedRow.online ? "tone-success" : "tone-neutral"}`}
                >
                  <span className="status-dot" />
                  {selectedRow.online ? "在线" : "离线"}
                </span>
              </header>
              <div className="device-detail-body">
                <div className="detail-section-heading">
                  <h3>网络信息</h3>
                  {selectedRow.raw.global_ipv6.length ? (
                    <span className="badge">公网 IPv6</span>
                  ) : (
                    <span className="badge badge-neutral">
                      {selectedRow.online ? "无公网 IPv6" : "离线 · 上次地址"}
                    </span>
                  )}
                </div>
                <dl className="detail-list">
                  <div>
                    <dt>IPv4 地址</dt>
                    <dd className="mono">
                      {selectedRow.raw.ipv4.join(" / ") || "--"}
                    </dd>
                  </div>
                  <div>
                    <dt>IPv6 地址</dt>
                    <dd className="mono">
                      {uniqueStrings([
                        ...selectedRow.raw.global_ipv6,
                        ...selectedRow.raw.ipv6,
                      ]).map((ip) => (
                        <span className="ip-line" key={ip}>
                          {ip}
                        </span>
                      ))}
                    </dd>
                  </div>
                  <div>
                    <dt>MAC 地址</dt>
                    <dd className="mono">{selectedRow.mac}</dd>
                  </div>
                </dl>
                <div className="detail-separator" />
                <div className="detail-section-heading">
                  <h3>DDNS 绑定</h3>
                  <span
                    className={`badge ${selectedRow.enabled ? "" : "badge-neutral"}`}
                  >
                    {selectedRow.enabled
                      ? "已启用"
                      : selectedRow.configured
                        ? "已停用"
                        : "未绑定"}
                  </span>
                </div>
                {selectedRow.config ? (
                  <>
                    <div className="binding-preview">
                      <strong>
                        <Globe size={16} />
                        {selectedRow.domain}
                      </strong>
                      <p className="mono">
                        {recordTypeForConfig(selectedRow.config)} ·{" "}
                        {selectedRow.boundIp}
                        {!selectedRow.online ? "（上次地址）" : ""}
                      </p>
                    </div>
                    <dl className="detail-list">
                      <div>
                        <dt>服务商</dt>
                        <dd>阿里云 DNS</dd>
                      </div>
                      <div>
                        <dt>同步策略</dt>
                        <dd>
                          每 {selectedRow.config.interval_minutes} 分钟 · TTL{" "}
                          {selectedRow.config.ttl} 秒
                        </dd>
                      </div>
                      <div>
                        <dt>最近同步</dt>
                        <dd className="mono">{selectedRow.lastSync}</dd>
                      </div>
                      <div>
                        <dt>同步结果</dt>
                        <dd className={`tone-${statusTone(selectedStatus)}`}>
                          {selectedStatus}
                        </dd>
                      </div>
                    </dl>
                  </>
                ) : (
                  <div className="binding-preview">
                    <strong>为这台设备绑定一个域名</strong>
                    <p>自动将 A / AAAA 记录更新到所选设备地址。</p>
                  </div>
                )}
              </div>
              <footer className="device-detail-footer">
                <button
                  className="text-button danger-text"
                  disabled={!selectedRow.configured || mutating || syncing}
                  onClick={() => unbindDevice(selectedRow)}
                >
                  <Trash2 size={14} />
                  解除绑定
                </button>
                <div className="toolbar">
                  {selectedRow.enabled ? (
                    <button
                      className="btn btn-secondary"
                      disabled={mutating || syncing || !selectedRow.online}
                      title={
                        !selectedRow.online
                          ? "设备离线，等待上线后再同步"
                          : undefined
                      }
                      onClick={syncNow}
                    >
                      <RefreshCw
                        size={14}
                        className={syncing ? "spinning" : ""}
                      />
                      立即同步
                    </button>
                  ) : null}
                  <button
                    className="btn btn-primary"
                    disabled={mutating || syncing}
                    onClick={() => openConfigDialog(selectedRow)}
                  >
                    <Pencil size={14} />
                    {selectedRow.configured ? "编辑绑定" : "配置绑定"}
                  </button>
                </div>
              </footer>
            </>
          ) : (
            <div className="empty-state">
              选择设备以查看网络信息和 DDNS 绑定
            </div>
          )}
        </section>
      </div>

      {configDialogOpen && selectedRow && draft ? (
        <EditorDrawer
          title="设备 DDNS 解析配置"
          eyebrow="DEVICE BINDING"
          onClose={closeConfigDialog}
          busy={mutating || syncing}
          footer={
            <>
              <button
                className="btn btn-secondary"
                type="button"
                disabled={mutating || syncing || !selectedRow.online}
                title={
                  !selectedRow.online ? "设备离线，等待上线后再同步" : undefined
                }
                onClick={syncNow}
              >
                <Zap size={14} />
                立即同步
              </button>
              <button
                className="btn btn-primary"
                type="button"
                disabled={mutating || syncing}
                onClick={() => saveSelectedConfig()}
              >
                <Save size={14} />
                {mutating ? "正在保存" : "保存绑定"}
              </button>
            </>
          }
        >
          {statusMessage ? (
            <p className={`status-message msg-${messageType}`} role="status">
              {statusMessage}
            </p>
          ) : null}
          <div className="binding-layout">
            <div className="form-grid">
              <label className="field-label">
                <span>DDNS 服务商</span>
                <select
                  value={draft.provider}
                  onChange={(event) =>
                    updateDraft("provider", event.target.value)
                  }
                >
                  <option value="aliyun">阿里云</option>
                </select>
              </label>
              <label className="field-label">
                <span>设备名称</span>
                <input
                  value={draft.device_name}
                  onChange={(event) =>
                    updateDraft("device_name", event.target.value)
                  }
                />
              </label>
              <label className="field-label">
                <span>AccessKey ID</span>
                <input
                  value={draft.access_key_id}
                  type="text"
                  autoComplete="off"
                  onChange={(event) =>
                    updateDraft("access_key_id", event.target.value)
                  }
                />
              </label>
              <label className="field-label">
                <span>子域名（可选）</span>
                <input
                  value={draft.sub_domain}
                  onChange={(event) =>
                    updateDraft("sub_domain", event.target.value)
                  }
                />
              </label>
              <label className="field-label">
                <span>AccessKey Secret</span>
                <input
                  value={draft.access_key_secret}
                  type="password"
                  autoComplete="off"
                  onChange={(event) =>
                    updateDraft("access_key_secret", event.target.value)
                  }
                />
              </label>
              <label className="field-label toggle-row">
                <span>启用同步</span>
                <input
                  type="checkbox"
                  checked={draft.enabled}
                  onChange={(event) => onDraftCheckbox("enabled", event)}
                />
              </label>
              <label className="field-label">
                <span>主域名</span>
                <input
                  value={draft.domain}
                  onChange={(event) =>
                    updateDraft("domain", event.target.value)
                  }
                />
              </label>
              <label className="field-label">
                <span>记录类型</span>
                <select
                  value={draft.record_type}
                  onChange={(event) =>
                    updateDraft("record_type", event.target.value)
                  }
                >
                  <option value="AAAA">AAAA - IPv6</option>
                  <option value="A">A - IPv4</option>
                </select>
              </label>
              <label className="field-label candidate-index-field">
                <span>
                  {recordTypeForConfig(draft) === "AAAA"
                    ? "候选顺位"
                    : "绑定 IP"}
                </span>
                {recordTypeForConfig(draft) === "AAAA" ? (
                  <div className="candidate-index-control">
                    <input
                      value={draft.ip_candidate_index}
                      type="number"
                      min={1}
                      max={Math.max(
                        availableIpOptions.length,
                        draft.ip_candidate_index || 1,
                      )}
                      onChange={(event) =>
                        updateIpv6CandidateIndex(Number(event.target.value))
                      }
                    />
                    <small
                      className={`ipv6-candidate-value${selectedIpv6Candidate ? "" : " is-empty"}`}
                    >
                      {selectedIpv6Candidate
                        ? selectedIpv6Candidate
                        : availableIpOptions.length === 0
                          ? "未发现可用 IPv6 候选"
                          : `第 ${draft.ip_candidate_index} 顺位暂无地址`}
                    </small>
                  </div>
                ) : (
                  <select
                    value={draft.selected_ip}
                    onChange={(event) =>
                      updateDraft("selected_ip", event.target.value)
                    }
                  >
                    <option value="">自动选择</option>
                    {availableIpOptions.map((ip) => (
                      <option key={ip} value={ip}>
                        {ip}
                      </option>
                    ))}
                  </select>
                )}
              </label>
              <label className="field-label">
                <span>最短 TTL</span>
                <input
                  value={draft.ttl}
                  type="number"
                  min={60}
                  max={86400}
                  onChange={(event) =>
                    updateDraft("ttl", Number(event.target.value))
                  }
                />
              </label>
              <label className="field-label">
                <span>绑定设备</span>
                <input
                  value={selectedRow.raw.mac || "未获取到 MAC 地址"}
                  type="text"
                  disabled
                />
              </label>
              <label className="field-label">
                <span>同步间隔</span>
                <input
                  value={draft.interval_minutes}
                  type="number"
                  min={1}
                  max={1440}
                  onChange={(event) =>
                    updateDraft("interval_minutes", Number(event.target.value))
                  }
                />
              </label>
            </div>

            <aside className="preview-card">
              <h3>当前生效预览</h3>
              {previewRows.length === 0 ? (
                <div className="preview-empty">暂无可预览解析</div>
              ) : null}
              {previewRows.map(([device, domain]) => (
                <div key={device} className="preview-row">
                  <span>{device}</span>
                  <strong>→</strong>
                  <span>{domain}</span>
                </div>
              ))}
              <p>共 {previewRows.length} 条绑定</p>
            </aside>
          </div>

          <p className="form-note">
            <Info size={15} />
            保存后由后台按间隔更新，立即同步会同时保存配置。
          </p>
        </EditorDrawer>
      ) : null}
    </div>
  );
}
