import type { HttpMessage, ResponseMessage } from "@/types";

/** Decodes the body to text (if it's textual), otherwise returns a binary label. */
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

/** Tries to parse a string as JSON; null on failure. */
export function tryParseJson(text: string): unknown | null {
  const t = text.trim();
  if (!t || (t[0] !== "{" && t[0] !== "[")) return null;
  try {
    return JSON.parse(t);
  } catch {
    return null;
  }
}
