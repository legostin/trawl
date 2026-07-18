import { useState } from "react";
import { useFlows } from "../store";
import type { HttpMessage, ResponseMessage } from "../types";

function bodyToText(msg: HttpMessage | ResponseMessage | null | undefined): string {
  if (!msg) return "";
  const b = msg.body;
  if (typeof b === "string") return b;
  if (!msg.bodyIsText) return `<binary ${b.length} bytes>`;
  try {
    return new TextDecoder().decode(new Uint8Array(b));
  } catch {
    return `<binary ${b.length} bytes>`;
  }
}

export function FlowDetail() {
  const flow = useFlows((s) => s.flows.find((f) => f.id === s.selectedId) ?? null);
  const [tab, setTab] = useState<"headers" | "body" | "timing">("headers");

  if (!flow) return <div style={{ padding: 16, opacity: 0.6 }}>Выберите запрос</div>;

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%", fontSize: 12 }}>
      <div style={{ padding: 8, borderBottom: "1px solid #333" }}>
        <strong>{flow.method}</strong> {flow.url.scheme}://{flow.url.host}:{flow.url.port}
        {flow.url.path}
        {flow.error && <div style={{ color: "#f88" }}>Ошибка: {flow.error}</div>}
      </div>
      <div style={{ display: "flex", gap: 8, padding: 8 }}>
        {(["headers", "body", "timing"] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            style={{ fontWeight: tab === t ? "bold" : "normal" }}
          >
            {t}
          </button>
        ))}
      </div>
      <div
        style={{
          flex: 1,
          overflow: "auto",
          padding: 8,
          fontFamily: "monospace",
          whiteSpace: "pre-wrap",
        }}
      >
        {tab === "headers" && (
          <>
            <div style={{ opacity: 0.6 }}>— Request —</div>
            {flow.request.headers.map(([k, v], i) => (
              <div key={`rq${i}`}>
                {k}: {v}
              </div>
            ))}
            <div style={{ opacity: 0.6, marginTop: 8 }}>— Response —</div>
            {flow.response?.headers.map(([k, v], i) => (
              <div key={`rs${i}`}>
                {k}: {v}
              </div>
            ))}
          </>
        )}
        {tab === "body" && (
          <>
            <div style={{ opacity: 0.6 }}>— Request body —</div>
            <div>{bodyToText(flow.request)}</div>
            <div style={{ opacity: 0.6, marginTop: 8 }}>— Response body —</div>
            <div>{bodyToText(flow.response)}</div>
          </>
        )}
        {tab === "timing" && (
          <div>
            sent: {flow.timings.sent ?? "-"} · ttfb: {flow.timings.ttfb ?? "-"} · done:{" "}
            {flow.timings.done ?? "-"}
          </div>
        )}
      </div>
    </div>
  );
}
