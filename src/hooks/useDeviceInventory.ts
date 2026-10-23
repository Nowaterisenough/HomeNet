import { useEffect, useMemo, useSyncExternalStore } from "react";
import { createDeviceInventory } from "../lib/deviceInventory";
import { invokeCommand } from "../lib/tauri";
import type { DeviceDdnsConfig, LanDevice } from "../types";
import { withBoundDevices } from "../utils/devices";

const inventory = createDeviceInventory({
  loadDevices: () => invokeCommand<LanDevice[]>("list_lan_devices"),
  loadConfigs: () =>
    invokeCommand<DeviceDdnsConfig[]>("list_device_ddns_configs"),
});

// Owned by App, not the page: changing tabs never starts another scan or timer.
export function useDeviceInventory() {
  const state = useSyncExternalStore(
    inventory.subscribe,
    inventory.getSnapshot,
  );
  useEffect(() => inventory.start(), []);
  const devices = useMemo(
    () => withBoundDevices(state.devices, state.configs),
    [state.devices, state.configs],
  );
  return {
    ...state,
    devices,
    refresh: inventory.refresh,
    refreshConfigs: inventory.refreshConfigs,
  };
}

export type DeviceInventory = ReturnType<typeof useDeviceInventory>;
