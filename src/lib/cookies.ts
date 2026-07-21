import type { Header } from "@/types";

export interface Cookie {
  name: string;
  value: string;
  /** Response cookie attributes (Path, Domain, Expires, HttpOnly, …). */
  attrs: [string, string][];
  /** The original single-cookie string (`name=value; Path=/; …`). */
  raw: string;
}

function headerValue(headers: Header[], name: string): string | undefined {
  return headers.find(([k]) => k.toLowerCase() === name.toLowerCase())?.[1];
}

function splitPair(s: string): [string, string] {
  const eq = s.indexOf("=");
  return eq === -1 ? [s.trim(), ""] : [s.slice(0, eq).trim(), s.slice(eq + 1).trim()];
}

/** Cookies sent by the client — from the request `Cookie` header. */
export function parseRequestCookies(headers: Header[]): Cookie[] {
  const raw = headerValue(headers, "cookie");
  if (!raw) return [];
  return raw
    .split(";")
    .map((s) => s.trim())
    .filter(Boolean)
    .map((pair) => {
      const [name, value] = splitPair(pair);
      return { name, value, attrs: [], raw: `${name}=${value}` };
    });
}

/** Cookies set by the server — one per `Set-Cookie` response header. */
export function parseResponseCookies(headers: Header[]): Cookie[] {
  return headers
    .filter(([k]) => k.toLowerCase() === "set-cookie")
    .map(([, raw]) => {
      const parts = raw.split(";").map((s) => s.trim()).filter(Boolean);
      const [name, value] = splitPair(parts[0] ?? "");
      const attrs = parts.slice(1).map(splitPair);
      return { name, value, attrs, raw };
    });
}
