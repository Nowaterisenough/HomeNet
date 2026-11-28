import type { DeviceDdnsConfig, LanDevice } from "../types";
import { matchingDeviceForConfig, withBoundDevices } from "./devices.ts";

export const offlineDdnsResult = "设备离线，等待上线后自动同步";

export function ddnsDeviceEntries(
  devices: LanDevice[],
  configs: DeviceDdnsConfig[],
) {
  const all = withBoundDevices(devices, configs);
  const bindings = new Map<LanDevice, DeviceDdnsConfig[]>();
  for (const config of configs) {
    // Invalid legacy identities still need an offline row for repair/removal;
    // this display-only fallback must never match a discovered network device.
    const device =
      matchingDeviceForConfig(config, all) ??
      all.find(
        (item) =>
          item.source === "ddns-binding" &&
          !item.online &&
          item.id === config.device_id &&
          item.mac === config.device_mac,
      );
    if (!device) continue;
    bindings.set(device, [...(bindings.get(device) ?? []), config]);
  }
  return all.flatMap((device) => {
    const configs = bindings.get(device);
    if (!configs) return [];
    const online = configs.some((config) => Boolean(currentIpForConfig(config, device)));
    return [{ device, configs, online }];
  });
}

export function uniqueStrings(values: string[]): string[] {
  return values.reduce<string[]>((result, value) => {
    const trimmed = value.trim();
    if (trimmed && !result.includes(trimmed)) result.push(trimmed);
    return result;
  }, []);
}

export function recordTypeForConfig(config: DeviceDdnsConfig | null): "A" | "AAAA" {
  return config?.record_type.trim().toUpperCase() === "A" ? "A" : "AAAA";
}

export function configuredIpForConfig(config: DeviceDdnsConfig | null): string {
  if (!config) return "";
  const recordType = recordTypeForConfig(config);
  if (recordType === "A") return config.selected_ip.trim();
  return (config.selected_ip || config.selected_ipv6).trim();
}

export function availableIpsForRecordType(
  device: LanDevice,
  recordType: "A" | "AAAA",
): string[] {
  if (!device.online) return [];
  if (recordType === "A") {
    return uniqueStrings(device.ipv4);
  }
  return uniqueStrings(device.global_ipv6.filter((ip) => ip.includes(":")));
}

export function currentIpForConfig(
  config: DeviceDdnsConfig | null,
  device: LanDevice,
): string {
  if (!config) return "";
  const recordType = recordTypeForConfig(config);
  const available = availableIpsForRecordType(device, recordType);
  if (recordType === "AAAA") {
    const candidateIndex = Math.max(
      1,
      Math.floor(Number(config.ip_candidate_index) || 1),
    );
    return available[candidateIndex - 1] || "";
  }
  const configured = configuredIpForConfig(config);
  if (configured && available.includes(configured)) return configured;
  return available[0] || "";
}
