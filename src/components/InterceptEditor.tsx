import { useMemo, useState } from "react";
import { Ban, Check, Reply } from "lucide-react";
import { useFlows } from "../store";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { TabBar } from "./ui/tabs";
import { bodyToText } from "@/lib/body";
import { queryParams } from "@/lib/params";
import type { Flow, Header } from "@/types";

type Row = { key: string; value: string };
type Tab = "query" | "headers" | "body" | "response";

function toRows(pairs: Header[]): Row[] {
  return pairs.map(([key, value]) => ({ key, value }));
}
function toPairs(rows: Row[]): [string, string][] {
  return rows.filter((r) => r.key.trim() !== "").map((r) => [r.key, r.value]);
}

/** Editable key/value rows, shared by the Headers, Query and Response-headers tabs. */
function KeyValueEditor({
  rows,
  onChange,
  addLabel,
}: {
  rows: Row[];
  onChange: (rows: Row[]) => void;
  addLabel: string;
}) {
  const patch = (i: number, p: Partial<Row>) =>
    onChange(rows.map((r, j) => (j === i ? { ...r, ...p } : r)));
  const remove = (i: number) => onChange(rows.filter((_, j) => j !== i));
  const add = () => onChange([...rows, { key: "", value: "" }]);

  return (
    <>
      <table className="mb-2 w-full border-collapse text-xs">
        <tbody>
          {rows.map((r, i) => (
            <tr key={i} className="border-b border-border/50">
              <td className="w-1/3 py-1 pr-2">
                <Input
                  value={r.key}
                  onChange={(e) => patch(i, { key: e.target.value })}
                  className="h-6 font-mono"
                />
              </td>
              <td className="py-1 pr-2">
                <Input
                  value={r.value}
                  onChange={(e) => patch(i, { value: e.target.value })}
                  className="h-6 font-mono"
                />
              </td>
              <td className="w-8 py-1">
                <Button size="iconSm" variant="ghost" title="Remove" onClick={() => remove(i)}>
                  ×
                </Button>
              </td>
            </tr>
          ))}
          {rows.length === 0 && (
            <tr>
              <td colSpan={3} className="py-2 text-muted-foreground">
                None
              </td>
            </tr>
          )}
        </tbody>
      </table>
      <Button size="sm" variant="ghost" onClick={add}>
        {addLabel}
      </Button>
    </>
  );
}

export function InterceptEditor({ flow }: { flow: Flow }) {
  const phase = flow.pausedPhase ?? "request";
  const resolve = useFlows((s) => s.resolveBreakpoint);
  const isRequest = phase === "request";

  const basePath = useMemo(() => flow.url.path.split("?")[0], [flow.url.path]);

  // Request-side fields (drive Execute in the request phase).
  const [method, setMethod] = useState(flow.method);
  const [reqHeaderRows, setReqHeaderRows] = useState<Row[]>(toRows(flow.request.headers));
  const [queryRows, setQueryRows] = useState<Row[]>(toRows(queryParams(flow.url.path) as Header[]));
  const [reqBody, setReqBody] = useState(bodyToText(flow.request));

  // Response-side fields. In the response phase they edit the real response
  // (Execute); in the request phase they compose the local response (Respond).
  const [status, setStatus] = useState(String(flow.response?.status ?? 200));
  const [respHeaderRows, setRespHeaderRows] = useState<Row[]>(
    isRequest
      ? [{ key: "content-type", value: "application/json" }]
      : toRows(flow.response?.headers ?? []),
  );
  const [respBody, setRespBody] = useState(isRequest ? "" : bodyToText(flow.response));

  const [busy, setBusy] = useState(false);

  const tabs: { value: Tab; label: string }[] = isRequest
    ? [
        { value: "query", label: `Query (${queryRows.length})` },
        { value: "headers", label: `Headers (${reqHeaderRows.length})` },
        { value: "body", label: "Body" },
        { value: "response", label: "Response" },
      ]
    : [
        { value: "headers", label: `Headers (${respHeaderRows.length})` },
        { value: "body", label: "Body" },
      ];
  const [tab, setTab] = useState<Tab>(isRequest ? "query" : "headers");
  const active = tabs.some((t) => t.value === tab) ? tab : tabs[0].value;

  const buildPath = (): string => {
    const qs = queryRows
      .filter((r) => r.key.trim() !== "")
      .map((r) => `${encodeURIComponent(r.key)}=${encodeURIComponent(r.value)}`)
      .join("&");
    return qs ? `${basePath}?${qs}` : basePath;
  };

  const act = async (action: "execute" | "abort" | "respond") => {
    setBusy(true);
    try {
      if (action === "abort") {
        await resolve(flow.id, phase, "abort", { reason: "aborted from UI" });
      } else if (action === "respond") {
        // Return a local response (request phase) — never hits the server.
        await resolve(flow.id, phase, "respond", {
          status: Number(status) || 200,
          headers: toPairs(respHeaderRows),
          body: respBody,
        });
      } else if (isRequest) {
        await resolve(flow.id, phase, "execute", {
          method,
          path: buildPath(),
          headers: toPairs(reqHeaderRows),
          body: reqBody,
        });
      } else {
        await resolve(flow.id, phase, "execute", {
          status: Number(status) || 200,
          headers: toPairs(respHeaderRows),
          body: respBody,
        });
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border bg-http-red/10 px-3 py-2">
        <span className="rounded bg-http-red px-1.5 py-0.5 text-[10px] font-semibold uppercase text-white">
          Paused · {phase}
        </span>
        {isRequest ? (
          <label className="flex items-center gap-1 text-xs text-muted-foreground">
            Method
            <Input
              value={method}
              onChange={(e) => setMethod(e.target.value)}
              className="h-7 w-24 font-mono"
            />
          </label>
        ) : (
          <label className="flex items-center gap-1 text-xs text-muted-foreground">
            Status
            <Input
              value={status}
              onChange={(e) => setStatus(e.target.value)}
              className="h-7 w-20 font-mono"
            />
          </label>
        )}
        <div className="ml-auto flex items-center gap-1">
          <Button size="sm" disabled={busy} onClick={() => void act("execute")}>
            <Check />
            Execute
          </Button>
          {isRequest && (
            <Button
              size="sm"
              variant="outline"
              disabled={busy}
              title="Return the Response tab to the client without contacting the server"
              onClick={() => void act("respond")}
            >
              <Reply />
              Respond locally
            </Button>
          )}
          <Button size="sm" variant="destructive" disabled={busy} onClick={() => void act("abort")}>
            <Ban />
            Abort
          </Button>
        </div>
      </div>

      <TabBar<Tab> value={active} onChange={setTab} tabs={tabs} />

      <div className="min-h-0 flex-1 overflow-auto p-3">
        {active === "query" && (
          <KeyValueEditor rows={queryRows} onChange={setQueryRows} addLabel="+ Add parameter" />
        )}
        {active === "headers" && (
          <KeyValueEditor
            rows={isRequest ? reqHeaderRows : respHeaderRows}
            onChange={isRequest ? setReqHeaderRows : setRespHeaderRows}
            addLabel="+ Add header"
          />
        )}
        {active === "body" && (
          <textarea
            value={isRequest ? reqBody : respBody}
            onChange={(e) => (isRequest ? setReqBody(e.target.value) : setRespBody(e.target.value))}
            spellCheck={false}
            className="h-72 w-full resize-y rounded border border-border bg-card p-2 font-mono text-xs"
          />
        )}
        {active === "response" && (
          <div className="flex flex-col gap-3">
            <div className="text-[11px] text-muted-foreground">
              Used by <span className="font-medium text-foreground">Respond locally</span> — returned to
              the client without contacting the server.
            </div>
            <label className="flex items-center gap-1 text-xs text-muted-foreground">
              Status
              <Input
                value={status}
                onChange={(e) => setStatus(e.target.value)}
                className="h-7 w-24 font-mono"
              />
            </label>
            <div>
              <div className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                Response headers
              </div>
              <KeyValueEditor
                rows={respHeaderRows}
                onChange={setRespHeaderRows}
                addLabel="+ Add header"
              />
            </div>
            <div>
              <div className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                Response body
              </div>
              <textarea
                value={respBody}
                onChange={(e) => setRespBody(e.target.value)}
                spellCheck={false}
                className="h-56 w-full resize-y rounded border border-border bg-card p-2 font-mono text-xs"
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
