import type { DeviceDdnsConfig, LanDevice } from "../types";
import {
  deviceRowId,
  matchingDeviceForConfig,
  normalizeDeviceMac,
  withBoundDevices,
} from "./devices.ts";

export function devicePickerOptions(
  devices: LanDevice[],
  configs: DeviceDdnsConfig[],
  query = "",
) {
  const all = withBoundDevices(devices, configs);
  const bound = new Set(
    configs
      .map((config) => matchingDeviceForConfig(config, all))
      .filter(Boolean),
  );
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const queryMac = normalizeDeviceMac(query);
  return all
    .map((device) => ({
      id: deviceRowId(device),
      device,
      label:
        device.hostname ||
        device.display_name ||
        device.mac ||
        device.ipv4[0] ||
        "未命名设备",
      bound: bound.has(device),
      address: device.ipv4[0] || device.ipv6[0] || "暂无地址",
      selectable: Boolean(normalizeDeviceMac(device.mac)),
    }))
    .filter((option) => {
      const text = [
        option.label,
        option.device.mac,
        ...option.device.ipv4,
        ...option.device.ipv6,
      ]
        .join(" ")
        .toLowerCase();
      return (
        terms.every((term) => text.includes(term)) ||
        Boolean(queryMac && queryMac === normalizeDeviceMac(option.device.mac))
      );
    })
    .sort(
      (a, b) =>
        Number(b.device.online) - Number(a.device.online) ||
        Number(b.device.source === "local-interface") -
          Number(a.device.source === "local-interface") ||
        a.label.localeCompare(b.label, "zh-CN", { numeric: true }) ||
        a.id.localeCompare(b.id),
    );
}
