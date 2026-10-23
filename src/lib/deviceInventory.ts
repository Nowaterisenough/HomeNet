import type { DeviceDdnsConfig, LanDevice } from "../types";
import { mergeDiscoveredDevices } from "../utils/devices.ts";

export const DEVICE_REFRESH_INTERVAL_MS = 60000;

interface InventoryState {
  devices: LanDevice[];
  configs: DeviceDdnsConfig[];
  devicesLoaded: boolean;
  configsLoaded: boolean;
  loading: boolean;
  lastScan: number | null;
  deviceError: string;
  configError: string;
}

interface InventorySource {
  loadDevices: () => Promise<LanDevice[]>;
  loadConfigs: () => Promise<DeviceDdnsConfig[]>;
  now?: () => number;
  schedule?: (refresh: () => void, interval: number) => () => void;
}

export function createDeviceInventory(source: InventorySource) {
  let state: InventoryState = {
    devices: [],
    configs: [],
    devicesLoaded: false,
    configsLoaded: false,
    loading: false,
    lastScan: null,
    deviceError: "",
    configError: "",
  };
  const subscribers = new Set<() => void>();
  let pending: Promise<void> | null = null;
  let requested = false;
  let configGeneration = 0;
  let owners = 0;
  let stopTimer: (() => void) | undefined;
  const schedule =
    source.schedule ??
    ((refresh, interval) => {
      const timer = setInterval(refresh, interval);
      return () => clearInterval(timer);
    });

  function publish(update: Partial<InventoryState>) {
    state = { ...state, ...update };
    subscribers.forEach((notify) => notify());
  }

  async function refreshConfigs() {
    const generation = ++configGeneration;
    try {
      const configs = await source.loadConfigs();
      if (generation === configGeneration)
        publish({ configs, configsLoaded: true, configError: "" });
      return state.configs;
    } catch (error) {
      if (generation === configGeneration)
        publish({ configError: `读取 DDNS 配置失败：${String(error)}` });
      throw error;
    }
  }

  function refresh() {
    if (pending) return pending;
    requested = true;
    publish({ loading: true });
    pending = (async () => {
      // Configurations can appear immediately, even while LAN discovery is still pending.
      await Promise.all([
        refreshConfigs().catch(() => undefined),
        (async () => {
          try {
            const devices = await source.loadDevices();
            publish({
              devices: mergeDiscoveredDevices(state.devices, devices),
              devicesLoaded: true,
              lastScan: (source.now ?? Date.now)(),
              deviceError: "",
            });
          } catch (error) {
            publish({
              deviceError: `读取局域网设备失败：${String(error)}；保留上次结果`,
            });
          }
        })(),
      ]);
    })().finally(() => {
      pending = null;
      publish({ loading: false });
    });
    return pending;
  }

  return {
    getSnapshot: () => state,
    subscribe(notify: () => void) {
      subscribers.add(notify);
      return () => {
        subscribers.delete(notify);
      };
    },
    refresh,
    refreshConfigs,
    start() {
      if (owners++ === 0) {
        if (!requested) void refresh();
        stopTimer = schedule(() => {
          void refresh();
        }, DEVICE_REFRESH_INTERVAL_MS);
      }
      let stopped = false;
      return () => {
        if (stopped) return;
        stopped = true;
        if (--owners === 0) {
          stopTimer?.();
          stopTimer = undefined;
        }
      };
    },
  };
}
