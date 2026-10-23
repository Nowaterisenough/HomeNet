import { useState } from "react";
import { Info, Plus } from "lucide-react";
import EditorDrawer from "./EditorDrawer";
import DevicePicker from "./DevicePicker";
import { invokeCommand } from "../lib/tauri";
import type { DeviceDdnsConfig, LanDevice } from "../types";
import { deviceRowId, normalizeDeviceMac } from "../utils/devices";
import { newDdnsDraft, prepareNewDdnsBinding } from "../utils/ddns";
import { operationError } from "../utils/errors";

interface Props {
  devices: LanDevice[];
  configs: DeviceDdnsConfig[];
  initialDevice: LanDevice | null;
  template: DeviceDdnsConfig | null;
  loading: boolean;
  discoveryError: string;
  onRefreshDevices: () => Promise<void>;
  onClose: () => void;
  onCreated: (binding: DeviceDdnsConfig) => Promise<void>;
}

export default function CreateDdnsBindingDialog({
  devices,
  configs,
  initialDevice,
  template,
  loading,
  discoveryError,
  onRefreshDevices,
  onClose,
  onCreated,
}: Props) {
  const initial = initialDevice ?? devices[0] ?? null;
  const [manual, setManual] = useState(false);
  const [deviceId, setDeviceId] = useState(initial ? deviceRowId(initial) : "");
  const [deviceMac, setDeviceMac] = useState(
    normalizeDeviceMac(initial?.mac || ""),
  );
  const [draft, setDraft] = useState(() => ({
    ...newDdnsDraft(template),
    device_name: initial?.hostname || initial?.display_name || "",
  }));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [created, setCreated] = useState<DeviceDdnsConfig | null>(null);
  const device = manual
    ? null
    : (devices.find((item) => deviceRowId(item) === deviceId) ??
      (deviceMac
        ? devices.find((item) => normalizeDeviceMac(item.mac) === deviceMac)
        : null) ??
      null);
  function update<K extends keyof DeviceDdnsConfig>(
    key: K,
    value: DeviceDdnsConfig[K],
  ) {
    setDraft((current) => ({ ...current, [key]: value }));
    setError("");
  }
  async function save() {
    let saved = created;
    const result = saved
      ? { config: saved }
      : prepareNewDdnsBinding(draft, device, manual, configs);
    if (!result.config) {
      setError(result.error || "请检查配置");
      return;
    }
    setSaving(true);
    setError("");
    try {
      if (!saved) {
        saved = await invokeCommand<DeviceDdnsConfig>(
          "create_device_ddns_config",
          { config: result.config },
        );
        setCreated(saved);
      }
      await onCreated(saved);
    } catch (error) {
      setError(
        operationError(
          saved ? "绑定已新增，刷新列表失败" : "新增绑定失败",
          error,
        ),
      );
    } finally {
      setSaving(false);
    }
  }
  return (
    <EditorDrawer
      title="新增 DDNS 绑定"
      onClose={onClose}
      busy={saving}
      footer={
        <>
          <button
            className="btn btn-secondary"
            disabled={saving}
            onClick={onClose}
          >
            取消
          </button>
          <button className="btn btn-primary" disabled={saving} onClick={save}>
            <Plus size={14} />
            {saving ? "正在保存" : created ? "刷新列表" : "新增绑定"}
          </button>
        </>
      }
    >
      {error ? (
        <p className="status-message msg-error" role="status">
          {error}
        </p>
      ) : null}
      <fieldset
        className="ddns-create-form"
        disabled={saving || Boolean(created)}
      >
        <div className="filter-tabs" aria-label="设备添加方式">
          <button
            type="button"
            aria-pressed={!manual}
            onClick={() => setManual(false)}
          >
            局域网设备
          </button>
          <button
            type="button"
            aria-pressed={manual}
            onClick={() => setManual(true)}
          >
            手动添加设备
          </button>
        </div>
        {!manual ? (
          <div className="field-label ddns-create-target">
            <span>绑定设备</span>
            <DevicePicker
              devices={devices}
              configs={configs}
              value={device}
              loading={loading}
              error={discoveryError}
              disabled={saving || Boolean(created)}
              onRefresh={onRefreshDevices}
              onChange={(target) => {
                setDeviceId(deviceRowId(target));
                setDeviceMac(normalizeDeviceMac(target.mac));
                update(
                  "device_name",
                  target.hostname || target.display_name || "",
                );
              }}
            />
            <small>
              每 60
              秒自动扫描，也可手动刷新。未发现其他设备时，请检查本地网络权限和路由器的设备隔离设置。
            </small>
          </div>
        ) : null}
        <div className="form-grid">
          <label className="field-label">
            <span>设备名称</span>
            <input
              value={draft.device_name}
              onChange={(event) => update("device_name", event.target.value)}
              placeholder="例如 游戏主机"
            />
          </label>
          <label className="field-label">
            <span>设备 MAC 地址</span>
            <input
              value={manual ? draft.device_mac : device?.mac || ""}
              readOnly={!manual}
              onChange={(event) => update("device_mac", event.target.value)}
              placeholder="AA:BB:CC:DD:EE:FF"
            />
          </label>
          <label className="field-label">
            <span>AccessKey ID</span>
            <input
              value={draft.access_key_id}
              autoComplete="off"
              onChange={(event) => update("access_key_id", event.target.value)}
            />
          </label>
          <label className="field-label">
            <span>AccessKey Secret</span>
            <input
              value={draft.access_key_secret}
              type="password"
              autoComplete="off"
              onChange={(event) =>
                update("access_key_secret", event.target.value)
              }
            />
          </label>
          <label className="field-label">
            <span>主域名</span>
            <input
              value={draft.domain}
              placeholder="example.com"
              onChange={(event) => update("domain", event.target.value)}
            />
          </label>
          <label className="field-label">
            <span>子域名（可选）</span>
            <input
              value={draft.sub_domain}
              placeholder="例如 game，留空表示主域名"
              onChange={(event) => update("sub_domain", event.target.value)}
            />
          </label>
          <label className="field-label">
            <span>记录类型</span>
            <select
              value={draft.record_type}
              onChange={(event) => update("record_type", event.target.value)}
            >
              <option value="AAAA">AAAA - IPv6</option>
              <option value="A">A - IPv4</option>
            </select>
          </label>
          <label className="field-label">
            <span>IPv6 候选顺位</span>
            <input
              type="number"
              min={1}
              value={draft.ip_candidate_index}
              disabled={draft.record_type === "A"}
              onChange={(event) =>
                update("ip_candidate_index", Number(event.target.value))
              }
            />
          </label>
          <label className="field-label">
            <span>TTL（秒）</span>
            <input
              type="number"
              min={60}
              max={86400}
              value={draft.ttl}
              onChange={(event) => update("ttl", Number(event.target.value))}
            />
          </label>
          <label className="field-label">
            <span>同步间隔（分钟）</span>
            <input
              type="number"
              min={1}
              max={1440}
              value={draft.interval_minutes}
              onChange={(event) =>
                update("interval_minutes", Number(event.target.value))
              }
            />
          </label>
        </div>
        <label className="setting-toggle">
          <span>
            <strong>启用自动同步</strong>
            <small>保存后由后台同步；离线设备等待被发现后再更新。</small>
          </span>
          <input
            type="checkbox"
            checked={draft.enabled}
            onChange={(event) => update("enabled", event.target.checked)}
          />
        </label>
      </fieldset>
      <p className="form-note">
        <Info size={15} />
        {manual
          ? "按 MAC 地址识别设备；手动添加不会绕过本地网络权限，也不会把未经发现的 IP 用于同步。"
          : "新增不会覆盖已有绑定。服务商为阿里云 DNS；已填入的凭据沿用当前配置，请确认域名。"}
      </p>
    </EditorDrawer>
  );
}
