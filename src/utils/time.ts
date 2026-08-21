export function formatSyncTime(value: string, fallback = "-"): string {
  const normalized = value.trim();
  if (!normalized) return fallback;

  const match = normalized.match(
    /(?:^|[T\s])(\d{2}:\d{2}:\d{2})(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?$/,
  );
  return match?.[1] ?? normalized;
}
