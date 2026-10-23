import assert from "node:assert/strict";
import test from "node:test";
import { devicePickerOptions } from "../src/utils/devicePicker.ts";
import { defaultDeviceDdnsConfig } from "../src/utils/ddns.ts";
import { isOutsidePopoverInteraction } from "../src/utils/popover.ts";

const mac = (suffix) => ["02", "aa", "bb", "cc", "dd", suffix].join(":");
const device = (suffix, name, extra = {}) => ({
  id: `device-${suffix}`,
  display_name: name,
  hostname: "",
  mac: mac(suffix),
  ipv4: [`192.0.2.${parseInt(suffix, 16)}`],
  ipv6: [],
  global_ipv6: [],
  source: "macos-neighbor",
  online: true,
  last_seen: "",
  ...extra,
});
const binding = (suffix, name) => ({
  ...defaultDeviceDdnsConfig,
  device_id: `device-${suffix}`,
  device_mac: mac(suffix),
  device_name: name,
});

test("picker includes every unbound discovered device plus offline bindings, without duplicating bound devices", () => {
  const devices = [
    device("01", "Local", { source: "local-interface" }),
    device("02", "NAS"),
    device("03", "Router"),
  ];
  const result = devicePickerOptions(devices, [
    binding("01", "Local"),
    binding("04", "Offline PC"),
  ]);
  assert.equal(result.length, 4);
  assert.deepEqual(
    result.filter((option) => !option.bound).map((option) => option.label),
    ["NAS", "Router"],
  );
  assert.equal(result[0].device.source, "local-interface");
  assert.equal(result.at(-1).device.online, false);
});

test("picker searches by name, any IP and normalized MAC, not just the binding name", () => {
  const devices = [
    device("02", "Family NAS", { ipv6: ["2001:db8::22"] }),
    device("03", "Router"),
  ];
  for (const query of [
    "family nas",
    "192.0.2.2",
    "2001:db8::22",
    mac("02").toUpperCase().replaceAll(":", "-"),
    "02aabbccdd02",
  ]) {
    assert.equal(
      devicePickerOptions(devices, [], query)[0].label,
      "Family NAS",
    );
    assert.equal(devicePickerOptions(devices, [], query).length, 1);
  }
  assert.equal(devicePickerOptions(devices, [], "absent").length, 0);
});

test("same-name devices stay separate and missing-MAC devices remain visible but cannot be bound", () => {
  const result = devicePickerOptions(
    [
      device("02", "PC"),
      device("03", "PC"),
      device("04", "Unknown", { mac: "" }),
    ],
    [],
  );
  assert.equal(new Set(result.map((option) => option.id)).size, 3);
  assert.equal(
    result.find((option) => option.label === "Unknown").selectable,
    false,
  );
});

test("picker is not limited to the six rows shown on a device page", () => {
  const devices = Array.from({ length: 40 }, (_, index) =>
    device((index + 1).toString(16).padStart(2, "0"), `Device ${index + 1}`),
  );
  assert.equal(devicePickerOptions(devices, []).length, 40);
  assert.equal(
    devicePickerOptions(devices, [], "Device 40")[0].device.id,
    devices[39].id,
  );
});

test("WebKit document focus during refresh does not dismiss the popup, but an outside click does", () => {
  const document = { body: {}, documentElement: {} };
  const trigger = {
    ownerDocument: document,
    contains: (target) => target === trigger,
  };
  const refreshButton = {};
  const popup = {
    ownerDocument: document,
    contains: (target) => target === refreshButton || target === popup,
  };
  const event = (type, target, path = [target]) => ({
    type,
    target,
    composedPath: () => path,
  });
  assert.equal(
    isOutsidePopoverInteraction(
      event("focusin", document.body),
      trigger,
      popup,
    ),
    false,
  );
  assert.equal(
    isOutsidePopoverInteraction(
      event("focusin", document.documentElement),
      trigger,
      popup,
    ),
    false,
  );
  assert.equal(
    isOutsidePopoverInteraction(
      event("pointerdown", refreshButton),
      trigger,
      popup,
    ),
    false,
  );
  assert.equal(
    isOutsidePopoverInteraction(event("focusin", trigger), trigger, popup),
    false,
  );
  assert.equal(
    isOutsidePopoverInteraction(
      event("pointerdown", document.body),
      trigger,
      popup,
    ),
    true,
  );
  assert.equal(
    isOutsidePopoverInteraction(event("focusin", {}), trigger, popup),
    true,
  );
  assert.equal(
    isOutsidePopoverInteraction(
      event("pointerdown", {}, [refreshButton, popup]),
      trigger,
      popup,
    ),
    false,
  );
});
