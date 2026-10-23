export function operationError(operation: string, error: unknown): string {
  let message = error instanceof Error ? error.message : String(error);
  // Older backends may already include this prefix (sometimes more than once).
  for (;;) {
    const prefix = [operation, "检查或安装更新失败"]
      .flatMap((label) => [`${label}：`, `${label}:`])
      .find((label) => message.startsWith(label));
    if (!prefix) break;
    message = message.slice(prefix.length).trimStart();
  }
  return `${operation}：${message}`;
}
