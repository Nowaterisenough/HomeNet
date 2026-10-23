import type { DeviceDdnsConfig, LanDevice } from "../types";

export function normalizeDeviceMac(value: string): string {
  const parts = value.trim().toLowerCase().split(/[:-]/);
  if (
    parts.length === 6 &&
    parts.every((part) => /^[0-9a-f]{1,2}$/.test(part))
  ) {
    const normalized = parts.map((part) => part.padStart(2, "0")).join(":");
    return normalized === "02:00:00:00:00:00" ? "" : normalized;
  }
  const compact = value.trim().replace(/[.:-]/g, "").toLowerCase();
  return compact !== "020000000000" && /^[0-9a-f]{12}$/.test(compact)
    ? compact.match(/../g)!.join(":")
    : "";
}

export function deviceRowId(device: LanDevice): string {
  if (device.id === "local-machine" && device.source === "local-interface")
    return "local-machine";
  const mac = normalizeDeviceMac(device.mac);
  return mac ? `mac:${mac}` : `id:${device.id || device.display_name}`;
}

export function isGeneratedLocalMachineConfig(
  config: DeviceDdnsConfig,
): boolean {
  return (
    config.device_id.trim() === "local-machine" &&
    config.device_name.trim().startsWith("本机设备")
  );
}

export function matchingDeviceForConfig(
  config: DeviceDdnsConfig,
  devices: LanDevice[],
): LanDevice | null {
  const mac = normalizeDeviceMac(config.device_mac);
  if (mac) {
    const match = devices.find(
      (device) => normalizeDeviceMac(device.mac) === mac,
    );
    if (match) return match;
  }
  if (isGeneratedLocalMachineConfig(config)) {
    return (
      devices.find(
        (device) =>
          device.id === "local-machine" && device.source === "local-interface",
      ) ?? null
    );
  }
  if (config.device_mac.trim() && !mac) return null;
  const id = config.device_id.trim();
  return (
    devices.find(
      (device) =>
        id &&
        device.id.trim() === id &&
        (!mac || !normalizeDeviceMac(device.mac)),
    ) ?? null
  );
}

export function mergeDiscoveredDevices(
  previous: LanDevice[],
  current: LanDevice[],
): LanDevice[] {
  const devices = new Map(
    current.map((device) => [deviceRowId(device), device]),
  );
  for (const device of previous) {
    if (
      device.source === "icmp-reply" &&
      current.some(
        (next) =>
          normalizeDeviceMac(next.mac) &&
          next.ipv4.some((ip) => device.ipv4.includes(ip)),
      )
    )
      continue;
    if (!devices.has(deviceRowId(device))) {
      // Remember devices for this session without presenting stale addresses as sync candidates.
      devices.set(deviceRowId(device), {
        ...device,
        online: false,
        global_ipv6: [],
      });
    }
  }
  return [...devices.values()];
}

export function withBoundDevices(
  devices: LanDevice[],
  configs: DeviceDdnsConfig[],
): LanDevice[] {
  const result = devices.slice();
  for (const config of configs) {
    if (matchingDeviceForConfig(config, result)) continue;
    const ip = (config.selected_ip || config.selected_ipv6).trim();
    result.push({
      id: config.device_id || `bound-${normalizeDeviceMac(config.device_mac)}`,
      display_name: config.device_name || config.device_mac || "已绑定设备",
      hostname: "",
      mac: config.device_mac,
      ipv4: ip && !ip.includes(":") ? [ip] : [],
      ipv6: ip.includes(":") ? [ip] : [],
      global_ipv6: [],
      online: false,
      source: "ddns-binding",
      last_seen: "",
    });
  }
  return result;
}
