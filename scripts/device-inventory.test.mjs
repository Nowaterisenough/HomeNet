import assert from "node:assert/strict";
import test from "node:test";
import {
  createDeviceInventory,
  DEVICE_REFRESH_INTERVAL_MS,
} from "../src/lib/deviceInventory.ts";
import {
  deviceRowId,
  matchingDeviceForConfig,
  mergeDiscoveredDevices,
  withBoundDevices,
  normalizeDeviceMac,
} from "../src/utils/devices.ts";

test("redacted MAC values do not merge remote devices with the local machine", () => {
  for (const value of ["02:00:00:00:00:00", "2:0:0:0:0:0", "020000000000"])
    assert.equal(normalizeDeviceMac(value), "");
  const local = device({
    id: "local-machine",
    source: "local-interface",
    mac: "",
  });
  const remote = binding({
    device_id: "local-machine",
    device_name: "Remote PC",
    device_mac: "02:00:00:00:00:00",
  });
  assert.equal(matchingDeviceForConfig(remote, [local]), null);
});

test("IP-only discovery is replaced by a current MAC-backed device without an offline alias", () => {
  const unknown = device({
    id: "ip-192-0-2-10",
    mac: "",
    source: "icmp-reply",
  });
  const known = device();
  assert.deepEqual(mergeDiscoveredDevices([unknown], [known]), [known]);
});

const device = (overrides = {}) => ({
  id: "mac-02-aa-bb-cc-dd-01",
  display_name: "Desktop",
  hostname: "",
  mac: "02:aa:bb:cc:dd:01",
  ipv4: ["192.0.2.10"],
  ipv6: ["2001:db8::10"],
  global_ipv6: ["2001:db8::10"],
  online: true,
  source: "macos-neighbor",
  last_seen: "2026-10-23T02:00:00Z",
  ...overrides,
});
const binding = (overrides = {}) => ({
  device_id: "mac-02-aa-bb-cc-dd-01",
  device_mac: "02:aa:bb:cc:dd:01",
  device_name: "Desktop",
  enabled: true,
  provider: "aliyun",
  domain: "example.test",
  sub_domain: "game",
  record_type: "AAAA",
  selected_ip: "2001:db8::10",
  selected_ipv6: "2001:db8::10",
  ...overrides,
});
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

test("bound devices appear offline on a cold start without becoming IPv6 candidates", () => {
  const rows = withBoundDevices([], [binding()]);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].online, false);
  assert.deepEqual(rows[0].ipv6, ["2001:db8::10"]);
  assert.deepEqual(rows[0].global_ipv6, []);
  assert.equal(rows[0].last_seen, "");
  assert.equal(matchingDeviceForConfig(binding(), rows), rows[0]);
  assert.deepEqual(withBoundDevices([], []), []);
});

test("binding matching normalizes MAC formats and never attaches a remote legacy ID to local hardware", () => {
  const local = device({
    id: "local-machine",
    mac: "02:aa:bb:cc:dd:02",
    source: "local-interface",
  });
  const remote = binding({
    device_id: "local-machine",
    device_mac: "02-AA-BB-CC-DD-01",
  });
  const rows = withBoundDevices([local], [remote]);
  assert.equal(rows.length, 2);
  assert.notEqual(deviceRowId(rows[0]), deviceRowId(rows[1]));
  assert.equal(matchingDeviceForConfig(remote, rows), rows[1]);
  assert.equal(withBoundDevices([device()], [remote]).length, 1);
});

test("known devices survive a partial scan, then recover without duplicate rows", () => {
  const original = device();
  const offline = mergeDiscoveredDevices([original], []);
  assert.equal(offline[0].online, false);
  assert.deepEqual(offline[0].global_ipv6, []);
  assert.equal(offline[0].last_seen, original.last_seen);
  const online = mergeDiscoveredDevices(offline, [
    device({ id: "renamed-device" }),
  ]);
  assert.equal(online.length, 1);
  assert.equal(online[0].online, true);
});

test("page subscriptions and owner remounts reuse the cache; only the timer triggers the next scan", async () => {
  let scans = 0,
    tick,
    stops = 0;
  const store = createDeviceInventory({
    loadDevices: async () => {
      scans++;
      return [device()];
    },
    loadConfigs: async () => [binding()],
    schedule: (refresh, interval) => {
      assert.equal(interval, DEVICE_REFRESH_INTERVAL_MS);
      tick = refresh;
      return () => stops++;
    },
  });
  const stop = store.start();
  await store.refresh();
  assert.equal(scans, 1);
  const unsubscribe = store.subscribe(() => {});
  unsubscribe();
  stop();
  const stopAgain = store.start();
  assert.equal(scans, 1);
  tick();
  await store.refresh();
  assert.equal(scans, 2);
  stopAgain();
  assert.equal(stops, 2);
});

test("overlapping scan requests share one promise and configurations load before the scan completes", async () => {
  const scan = deferred();
  let calls = 0;
  const store = createDeviceInventory({
    loadDevices: () => {
      calls++;
      return scan.promise;
    },
    loadConfigs: async () => [binding()],
  });
  const first = store.refresh();
  assert.equal(store.refresh(), first);
  await Promise.resolve();
  assert.equal(store.getSnapshot().configsLoaded, true);
  assert.equal(store.getSnapshot().devicesLoaded, false);
  assert.equal(store.getSnapshot().loading, true);
  scan.resolve([device()]);
  await first;
  assert.equal(calls, 1);
});

test("scan and configuration failures keep the last successful data and timestamp", async () => {
  let fail = false;
  const store = createDeviceInventory({
    now: () => 1234,
    loadDevices: async () => {
      if (fail) throw Error("scan failed");
      return [device()];
    },
    loadConfigs: async () => {
      if (fail) throw Error("config failed");
      return [binding()];
    },
  });
  await store.refresh();
  const previous = store.getSnapshot();
  fail = true;
  await store.refresh();
  const state = store.getSnapshot();
  assert.equal(state.devices, previous.devices);
  assert.equal(state.configs, previous.configs);
  assert.equal(state.lastScan, 1234);
  assert.equal(state.loading, false);
  assert.match(state.deviceError, /保留上次结果/);
  assert.match(state.configError, /config failed/);
});

test("a slow pre-mutation config response cannot undo a newer save or deletion", async () => {
  const old = deferred();
  let loads = 0;
  const store = createDeviceInventory({
    loadDevices: async () => [],
    loadConfigs: () => (++loads === 1 ? old.promise : Promise.resolve([])),
  });
  const scan = store.refresh();
  await store.refreshConfigs();
  old.resolve([binding()]);
  await scan;
  assert.deepEqual(store.getSnapshot().configs, []);
});
