import type { DeviceDdnsConfig, LanDevice } from "../types";
import { matchingDeviceForConfig, withBoundDevices } from "./devices.ts";

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
  return all
    .filter((device) => bindings.has(device))
    .map((device) => ({
      device,
      configs: bindings.get(device)!,
    }));
}
