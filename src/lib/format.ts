import type { HttpMessage, ResponseMessage } from "@/types";

/** Длина тела в байтах (тело приходит как массив байт или строка). */
export function bodyLength(msg: HttpMessage | ResponseMessage | null | undefined): number {
  if (!msg) return 0;
  const b = msg.body;
  return typeof b === "string" ? b.length : b.length;
}

export function formatBytes(n: number): string {
  if (n <= 0) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

/** Длительность потока в мс из timings, либо null. */
export function durationMs(sent: number | null, done: number | null): number | null {
  if (sent === null || done === null) return null;
  return Math.max(0, done - sent);
}

export function formatDuration(ms: number | null): string {
  if (ms === null) return "—";
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(2)} s`;
}

/** Время запроса (локальное) с точностью до секунды: HH:MM:SS. */
export function formatClock(unixMs: number): string {
  if (!unixMs) return "—";
  const d = new Date(unixMs);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
