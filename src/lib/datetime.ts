export function toLocalTime(value: string | null | undefined): string {
  if (!value) return "-";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

export type ReplayWindow = "15m" | "1h" | "6h" | "24h" | "all";

export function replayWindowToMinutes(value: ReplayWindow): number | null {
  if (value === "15m") return 15;
  if (value === "1h") return 60;
  if (value === "6h") return 360;
  if (value === "24h") return 1440;
  return null;
}

export function replayWindowToStartIso(value: ReplayWindow): string | null {
  if (value === "all") return null;
  const minutes = replayWindowToMinutes(value);
  if (!minutes) return null;
  return new Date(Date.now() - minutes * 60_000).toISOString();
}

export const replayWindowOptions: { value: string; label: string }[] = [
  { value: "15m", label: "15m" },
  { value: "1h", label: "1h" },
  { value: "6h", label: "6h" },
  { value: "24h", label: "24h" },
  { value: "all", label: "all" }
];