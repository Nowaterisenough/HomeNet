import { ChangeEvent, useEffect, useMemo, useState } from "react";
import { Info, Pencil, RefreshCw, Save, Search, Trash2, Zap } from "lucide-react";
import { useDraggableModal } from "../hooks/useDraggableModal";
import { invokeCommand } from "../lib/tauri";
import type { DeviceDdnsConfig, LanDevice } from "../types";

interface DeviceDdnsPanelProps {
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

function availableIpsForRecordType(device: LanDevice, recordType: "A" | "AAAA"): string[] {
  if (recordType === "A") {
    return uniqueStrings(device.ipv4);
  }
  return uniqueStrings(device.global_ipv6.filter((ip) => ip.includes(":")));
}

function currentIpForConfig(config: DeviceDdnsConfig | null, device: LanDevice): string {
  const recordType = recordTypeForConfig(config);
  const available = availableIpsForRecordType(device, recordType);
  const configured = configuredIpForConfig(config);
  if (configured && available.includes(configured)) return configured;
  return available[0] || "";
}

function normalizeConfig(data: Partial<DeviceDdnsConfig> | null | undefined): DeviceDdnsConfig {
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
    interval_minutes: Number(data?.interval_minutes) || defaultConfig.interval_minutes,
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

function configMatchesDevice(config: DeviceDdnsConfig, device: LanDevice): boolean {
  const configId = config.device_id.trim();
  if (configId && configId === device.id) return true;
  const configMac = config.device_mac.trim();
  return !!configMac && !!device.mac && configMac.toLowerCase() === device.mac.toLowerCase();
}

function configForDevice(device: LanDevice, configs: DeviceDdnsConfig[]): DeviceDdnsConfig | null {
  return configs.find((config) => configMatchesDevice(config, device)) ?? null;
}

function selectedIpForDraft(config: DeviceDdnsConfig | null, device: LanDevice): string {
  return currentIpForConfig(config, device) || configuredIpForConfig(config) || "-";
}

function boundIpForConfig(config: DeviceDdnsConfig | null, device: LanDevice): string {
  if (!config) return "-";
  return currentIpForConfig(config, device) || "-";
}

function configuredDomain(config: DeviceDdnsConfig | null): string {
  const domain = config?.domain.trim() ?? "";
  const sub = config?.sub_domain.trim() ?? "";
  if (!domain) return "-";
  return sub ? `${sub}.${domain}` : domain;
}

function mapLanDevice(device: LanDevice, index: number, configs: DeviceDdnsConfig[]): DeviceRow {
  const config = configForDevice(device, configs);
  const nativeName = displayName(device, index);
  return {
    id: device.id || device.mac || nativeName,
    name: config?.device_name.trim() || nativeName,
    nativeName,
    mac: device.mac || "-",
    online: device.online,
    configured: Boolean(config),
    enabled: Boolean(config?.enabled),
    domain: configuredDomain(config),
    lastSync: config?.last_update_time || "-",
    boundIp: boundIpForConfig(config, device),
    raw: device,
    config,
  };
}

function buildDraft(row: DeviceRow, configs: DeviceDdnsConfig[]): DeviceDdnsConfig {
  const existing = row.config;
  const template = existing ?? configs[0] ?? defaultConfig;
  const recordType = existing?.record_type?.trim().toUpperCase() === "A" ? "A" : "AAAA";
  const selectedIp = selectedIpForDraft(existing, row.raw);
  const normalizedSelectedIp = selectedIp === "-" ? "" : selectedIp;
  return normalizeConfig({
    ...template,
    enabled: existing?.enabled ?? true,
    device_id: row.raw.id,
    device_mac: row.raw.mac,
    device_name: existing?.device_name || row.name || row.nativeName,
    record_type: recordType,
    selected_ip: normalizedSelectedIp,
    selected_ipv6: recordType === "AAAA" ? normalizedSelectedIp : "",
    last_update_time: existing?.last_update_time || "",
    last_result: existing?.last_result || "",
    last_online: existing?.last_online ?? false,
    sub_domain: existing?.sub_domain || "",
  });
}

export default function DeviceDdnsPanel({ className = "" }: DeviceDdnsPanelProps) {
  const [devices, setDevices] = useState<LanDevice[]>([]);
  const [configs, setConfigs] = useState<DeviceDdnsConfig[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [draft, setDraft] = useState<DeviceDdnsConfig | null>(null);
  const [configDialogOpen, setConfigDialogOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [statusMessage, setStatusMessage] = useState("");
  const [messageType, setMessageType] = useState<"info" | "success" | "error">("info");
  const { modalStyle, resetModalPosition, startModalDrag } = useDraggableModal();

  const rows = useMemo(
    () => devices.map((device, index) => mapLanDevice(device, index, configs)),
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
    return availableIpsForRecordType(selectedRow.raw, recordTypeForConfig(draft));
  }, [draft, selectedRow]);
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

  function syncDraftWithSelection(nextConfigs = configs, nextRows = rows, nextSelectedId = selectedId) {
    const row = nextRows.find((item) => item.id === nextSelectedId) ?? null;
    setDraft(row ? buildDraft(row, nextConfigs) : null);
  }

  async function loadConfigsData(): Promise<DeviceDdnsConfig[]> {
    return (await invokeCommand<DeviceDdnsConfig[]>("list_device_ddns_configs")).map(normalizeConfig);
  }

  async function loadDevices(options: { refreshDraft?: boolean; showLoading?: boolean } = {}) {
    const refreshDraft = options.refreshDraft ?? true;
    const showLoading = options.showLoading ?? true;
    if (showLoading) setLoading(true);
    try {
      const nextConfigs = await loadConfigsData();
      const nextDevices = await invokeCommand<LanDevice[]>("list_lan_devices");
      const nextRows = nextDevices.map((device, index) => mapLanDevice(device, index, nextConfigs));
      const current = nextRows.find((row) => row.id === selectedId);
      const next = current ?? nextRows.find((row) => row.configured) ?? nextRows[0] ?? null;

      setConfigs(nextConfigs);
      setDevices(nextDevices);
      setSelectedId(next?.id ?? "");
      if (refreshDraft || !configDialogOpen) {
        setDraft(next ? buildDraft(next, nextConfigs) : null);
      }
      setStatusMessage("");
      window.dispatchEvent(new CustomEvent("homenet:devices-refresh"));
    } catch (error) {
      setDevices([]);
      setConfigs([]);
      setSelectedId("");
      setDraft(null);
      setStatusMessage(`读取局域网设备失败：${String(error)}`);
      setMessageType("error");
    } finally {
      if (showLoading) setLoading(false);
    }
  }

  function selectRow(row: DeviceRow) {
    setSelectedId(row.id);
    if (!configDialogOpen) {
      setDraft(buildDraft(row, configs));
    }
  }

  function openConfigDialog(row: DeviceRow) {
    setSelectedId(row.id);
    setDraft(buildDraft(row, configs));
    resetModalPosition();
    setConfigDialogOpen(true);
  }

  function closeConfigDialog() {
    setConfigDialogOpen(false);
    syncDraftWithSelection();
  }

  function notifyDataChanged() {
    window.dispatchEvent(new CustomEvent("homenet:logs-refresh"));
    window.dispatchEvent(new CustomEvent("homenet:refresh-status"));
    window.dispatchEvent(new CustomEvent("homenet:devices-refresh"));
  }

  function validateDraft(requireEnabled: boolean): string {
    if (!selectedRow || !draft) return "请选择要配置的局域网设备";
    if (!selectedRow.raw.mac.trim()) return "绑定设备必须有 MAC 地址";
    if (requireEnabled && !draft.enabled) return "请先启用同步";
    if (!draft.access_key_id.trim() || !draft.access_key_secret.trim()) {
      return "请填写完整的 AccessKey ID 和 Secret";
    }
    if (!draft.domain.trim()) return "请填写主域名";
    if (!["A", "AAAA"].includes(draft.record_type.trim().toUpperCase())) {
      return "记录类型仅支持 A 或 AAAA";
    }
    if (draft.record_type.trim().toUpperCase() === "AAAA") {
      if (availableIpOptions.length === 0) return "当前设备没有可用于 DDNS 的稳定公网 IPv6";
      if (draft.selected_ip && !availableIpOptions.includes(draft.selected_ip)) {
        return "已选 IPv6 不再适合 DDNS，请重新选择";
      }
    }
    if (draft.sub_domain.trim().includes(",")) return "每台设备请填写一个独立子域名";
    return "";
  }

  function buildPayload(): DeviceDdnsConfig | null {
    if (!selectedRow || !draft) return null;
    const recordType = draft.record_type.trim().toUpperCase() === "A" ? "A" : "AAAA";
    const selectedIp =
      draft.selected_ip ||
      (recordType === "A" ? selectedRow.raw.ipv4[0] : firstGlobalIpv6(selectedRow.raw)) ||
      "";
    return {
      ...draft,
      provider: draft.provider || defaultConfig.provider,
      sub_domain: draft.sub_domain.trim(),
      record_type: recordType,
      ttl: Number(draft.ttl) || defaultConfig.ttl,
      interval_minutes: Number(draft.interval_minutes) || defaultConfig.interval_minutes,
      device_id: selectedRow.raw.id,
      device_mac: selectedRow.raw.mac,
      device_name: deviceDisplayNameForDraft(),
      selected_ip: selectedIp,
      selected_ipv6: recordType === "AAAA" ? selectedIp : "",
    };
  }

  async function saveSelectedConfig(showSuccess = true): Promise<DeviceDdnsConfig | null> {
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
      const nextRows = devices.map((device, index) => mapLanDevice(device, index, nextConfigs));
      setConfigs(nextConfigs);
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
      const nextRows = devices.map((device, index) => mapLanDevice(device, index, nextConfigs));
      setConfigs(nextConfigs);
      syncDraftWithSelection(nextConfigs, nextRows, selectedId === row.id ? row.id : selectedId);
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
      const result = await invokeCommand<string>("trigger_device_ddns_update", { config: payload });
      const nextConfigs = await loadConfigsData();
      const nextRows = devices.map((device, index) => mapLanDevice(device, index, nextConfigs));
      setConfigs(nextConfigs);
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

  function updateDraft<K extends keyof DeviceDdnsConfig>(key: K, value: DeviceDdnsConfig[K]) {
    setDraft((current) => (current ? { ...current, [key]: value } : current));
  }

  function onDraftCheckbox(key: keyof DeviceDdnsConfig, event: ChangeEvent<HTMLInputElement>) {
    updateDraft(key, event.target.checked as never);
  }

  useEffect(() => {
    loadDevices();
    const deviceRefreshTimer = window.setInterval(() => {
      loadDevices({ refreshDraft: false, showLoading: false });
    }, 60000);
    return () => window.clearInterval(deviceRefreshTimer);
  }, []);

  return (
    <div className={`device-ddns-stack ${className}`.trim()}>
      <section className="panel devices-list-panel">
        <header className="panel-header panel-header-large">
          <h2>局域网设备与 DDNS 绑定</h2>
          <div className="toolbar">
            <button className="btn btn-secondary" type="button" disabled={loading} onClick={() => loadDevices()}>
              <RefreshCw className={loading ? "spinning" : undefined} size={13} strokeWidth={2.2} />
              刷新设备
            </button>
            <label className="search-box">
              <Search size={13} strokeWidth={2.1} />
              <input
                value={searchQuery}
                type="search"
                placeholder="搜索设备名称、绑定 IP 或 MAC"
                onChange={(event) => setSearchQuery(event.target.value)}
              />
            </label>
          </div>
        </header>

        {statusMessage ? <p className={`status-message msg-${messageType}`}>{statusMessage}</p> : null}

        <div className="table-wrap">
          <table className="device-table">
            <thead>
              <tr>
                <th>设备名称</th>
                <th>在线状态</th>
                <th>DDNS 状态</th>
                <th>MAC 地址</th>
                <th>域名地址</th>
                <th>绑定 IP</th>
                <th>最后同步</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {filteredDevices.length === 0 ? (
                <tr>
                  <td className="empty-cell" colSpan={8}>
                    {loading ? "正在扫描局域网设备" : "暂无局域网设备"}
                  </td>
                </tr>
              ) : (
                filteredDevices.map((device) => (
                  <tr
                    key={device.id}
                    className={selectedId === device.id ? "selected-row" : undefined}
                    onClick={() => selectRow(device)}
                  >
                    <td title={device.nativeName}>{device.name}</td>
                    <td>
                      <span className={`state-pill ${device.online ? "pill-online" : "pill-offline"}`}>
                        {device.online ? "在线" : "离线"}
                      </span>
                    </td>
                    <td>
                      <span className={`bind-pill ${device.configured ? "pill-bound" : "pill-unbound"}`}>
                        {device.configured ? (device.enabled ? "已启用" : "已配置") : "未配置"}
                      </span>
                    </td>
                    <td>{device.mac}</td>
                    <td>{device.domain}</td>
                    <td title={device.boundIp}>{device.boundIp}</td>
                    <td>{device.lastSync}</td>
                    <td>
                      <div className="row-actions device-row-actions">
                        <button
                          className="icon-action"
                          type="button"
                          title="编辑绑定"
                          onClick={(event) => {
                            event.stopPropagation();
                            openConfigDialog(device);
                          }}
                        >
                          <Pencil size={13} strokeWidth={2.1} />
                        </button>
                        <button
                          className="icon-action"
                          type="button"
                          title="解除绑定"
                          disabled={mutating || !device.configured}
                          onClick={(event) => {
                            event.stopPropagation();
                            unbindDevice(device);
                          }}
                        >
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
      </section>

      {configDialogOpen && selectedRow && draft ? (
        <div className="modal-backdrop" onClick={(event) => event.currentTarget === event.target && closeConfigDialog()}>
          <section className="modal-dialog binding-panel" style={modalStyle}>
            <header className="modal-header draggable-header" onPointerDown={startModalDrag}>
              <h2>设备 DDNS 解析配置</h2>
              <button
                className="btn btn-secondary"
                type="button"
                onPointerDown={(event) => event.stopPropagation()}
                onClick={closeConfigDialog}
              >
                关闭
              </button>
            </header>

            <div className="binding-layout">
              <div className="form-grid">
                <label className="field-label">
                  <span>DDNS 服务商</span>
                  <select value={draft.provider} onChange={(event) => updateDraft("provider", event.target.value)}>
                    <option value="aliyun">阿里云</option>
                  </select>
                </label>
                <label className="field-label">
                  <span>设备名称</span>
                  <input value={draft.device_name} onChange={(event) => updateDraft("device_name", event.target.value)} />
                </label>
                <label className="field-label">
                  <span>AccessKey ID</span>
                  <input
                    value={draft.access_key_id}
                    type="text"
                    autoComplete="off"
                    onChange={(event) => updateDraft("access_key_id", event.target.value)}
                  />
                </label>
                <label className="field-label">
                  <span>子域名（可选）</span>
                  <input value={draft.sub_domain} onChange={(event) => updateDraft("sub_domain", event.target.value)} />
                </label>
                <label className="field-label">
                  <span>AccessKey Secret</span>
                  <input
                    value={draft.access_key_secret}
                    type="password"
                    autoComplete="off"
                    onChange={(event) => updateDraft("access_key_secret", event.target.value)}
                  />
                </label>
                <label className="field-label toggle-row">
                  <span>启用同步</span>
                  <input type="checkbox" checked={draft.enabled} onChange={(event) => onDraftCheckbox("enabled", event)} />
                </label>
                <label className="field-label">
                  <span>主域名</span>
                  <input value={draft.domain} onChange={(event) => updateDraft("domain", event.target.value)} />
                </label>
                <label className="field-label">
                  <span>记录类型</span>
                  <select value={draft.record_type} onChange={(event) => updateDraft("record_type", event.target.value)}>
                    <option value="AAAA">AAAA - IPv6</option>
                    <option value="A">A - IPv4</option>
                  </select>
                </label>
                <label className="field-label">
                  <span>绑定 IP</span>
                  <select value={draft.selected_ip} onChange={(event) => updateDraft("selected_ip", event.target.value)}>
                    <option value="">自动选择</option>
                    {availableIpOptions.map((ip) => (
                      <option key={ip} value={ip}>
                        {ip}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field-label">
                  <span>最短 TTL</span>
                  <input
                    value={draft.ttl}
                    type="number"
                    min={60}
                    max={86400}
                    onChange={(event) => updateDraft("ttl", Number(event.target.value))}
                  />
                </label>
                <label className="field-label">
                  <span>绑定设备</span>
                  <input value={selectedRow.raw.mac || "未获取到 MAC 地址"} type="text" disabled />
                </label>
                <label className="field-label">
                  <span>同步间隔</span>
                  <input
                    value={draft.interval_minutes}
                    type="number"
                    min={1}
                    max={1440}
                    onChange={(event) => updateDraft("interval_minutes", Number(event.target.value))}
                  />
                </label>
              </div>

              <aside className="preview-card">
                <h3>当前生效预览</h3>
                {previewRows.length === 0 ? <div className="preview-empty">暂无可预览解析</div> : null}
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

            <footer className="panel-footer binding-footer">
              <span>
                <Info className="info-dot" size={13} strokeWidth={2.1} aria-hidden="true" />
                保存后由后台任务按间隔更新，立即同步会调用真实 DDNS 接口。
              </span>
              <div className="footer-actions">
                <button className="btn btn-primary" type="button" disabled={mutating} onClick={() => saveSelectedConfig()}>
                  <Save size={13} strokeWidth={2.2} />
                  保存绑定
                </button>
                <button className="btn btn-secondary" type="button" disabled={syncing} onClick={syncNow}>
                  <Zap size={13} strokeWidth={2.2} />
                  立即同步
                </button>
              </div>
            </footer>
          </section>
        </div>
      ) : null}
    </div>
  );
}
