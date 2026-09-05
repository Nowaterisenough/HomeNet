export function formatUptime(value: number): string {
  const seconds = Math.max(0, Math.floor(Number(value) || 0));
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  if (seconds < 60) return `${seconds} 秒`;
  if (minutes < 60) return `${minutes} 分钟`;
  if (hours < 24) return `${hours} 小时 ${minutes % 60} 分钟`;
  return `${Math.floor(hours / 24)} 天 ${hours % 24} 小时`;
}

export function formatEndpoint(address: string, port: number | string): string {
  const host = address.trim();
  return `${host.includes(":") && !host.startsWith("[") ? `[${host}]` : host}:${port}`;
}

export function statusTone(
  value: string,
): "success" | "warning" | "error" | "neutral" {
  if (/失败|错误|异常|冲突|error|fail/i.test(value)) return "error";
  if (/未连接|等待|离线|warning|warn/i.test(value)) return "warning";
  if (/正常|运行中|成功|已同步|无需更新|未变化|无需变更/i.test(value))
    return "success";
  return "neutral";
}
