export type PortParseResult =
  | { ok: true; ports: number[] }
  | { ok: false; message: string };

export type PortPairResult =
  | { ok: true; pairs: Array<{ listenPort: number; targetPort: number }> }
  | { ok: false; message: string };

const MIN_PORT = 1;
const MAX_PORT = 65535;
const MAX_BATCH_PORTS = 512;

function parsePortNumber(value: string): number | null {
  if (!/^\d+$/.test(value)) {
    return null;
  }
  const port = Number(value);
  if (!Number.isInteger(port) || port < MIN_PORT || port > MAX_PORT) {
    return null;
  }
  return port;
}

export function parsePortExpression(input: string): PortParseResult {
  const normalized = input
    .replace(/[；，,]/g, ";")
    .replace(/[–—]/g, "-")
    .trim();

  if (!normalized) {
    return { ok: false, message: "请填写监听端口" };
  }

  const tokens = normalized
    .split(";")
    .map((token) => token.trim().replace(/\s+/g, ""))
    .filter(Boolean);

  if (tokens.length === 0) {
    return { ok: false, message: "请填写监听端口" };
  }

  const ports: number[] = [];
  const seen = new Set<number>();

  for (const token of tokens) {
    const match = /^(\d+)(?:-(\d+))?$/.exec(token);
    if (!match) {
      return {
        ok: false,
        message: "监听端口格式错误，请使用 80;443;1000-1003 这样的格式",
      };
    }

    const start = parsePortNumber(match[1]);
    const end = match[2] ? parsePortNumber(match[2]) : start;
    if (start === null || end === null) {
      return { ok: false, message: "监听端口范围：1-65535" };
    }
    if (start > end) {
      return { ok: false, message: "监听端口范围起始端口不能大于结束端口" };
    }

    for (let port = start; port <= end; port += 1) {
      if (seen.has(port)) continue;
      if (ports.length >= MAX_BATCH_PORTS) {
        return {
          ok: false,
          message: `一次最多添加 ${MAX_BATCH_PORTS} 个监听端口`,
        };
      }
      seen.add(port);
      ports.push(port);
    }
  }

  return { ok: true, ports };
}

export function formatPortExpression(port: number | null | undefined): string {
  return port ? String(port) : "";
}

export function pairPortExpressions(
  listenInput: string,
  targetInput: string,
): PortPairResult {
  const listenResult = parsePortExpression(listenInput);
  if (!listenResult.ok) {
    return listenResult;
  }

  const targetResult = parsePortExpression(targetInput);
  if (!targetResult.ok) {
    return {
      ok: false,
      message: targetResult.message.replace("监听端口", "目标端口"),
    };
  }

  if (
    targetResult.ports.length !== 1 &&
    targetResult.ports.length !== listenResult.ports.length
  ) {
    return {
      ok: false,
      message: "目标端口数量应为 1 个，或与监听端口数量一致",
    };
  }

  return {
    ok: true,
    pairs: listenResult.ports.map((listenPort, index) => ({
      listenPort,
      targetPort:
        targetResult.ports.length === 1
          ? targetResult.ports[0]
          : targetResult.ports[index],
    })),
  };
}
