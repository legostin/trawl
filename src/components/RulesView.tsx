import { useEffect, useMemo, useRef, useState } from "react";
import { BookMarked, FileCode2, Plus, Save, Trash2 } from "lucide-react";
import { useRules, type Phase, type Rule } from "../rules";
import { useProjects } from "../projects";
import { useFlows } from "../store";
import { ScriptEditor, type ScriptEditorApi } from "./ScriptEditor";
import { EmptyState } from "./EmptyState";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import { LabeledSwitch, Switch } from "./ui/switch";
import { STD_FUNCTIONS } from "../scripting/stdlib";
import { useSnippets, type SnippetKind } from "../scripting/snippetStore";
import { setLibraryTypes, setResponseDataType } from "../monaco-setup";
import { SnippetMenu } from "./SnippetMenu";
import { HintsPanel } from "./HintsPanel";
import { useToast } from "../toast";
import { analyzeJson, fieldsToType, matchGlob } from "@/lib/analyze";
import { bodyToText, tryParseJson } from "@/lib/body";
import { cn } from "@/lib/utils";

const NEW_SCRIPT =
  "// handler: you perform the request via send() and return the response.\n" +
  "let response = send(request);\n" +
  "// retry example:\n" +
  "// while (response.status === 429) { sleep(1000); response = send(request); }\n" +
  "return response;\n";

export function RulesView() {
  const { rules, selectedId, library, editingLibrary, load, select, editLibrary, upsert, remove, saveLibrary } =
    useRules();
  const activeId = useProjects((s) => s.activeId);

  const loadSnippets = useSnippets((s) => s.load);
  useEffect(() => {
    void load();
    void loadSnippets();
  }, [load, loadSnippets]);

  useEffect(() => {
    setLibraryTypes(library);
  }, [library]);

  // Показываем только правила активного проекта (или глобальные, когда проект off).
  const scoped = rules.filter((r) => (r.projectId ?? null) === (activeId ?? null));

  /** Toggle from the list without stealing the current selection/editor. */
  const toggleEnabled = async (r: Rule, enabled: boolean) => {
    const st = useRules.getState();
    const wasSelected = st.selectedId;
    const wasLibrary = st.editingLibrary;
    await upsert({ ...r, enabled });
    if (wasLibrary) useRules.getState().editLibrary();
    else useRules.getState().select(wasSelected);
  };

  const newRule = () => {
    void upsert({
      id: crypto.randomUUID(),
      name: "New rule",
      enabled: true,
      pattern: "*/*",
      phase: "handler",
      script: NEW_SCRIPT,
      projectId: activeId ?? null,
    });
  };

  const selected = scoped.find((r) => r.id === selectedId) ?? null;

  return (
    <div className="flex h-full">
      <div className="flex w-64 shrink-0 flex-col border-r border-border">
        <div className="flex items-center gap-2 border-b border-border bg-card px-2 py-1.5">
          <span className="text-xs font-semibold text-muted-foreground">Rules</span>
          <Button size="iconSm" variant="ghost" className="ml-auto" title="New rule" onClick={newRule}>
            <Plus />
          </Button>
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {scoped.map((r) => (
            <div
              key={r.id}
              className={cn(
                "flex w-full items-center gap-2 pr-3",
                r.id === selectedId ? "bg-primary/15" : "hover:bg-accent",
              )}
            >
              <button
                onClick={() => select(r.id)}
                className="flex min-w-0 flex-1 items-center gap-2 px-3 py-2 text-left text-xs"
              >
                <FileCode2 className={cn("size-3.5 shrink-0", r.enabled ? "text-http-green" : "text-muted-foreground")} />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium">{r.name}</span>
                  <span className="block truncate text-muted-foreground">{r.pattern}</span>
                </span>
              </button>
              <Switch
                checked={r.enabled}
                onCheckedChange={(v) => void toggleEnabled(r, v)}
                title={r.enabled ? "Disable rule" : "Enable rule"}
              />
            </div>
          ))}
          {scoped.length === 0 && (
            <div className="p-3 text-xs text-muted-foreground">No rules yet — press ＋</div>
          )}
        </div>
        <button
          onClick={editLibrary}
          className={cn(
            "flex items-center gap-2 border-t border-border px-3 py-2 text-left text-xs",
            editingLibrary ? "bg-primary/15" : "hover:bg-accent",
          )}
        >
          <BookMarked className="size-3.5 text-primary" />
          Function library
        </button>
      </div>

      <div className="min-w-0 flex-1">
        {editingLibrary ? (
          <LibraryEditor key="lib" initial={library} onSave={saveLibrary} />
        ) : selected ? (
          <RuleEditor key={selected.id} rule={selected} onSave={upsert} onDelete={() => void remove(selected.id)} />
        ) : (
          <EmptyState
            icon={<FileCode2 className="size-8" />}
            title="Select a rule"
            hint="Or create one: a rule transforms the request/response for a URL pattern via a JS script."
          />
        )}
      </div>
    </div>
  );
}

function RuleEditor({
  rule,
  onSave,
  onDelete,
}: {
  rule: Rule;
  onSave: (r: Rule) => Promise<void>;
  onDelete: () => void;
}) {
  const [draft, setDraft] = useState<Rule>(rule);
  const patch = (p: Partial<Rule>) => setDraft((d) => ({ ...d, ...p }));
  const editorApi = useRef<ScriptEditorApi | null>(null);
  const flows = useFlows((s) => s.flows);
  const addSnippet = useSnippets((s) => s.add);
  const showToast = useToast((s) => s.show);
  const [saving, setSaving] = useState<{ kind: SnippetKind; code: string } | null>(null);
  const [saveName, setSaveName] = useState("");

  const confirmSave = async () => {
    const name = saveName.trim();
    if (!name || !saving) {
      setSaving(null);
      return;
    }
    await addSnippet(saving.kind, name, saving.code);
    showToast(`Saved ${saving.kind} “${name}”`);
    setSaving(null);
    setSaveName("");
  };

  // Fields observed in past responses matching this pattern — power the hints
  // panel and the `response.data` structure autocomplete.
  const fields = useMemo(() => {
    const matched = flows.filter((f) =>
      [`${f.url.host}${f.url.path}`, `${f.url.host}:${f.url.port}${f.url.path}`].some((t) =>
        matchGlob(draft.pattern, t),
      ),
    );
    const values: unknown[] = [];
    for (const f of matched.slice(-20)) {
      const parsed = tryParseJson(bodyToText(f.response));
      if (parsed !== null) values.push(parsed);
    }
    return analyzeJson(values);
  }, [flows, draft.pattern]);

  useEffect(() => {
    setResponseDataType(fieldsToType(fields));
  }, [fields]);

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
        <Select value={draft.phase} onChange={(e) => patch({ phase: e.target.value as Phase })}>
          <option value="handler">handler (own send)</option>
          <option value="request">request</option>
          <option value="response">response</option>
          <option value="both">both</option>
        </Select>
        <LabeledSwitch
          label="enabled"
          checked={draft.enabled}
          onCheckedChange={(v) => patch({ enabled: v })}
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
      <div className="flex flex-wrap items-center gap-1.5 border-b border-border px-3 py-1.5">
        <SnippetMenu kind="template" label="Templates" onPick={(c) => editorApi.current?.replaceAll(c)} />
        <SnippetMenu kind="snippet" label="Snippets" onPick={(c) => editorApi.current?.insert(c)} />
        <div className="ml-auto flex items-center gap-1">
          <Button
            size="sm"
            variant="ghost"
            className="h-6 text-[11px]"
            title="Save the whole script as a reusable template"
            onClick={() => setSaving({ kind: "template", code: editorApi.current?.getValue() || draft.script })}
          >
            Save as template
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className="h-6 text-[11px]"
            title="Save the selection (or whole script) as a reusable snippet"
            onClick={() =>
              setSaving({
                kind: "snippet",
                code: editorApi.current?.getSelectionText() || editorApi.current?.getValue() || draft.script,
              })
            }
          >
            Save as snippet
          </Button>
        </div>
      </div>
      {saving && (
        <div className="flex items-center gap-2 border-b border-border bg-card px-3 py-1.5">
          <span className="text-[11px] text-muted-foreground">Save {saving.kind} as:</span>
          <Input
            autoFocus
            value={saveName}
            onChange={(e) => setSaveName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void confirmSave();
              if (e.key === "Escape") setSaving(null);
            }}
            placeholder="Name"
            className="h-6 w-56 text-[11px]"
          />
          <Button size="sm" className="h-6 text-[11px]" onClick={() => void confirmSave()}>
            Save
          </Button>
          <Button size="sm" variant="ghost" className="h-6 text-[11px]" onClick={() => setSaving(null)}>
            Cancel
          </Button>
        </div>
      )}
      <div className="flex min-h-0 flex-1">
        <div className="min-w-0 flex-1">
          <ScriptEditor
            value={draft.script}
            onChange={(script) => patch({ script })}
            apiRef={editorApi}
          />
        </div>
        <HintsPanel
          pattern={draft.pattern}
          fields={fields}
          onInsert={(code) => editorApi.current?.insert(code)}
        />
      </div>
    </div>
  );
}

function LibraryEditor({ initial, onSave }: { initial: string; onSave: (s: string) => Promise<void> }) {
  const [src, setSrc] = useState(initial);
  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border bg-card px-3 py-2">
        <span className="text-xs text-muted-foreground">
          Your functions are available in every rule (prelude); they can override the standard library.
        </span>
        <Button size="sm" className="ml-auto" onClick={() => void onSave(src)}>
          <Save />
          Save
        </Button>
      </div>
      <div className="grid min-h-0 flex-1 grid-cols-2">
        <div className="min-h-0 overflow-auto border-r border-border p-3">
          <div className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
            Standard library (built-in)
          </div>
          <ul className="flex flex-col gap-2">
            {STD_FUNCTIONS.map((fn) => (
              <li key={fn.signature} className="rounded border border-border/60 bg-card px-2 py-1.5">
                <div className="flex items-center gap-2">
                  <code className="font-mono text-[11px] text-primary break-all">{fn.signature}</code>
                  {fn.phase === "handler" && (
                    <span className="shrink-0 rounded bg-secondary px-1 text-[9px] uppercase text-muted-foreground">
                      handler
                    </span>
                  )}
                </div>
                <div className="mt-0.5 text-[11px] leading-snug text-muted-foreground">{fn.doc}</div>
              </li>
            ))}
          </ul>
        </div>
        <div className="min-h-0">
          <ScriptEditor value={src} onChange={setSrc} />
        </div>
      </div>
    </div>
  );
}
