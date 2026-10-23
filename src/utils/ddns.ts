import type { DeviceDdnsConfig, LanDevice } from "../types";
import { normalizeDeviceMac } from "./devices.ts";

export const defaultDeviceDdnsConfig: DeviceDdnsConfig = {
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

export function ddnsRecordKey(
  config: DeviceDdnsConfig,
): [string, string, string] {
  const domain = config.domain.trim().replace(/\.+$/, "").toLowerCase();
  const sub = config.sub_domain.trim().replace(/\.+$/, "").toLowerCase();
  return [
    config.provider.trim().toLowerCase(),
    !sub || sub === "@" ? domain : `${sub}.${domain}`,
    config.record_type.trim().toUpperCase(),
  ];
}

export function ddnsBindingKey(config: DeviceDdnsConfig): string {
  return JSON.stringify(ddnsRecordKey(config));
}

export function newDdnsDraft(
  template?: DeviceDdnsConfig | null,
): DeviceDdnsConfig {
  return {
    ...defaultDeviceDdnsConfig,
    enabled: true,
    provider: template?.provider || "aliyun",
    access_key_id: template?.access_key_id || "",
    access_key_secret: template?.access_key_secret || "",
    domain: template?.domain || "",
    record_type: template?.record_type || "AAAA",
    ttl: template?.ttl || 600,
    interval_minutes: template?.interval_minutes || 10,
  };
}

export function prepareNewDdnsBinding(
  draft: DeviceDdnsConfig,
  device: LanDevice | null,
  manual: boolean,
  existing: DeviceDdnsConfig[],
): { config?: DeviceDdnsConfig; error?: string } {
  if (!manual && !device)
    return { error: "请选择要绑定的设备，或切换到手动添加" };
  const mac = normalizeDeviceMac(manual ? draft.device_mac : device!.mac);
  if (
    !mac ||
    mac === "00:00:00:00:00:00" ||
    parseInt(mac.slice(0, 2), 16) & 1
  ) {
    return { error: "请填写真实设备 MAC 地址，不能使用系统占位值、全零、广播或组播地址" };
  }
  const config: DeviceDdnsConfig = {
    ...draft,
    device_mac: mac,
    device_id: manual ? `mac-${mac.replace(/:/g, "-")}` : device!.id,
    device_name:
      draft.device_name.trim() ||
      device?.hostname ||
      device?.display_name ||
      mac,
    access_key_id: draft.access_key_id.trim(),
    access_key_secret: draft.access_key_secret.trim(),
    domain: draft.domain.trim().replace(/\.+$/, "").toLowerCase(),
    sub_domain: draft.sub_domain.trim().replace(/\.+$/, "").toLowerCase(),
    record_type: draft.record_type.toUpperCase(),
    selected_ip: "",
    selected_ipv6: "",
    last_update_time: "",
    last_result: "",
    last_online: false,
  };
  if (config.sub_domain === "@") config.sub_domain = "";
  if (!config.access_key_id || !config.access_key_secret)
    return { error: "请填写完整的 AccessKey ID 和 Secret" };
  if (!config.domain) return { error: "请填写主域名" };
  if (/[\s/:\\?#,]/.test(config.domain + config.sub_domain))
    return { error: "域名中不能包含协议、路径、空格或逗号" };
  if (!["A", "AAAA"].includes(config.record_type))
    return { error: "记录类型仅支持 A 或 AAAA" };
  if (!Number.isInteger(config.ttl) || config.ttl < 60 || config.ttl > 86400)
    return { error: "TTL 应为 60–86400 秒的整数" };
  if (
    !Number.isInteger(config.interval_minutes) ||
    config.interval_minutes < 1 ||
    config.interval_minutes > 1440
  )
    return { error: "同步间隔应为 1–1440 分钟的整数" };
  if (
    !Number.isInteger(config.ip_candidate_index) ||
    config.ip_candidate_index < 1
  )
    return { error: "候选顺位应为正整数" };
  if (existing.some((item) => ddnsBindingKey(item) === ddnsBindingKey(config)))
    return {
      error: "此域名和记录类型已存在，请编辑原绑定；新增不会覆盖已有配置",
    };
  if (!manual && device!.online) {
    const ips = config.record_type === "A" ? device!.ipv4 : device!.global_ipv6;
    const ip =
      ips[config.record_type === "A" ? 0 : config.ip_candidate_index - 1];
    if (!ip)
      return {
        error:
          config.record_type === "A"
            ? "当前设备没有可用的 IPv4 地址"
            : "当前候选顺位没有可用的稳定公网 IPv6",
      };
    config.selected_ip = ip;
    if (config.record_type === "AAAA") config.selected_ipv6 = ip;
  }
  return { config };
}
