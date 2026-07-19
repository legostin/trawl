import { useEffect, useState } from "react";
import { BookMarked, FileCode2, Plus, Save, Trash2 } from "lucide-react";
import { useRules, type Phase, type Rule } from "../rules";
import { ScriptEditor } from "./ScriptEditor";
import { EmptyState } from "./EmptyState";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import { SNIPPETS } from "../scripting/snippets";
import { setLibraryTypes } from "../monaco-setup";
import { cn } from "@/lib/utils";

const NEW_SCRIPT = "// request доступен в фазе request, response — в фазе response.\n// request.headers['X-Debug'] = '1';\n";

export function RulesView() {
  const { rules, selectedId, library, editingLibrary, load, select, editLibrary, upsert, remove, saveLibrary } =
    useRules();

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    setLibraryTypes(library);
  }, [library]);

  const newRule = () => {
    void upsert({
      id: crypto.randomUUID(),
      name: "Новое правило",
      enabled: true,
      pattern: "*/*",
      phase: "request",
      script: NEW_SCRIPT,
    });
  };

  const selected = rules.find((r) => r.id === selectedId) ?? null;

  return (
    <div className="flex h-full">
      <div className="flex w-64 shrink-0 flex-col border-r border-border">
        <div className="flex items-center gap-2 border-b border-border bg-card px-2 py-1.5">
          <span className="text-xs font-semibold text-muted-foreground">Правила</span>
          <Button size="iconSm" variant="ghost" className="ml-auto" title="Новое правило" onClick={newRule}>
            <Plus />
          </Button>
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {rules.map((r) => (
            <button
              key={r.id}
              onClick={() => select(r.id)}
              className={cn(
                "flex w-full items-center gap-2 px-3 py-2 text-left text-xs",
                r.id === selectedId ? "bg-primary/15" : "hover:bg-accent",
              )}
            >
              <FileCode2 className={cn("size-3.5 shrink-0", r.enabled ? "text-http-green" : "text-muted-foreground")} />
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">{r.name}</span>
                <span className="block truncate text-muted-foreground">{r.pattern}</span>
              </span>
            </button>
          ))}
          {rules.length === 0 && (
            <div className="p-3 text-xs text-muted-foreground">Правил пока нет — нажмите ＋</div>
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
          Библиотека функций
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
            title="Выберите правило"
            hint="Или создайте новое: правило меняет запрос/ответ по URL-паттерну скриптом на JS."
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

  return (
    <div className="flex h-full flex-col">
      <div className="flex flex-wrap items-center gap-2 border-b border-border bg-card px-3 py-2">
        <Input
          value={draft.name}
          onChange={(e) => patch({ name: e.target.value })}
          className="h-7 w-44"
          placeholder="Имя"
        />
        <Input
          value={draft.pattern}
          onChange={(e) => patch({ pattern: e.target.value })}
          className="h-7 w-56 font-mono"
          placeholder="host/path glob, напр. api.example.com/*"
        />
        <Select value={draft.phase} onChange={(e) => patch({ phase: e.target.value as Phase })}>
          <option value="request">request</option>
          <option value="response">response</option>
          <option value="both">both</option>
        </Select>
        <label className="flex items-center gap-1 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={draft.enabled}
            onChange={(e) => patch({ enabled: e.target.checked })}
          />
          enabled
        </label>
        <div className="ml-auto flex items-center gap-1">
          <Button size="sm" onClick={() => void onSave(draft)}>
            <Save />
            Сохранить
          </Button>
          <Button size="iconSm" variant="ghost" title="Удалить" onClick={onDelete}>
            <Trash2 />
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-1 border-b border-border px-3 py-1.5">
        <span className="text-[11px] text-muted-foreground">Шаблоны:</span>
        {SNIPPETS.map((s) => (
          <Button
            key={s.label}
            size="sm"
            variant="outline"
            className="h-6 text-[11px]"
            onClick={() => patch({ script: draft.script + (draft.script.endsWith("\n") ? "" : "\n") + s.code })}
          >
            {s.label}
          </Button>
        ))}
      </div>
      <div className="min-h-0 flex-1">
        <ScriptEditor value={draft.script} onChange={(script) => patch({ script })} />
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
          Функции здесь доступны во всех правилах (prelude).
        </span>
        <Button size="sm" className="ml-auto" onClick={() => void onSave(src)}>
          <Save />
          Сохранить
        </Button>
      </div>
      <div className="min-h-0 flex-1">
        <ScriptEditor value={src} onChange={setSrc} />
      </div>
    </div>
  );
}
