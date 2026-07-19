import type { Flow } from "@/types";
import { bodyToText } from "./body";

function shQuote(s: string): string {
  return `'${s.replace(/'/g, "'\\''")}'`;
}

/** Собирает строку `curl` из перехваченного запроса. */
export function buildCurl(flow: Flow): string {
  const { scheme, host, port, path } = flow.url;
  const showPort = port !== 80 && port !== 443;
  const url = `${scheme}://${host}${showPort ? `:${port}` : ""}${path}`;

  const parts = [`curl -X ${flow.method.toUpperCase()} ${shQuote(url)}`];
  for (const [k, v] of flow.request.headers) {
    parts.push(`-H ${shQuote(`${k}: ${v}`)}`);
  }
  const body = bodyToText(flow.request);
  if (body && flow.request.bodyIsText && !body.startsWith("<binary")) {
    parts.push(`--data-raw ${shQuote(body)}`);
  }
  return parts.join(" \\\n  ");
}
