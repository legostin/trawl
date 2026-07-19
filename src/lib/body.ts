import type { HttpMessage, ResponseMessage } from "@/types";

/** Декодирует тело в текст (если оно текстовое), иначе отдаёт метку binary. */
export function bodyToText(msg: HttpMessage | ResponseMessage | null | undefined): string {
  if (!msg) return "";
  const b = msg.body;
  if (typeof b === "string") return b;
  if (b.length === 0) return "";
  if (!msg.bodyIsText) return `<binary ${b.length} bytes>`;
  try {
    return new TextDecoder().decode(new Uint8Array(b));
  } catch {
    return `<binary ${b.length} bytes>`;
  }
}

/** Пытается распарсить строку как JSON; null при неудаче. */
export function tryParseJson(text: string): unknown | null {
  const t = text.trim();
  if (!t || (t[0] !== "{" && t[0] !== "[")) return null;
  try {
    return JSON.parse(t);
  } catch {
    return null;
  }
}
