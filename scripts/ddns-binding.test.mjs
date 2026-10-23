import assert from "node:assert/strict";
import test from "node:test";
import {
  defaultDeviceDdnsConfig,
  ddnsBindingKey,
  newDdnsDraft,
  prepareNewDdnsBinding,
} from "../src/utils/ddns.ts";
import { withBoundDevices } from "../src/utils/devices.ts";
import { ddnsDeviceEntries } from "../src/utils/ddnsDevices.ts";
import { devicePickerOptions } from "../src/utils/devicePicker.ts";

const mac = ["02", "aa", "bb", "cc", "dd", "01"].join(":");
const base = () => ({
  ...defaultDeviceDdnsConfig,
  enabled: true,
  domain: "example.test",
  access_key_id: "test-key",
  access_key_secret: "test-secret",
  device_mac: mac,
  device_name: "Test device",
});
const device = {
  id: "device",
  display_name: "Test device",
  hostname: "",
  mac,
  ipv4: ["192.0.2.10"],
  ipv6: ["2001:db8::10"],
  global_ipv6: ["2001:db8::10"],
  online: true,
  source: "test",
  last_seen: "",
};

test("new drafts copy account defaults but not the existing binding or runtime fields", () => {
  const next = newDdnsDraft({
    ...base(),
    sub_domain: "old",
    device_id: "old",
    selected_ip: "2001:db8::10",
    last_result: "success",
    last_online: true,
  });
  assert.equal(next.access_key_id, "test-key");
  assert.equal(next.domain, "example.test");
  for (const key of ["sub_domain", "device_id", "selected_ip", "last_result"])
    assert.equal(next[key], "");
  assert.equal(next.last_online, false);
});
test("manual offline creation normalizes the identity and never invents a live IP", () => {
  const result = prepareNewDdnsBinding(
    {
      ...base(),
      device_mac: mac.toUpperCase().replaceAll(":", "-"),
      selected_ip: "stale",
    },
    null,
    true,
    [],
  );
  assert.ok(result.config, result.error);
  assert.equal(result.config.device_mac, mac);
  assert.equal(result.config.device_id, "mac-02-aa-bb-cc-dd-01");
  assert.equal(result.config.selected_ip, "");
  assert.equal(withBoundDevices([], [result.config])[0].online, false);
});
test("one device can own distinct domains and record types without duplicating device rows", () => {
  const root = base();
  const extra = prepareNewDdnsBinding(
    { ...base(), sub_domain: "game" },
    device,
    false,
    [root],
  );
  assert.ok(extra.config, extra.error);
  assert.equal(extra.config.selected_ipv6, "2001:db8::10");
  assert.equal(withBoundDevices([device], [root, extra.config]).length, 1);
  assert.ok(
    prepareNewDdnsBinding({ ...base(), record_type: "A" }, device, false, [
      root,
    ]).config,
  );
});
test("duplicate records are rejected regardless of case or root alias", () => {
  assert.equal(
    ddnsBindingKey({ ...base(), domain: "Example.Test.", sub_domain: "@" }),
    ddnsBindingKey(base()),
  );
  assert.match(
    prepareNewDdnsBinding({ ...base(), sub_domain: "@" }, device, false, [
      base(),
    ]).error,
    /已存在/,
  );
});
test("manual invalid MACs, missing credentials and invalid ranges fail validation", () => {
  for (const value of [
    "",
    "not-a-mac",
    "00:00:00:00:00:00",
    "02:00:00:00:00:00",
    "ff:ff:ff:ff:ff:ff",
  ])
    assert.ok(
      prepareNewDdnsBinding({ ...base(), device_mac: value }, null, true, [])
        .error,
    );
  for (const changed of [
    { access_key_secret: "" },
    { domain: "" },
    { domain: "https://example.test" },
    { ttl: 0 },
    { interval_minutes: 0 },
    { ip_candidate_index: 0 },
  ])
    assert.ok(
      prepareNewDdnsBinding({ ...base(), ...changed }, null, true, []).error,
    );
});
test("discovered online devices need a current candidate but offline devices can be saved pending discovery", () => {
  assert.ok(prepareNewDdnsBinding(base(), null, false, []).error);
  assert.ok(
    prepareNewDdnsBinding(
      { ...base(), ip_candidate_index: 2 },
      device,
      false,
      [],
    ).error,
  );
  assert.ok(
    prepareNewDdnsBinding(
      base(),
      { ...device, online: false, global_ipv6: [] },
      false,
      [],
    ).config,
  );
});

test("DDNS page lists only bound devices while the creation picker retains all LAN devices", () => {
  const unbound = { ...device, id: "nas", mac: mac.replace(/01$/, "02") };
  const offline = {
    ...base(),
    enabled: false,
    device_id: "offline",
    device_mac: mac.replace(/01$/, "03"),
    sub_domain: "offline",
  };
  const devices = [device, unbound];
  const configs = [base(), offline];
  const rows = ddnsDeviceEntries(devices, configs);
  assert.equal(rows.length, 2);
  assert.equal(rows[0].device.id, device.id);
  assert.equal(rows[1].device.online, false);
  assert.equal(rows[1].configs[0].enabled, false);
  assert.equal(devicePickerOptions(devices, configs).length, 3);
  assert.equal(devices.length, 2);
  assert.deepEqual(ddnsDeviceEntries(devices, []), []);
});

test("DDNS device rows appear after creation and disappear only after the last binding is removed", () => {
  const first = base();
  const second = { ...base(), sub_domain: "game" };
  const rows = ddnsDeviceEntries([device], [first, second]);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].configs.length, 2);
  assert.equal(ddnsDeviceEntries([device], [second]).length, 1);
  assert.equal(ddnsDeviceEntries([device], []).length, 0);
  assert.equal(devicePickerOptions([device], []).length, 1);
});

test("legacy bindings with a redacted MAC remain visible offline for repair without claiming a local device", () => {
  const local = {
    ...device,
    id: "local-machine",
    source: "local-interface",
    mac: "",
  };
  const legacy = {
    ...base(),
    device_id: "local-machine",
    device_name: "Remote PC",
    device_mac: "02:00:00:00:00:00",
  };
  const rows = ddnsDeviceEntries([local], [legacy]);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].device.source, "ddns-binding");
  assert.equal(rows[0].device.online, false);
  assert.equal(rows[0].configs[0], legacy);
});
