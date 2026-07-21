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
type Tab = "query" | "headers" | "body";

function toRows(pairs: Header[]): Row[] {
  return pairs.map(([key, value]) => ({ key, value }));
}
function toPairs(rows: Row[]): [string, string][] {
  return rows.filter((r) => r.key.trim() !== "").map((r) => [r.key, r.value]);
}

/** Editable key/value rows, shared by the Headers and Query tabs. */
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
    <div className="p-3">
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
    </div>
  );
}

export function InterceptEditor({ flow }: { flow: Flow }) {
  const phase = flow.pausedPhase ?? "request";
  const resolve = useFlows((s) => s.resolveBreakpoint);
  const isRequest = phase === "request";

  const basePath = useMemo(() => flow.url.path.split("?")[0], [flow.url.path]);

  const [method, setMethod] = useState(flow.method);
  const [status, setStatus] = useState(String(flow.response?.status ?? 200));
  const [headerRows, setHeaderRows] = useState<Row[]>(
    toRows(isRequest ? flow.request.headers : flow.response?.headers ?? []),
  );
  const [queryRows, setQueryRows] = useState<Row[]>(
    toRows(queryParams(flow.url.path) as Header[]),
  );
  const [body, setBody] = useState(bodyToText(isRequest ? flow.request : flow.response));
  const [busy, setBusy] = useState(false);

  const tabs: { value: Tab; label: string }[] = isRequest
    ? [
        { value: "query", label: `Query (${queryRows.length})` },
        { value: "headers", label: `Headers (${headerRows.length})` },
        { value: "body", label: "Body" },
      ]
    : [
        { value: "headers", label: `Headers (${headerRows.length})` },
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
        await resolve(flow.id, phase, "respond", {
          status: Number(status) || 200,
          headers: toPairs(headerRows),
          body,
        });
      } else {
        await resolve(flow.id, phase, "execute", {
          method: isRequest ? method : undefined,
          path: isRequest ? buildPath() : undefined,
          status: isRequest ? undefined : Number(status) || 200,
          headers: toPairs(headerRows),
          body,
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
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void act("respond")}>
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

      <div className="min-h-0 flex-1 overflow-auto">
        {active === "query" && (
          <KeyValueEditor rows={queryRows} onChange={setQueryRows} addLabel="+ Add parameter" />
        )}
        {active === "headers" && (
          <KeyValueEditor rows={headerRows} onChange={setHeaderRows} addLabel="+ Add header" />
        )}
        {active === "body" && (
          <div className="p-3">
            <textarea
              value={body}
              onChange={(e) => setBody(e.target.value)}
              spellCheck={false}
              className="h-72 w-full resize-y rounded border border-border bg-card p-2 font-mono text-xs"
            />
          </div>
        )}
      </div>
    </div>
  );
}
