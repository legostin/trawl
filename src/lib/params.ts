import type { HttpMessage } from "@/types";
import { bodyToText } from "@/lib/body";

export type Param = [name: string, value: string];

function headerValue(headers: [string, string][], name: string): string | undefined {
  return headers.find(([k]) => k.toLowerCase() === name.toLowerCase())?.[1];
}

/** Decode a single `application/x-www-form-urlencoded` component (`+` → space). */
function decodeComponent(s: string): string {
  try {
    return decodeURIComponent(s.replace(/\+/g, " "));
  } catch {
    return s;
  }
}

/** Parse a `a=1&b=2` string into ordered name/value pairs, keeping duplicates. */
export function parseUrlEncoded(raw: string): Param[] {
  if (!raw) return [];
  return raw
    .split("&")
    .filter((pair) => pair.length > 0)
    .map((pair) => {
      const eq = pair.indexOf("=");
      if (eq === -1) return [decodeComponent(pair), ""] as Param;
      return [decodeComponent(pair.slice(0, eq)), decodeComponent(pair.slice(eq + 1))] as Param;
    });
}

/** Query-string (GET) parameters extracted from a request path. */
export function queryParams(path: string): Param[] {
  const q = path.indexOf("?");
  if (q === -1) return [];
  return parseUrlEncoded(path.slice(q + 1));
}

/** True when the request body is `application/x-www-form-urlencoded`. */
export function isFormEncoded(msg: HttpMessage): boolean {
  const ct = headerValue(msg.headers, "content-type") ?? "";
  return ct.toLowerCase().includes("application/x-www-form-urlencoded");
}

/** Form (POST) parameters from a urlencoded request body; empty if not form-encoded. */
export function formParams(msg: HttpMessage): Param[] {
  if (!isFormEncoded(msg)) return [];
  return parseUrlEncoded(bodyToText(msg).trim());
}
