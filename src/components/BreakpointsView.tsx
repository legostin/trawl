import { useEffect, useState } from "react";
import { CircleDot, Plus, Save, Trash2 } from "lucide-react";
import { useBreakpoints, type Breakpoint } from "../breakpoints";
import { useProjects } from "../projects";
import { EmptyState } from "./EmptyState";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import { LabeledSwitch, Switch } from "./ui/switch";
import { cn } from "@/lib/utils";

const METHODS = ["*", "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

export function BreakpointsView() {
  const {
    breakpoints,
    selectedId,
    intercept,
    timeoutSecs,
    pauseOthers,
    load,
    select,
    upsert,
    remove,
    setIntercept,
    saveSettings,
  } = useBreakpoints();
  const activeId = useProjects((s) => s.activeId);

  useEffect(() => {
    void load();
  }, [load]);

  const scoped = breakpoints.filter((b) => (b.projectId ?? null) === (activeId ?? null));

  /** Toggle from the list without stealing the current selection. */
  const toggleEnabled = async (b: Breakpoint, enabled: boolean) => {
    const wasSelected = useBreakpoints.getState().selectedId;
    await upsert({ ...b, enabled });
    useBreakpoints.getState().select(wasSelected);
  };

  const newBreakpoint = () => {
    void upsert({
      id: crypto.randomUUID(),
      name: "New breakpoint",
      enabled: true,
      pattern: "*/*",
      method: null,
      onRequest: true,
      onResponse: false,
      projectId: activeId ?? null,
    });
  };

  const selected = scoped.find((b) => b.id === selectedId) ?? null;

  return (
    <div className="flex h-full">
      <div className="flex w-64 shrink-0 flex-col border-r border-border">
        <div className="flex items-center gap-2 border-b border-border bg-card px-2 py-1.5">
          <span className="text-xs font-semibold text-muted-foreground">Breakpoints</span>
          <span className="ml-auto">
            <LabeledSwitch
              label="intercept"
              checked={intercept}
              onCheckedChange={(v) => void setIntercept(v)}
              title="Master switch: pause matching flows"
            />
          </span>
          <Button size="iconSm" variant="ghost" title="New breakpoint" onClick={newBreakpoint}>
            <Plus />
          </Button>
        </div>
        <div className="flex items-center gap-2 border-b border-border px-2 py-1.5 text-[11px] text-muted-foreground">
          <label
            className="flex items-center gap-1"
            title="Auto-continue a paused flow after N seconds (0 = hold forever)"
          >
            timeout
            <input
              type="number"
              min={0}
              value={timeoutSecs}
              onChange={(e) => void saveSettings({ timeoutSecs: Math.max(0, Number(e.target.value) || 0) })}
              className="h-6 w-12 rounded border border-border bg-card px-1 text-foreground"
            />
            s
          </label>
          <label
            className="ml-auto flex items-center gap-1"
            title="While a breakpoint is active, hold new requests too"
          >
            <input
              type="checkbox"
              checked={pauseOthers}
              onChange={(e) => void saveSettings({ pauseOthers: e.target.checked })}
            />
            pause others
          </label>
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {scoped.map((b) => (
            <div
              key={b.id}
              className={cn(
                "flex w-full items-center gap-2 pr-3",
                b.id === selectedId ? "bg-primary/15" : "hover:bg-accent",
              )}
            >
              <button
                onClick={() => select(b.id)}
                className="flex min-w-0 flex-1 items-center gap-2 px-3 py-2 text-left text-xs"
              >
                <CircleDot
                  className={cn("size-3.5 shrink-0", b.enabled ? "text-http-red" : "text-muted-foreground")}
                />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium">{b.name}</span>
                  <span className="block truncate text-muted-foreground">{b.pattern}</span>
                </span>
              </button>
              <Switch
                checked={b.enabled}
                onCheckedChange={(v) => void toggleEnabled(b, v)}
                title={b.enabled ? "Disable breakpoint" : "Enable breakpoint"}
              />
            </div>
          ))}
          {scoped.length === 0 && (
            <div className="p-3 text-xs text-muted-foreground">No breakpoints yet — press ＋</div>
          )}
        </div>
      </div>

      <div className="min-w-0 flex-1">
        {selected ? (
          <BreakpointEditor
            key={selected.id}
            bp={selected}
            onSave={(b) => {
              // `enabled` is owned by the list toggle; never let a stale editor
              // draft overwrite the live value on Save.
              const live = useBreakpoints.getState().breakpoints.find((x) => x.id === b.id);
              return upsert({ ...b, enabled: live ? live.enabled : b.enabled });
            }}
            onDelete={() => void remove(selected.id)}
          />
        ) : (
          <EmptyState
            icon={<CircleDot className="size-8" />}
            title="Select a breakpoint"
            hint="A breakpoint pauses matching traffic so you can edit it live before it continues."
          />
        )}
      </div>
    </div>
  );
}

function BreakpointEditor({
  bp,
  onSave,
  onDelete,
}: {
  bp: Breakpoint;
  onSave: (b: Breakpoint) => Promise<void>;
  onDelete: () => void;
}) {
  const [draft, setDraft] = useState<Breakpoint>(bp);
  const patch = (p: Partial<Breakpoint>) => setDraft((d) => ({ ...d, ...p }));

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-border bg-card px-3 py-2">
        <Input
          value={draft.name}
          onChange={(e) => patch({ name: e.target.value })}
          className="h-7 w-44"
          placeholder="Name"
        />
        <Input
          value={draft.pattern}
          onChange={(e) => patch({ pattern: e.target.value })}
          className="h-7 w-56 font-mono"
          placeholder="host/path glob, e.g. api.example.com/*"
        />
        <Select
          value={draft.method ?? "*"}
          onChange={(e) => patch({ method: e.target.value === "*" ? null : e.target.value })}
        >
          {METHODS.map((m) => (
            <option key={m} value={m}>
              {m === "*" ? "any method" : m}
            </option>
          ))}
        </Select>
        <LabeledSwitch
          label="request"
          checked={draft.onRequest}
          onCheckedChange={(v) => patch({ onRequest: v })}
          title="Pause on the request phase"
        />
        <LabeledSwitch
          label="response"
          checked={draft.onResponse}
          onCheckedChange={(v) => patch({ onResponse: v })}
          title="Pause on the response phase"
        />
        <div className="ml-auto flex items-center gap-1">
          <Button size="sm" onClick={() => void onSave(draft)}>
            <Save />
            Save
          </Button>
          <Button size="iconSm" variant="ghost" title="Delete" onClick={onDelete}>
            <Trash2 />
          </Button>
        </div>
      </div>
      <div className="p-3 text-xs text-muted-foreground">
        Matching{" "}
        {draft.onRequest && draft.onResponse
          ? "requests and responses"
          : draft.onRequest
            ? "requests"
            : draft.onResponse
              ? "responses"
              : "nothing (enable request or response)"}{" "}
        for
        <code className="mx-1 font-mono text-foreground">{draft.pattern}</code>
        {draft.method && draft.method !== "*" ? ` (${draft.method})` : ""} will pause in the Traffic
        view for live editing.
      </div>
    </div>
  );
}
