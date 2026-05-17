import assert from "node:assert/strict";

const { pairPortExpressions, parsePortExpression } = await import("../src/utils/ports.ts");

function expectPorts(input, ports) {
  const result = parsePortExpression(input);
  assert.equal(result.ok, true, `${input} should parse: ${result.message ?? ""}`);
  assert.deepEqual(result.ports, ports);
}

function expectError(input, text) {
  const result = parsePortExpression(input);
  assert.equal(result.ok, false, `${input} should fail`);
  assert.match(result.message, text);
}

expectPorts("80", [80]);
expectPorts("80;443;1000-1002", [80, 443, 1000, 1001, 1002]);
expectPorts(" 80；443 ; 443 ; 1000 - 1001 ", [80, 443, 1000, 1001]);
expectError("", /监听端口/);
expectError("0", /1-65535/);
expectError("65536", /1-65535/);
expectError("100-98", /范围起始端口不能大于结束端口/);
expectError("80;abc", /端口格式/);

let pairs = pairPortExpressions("1000-1002", "80");
assert.equal(pairs.ok, true, pairs.message ?? "");
assert.deepEqual(pairs.pairs, [
  { listenPort: 1000, targetPort: 80 },
  { listenPort: 1001, targetPort: 80 },
  { listenPort: 1002, targetPort: 80 },
]);

pairs = pairPortExpressions("1000-1002", "2000-2002");
assert.equal(pairs.ok, true, pairs.message ?? "");
assert.deepEqual(pairs.pairs, [
  { listenPort: 1000, targetPort: 2000 },
  { listenPort: 1001, targetPort: 2001 },
  { listenPort: 1002, targetPort: 2002 },
]);

pairs = pairPortExpressions("1000-1002", "2000;2001");
assert.equal(pairs.ok, false);
assert.match(pairs.message, /目标端口数量/);

console.log("Port expression checks passed.");
