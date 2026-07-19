import { useEffect, useState } from "react";
import { Plus, Trash2, X } from "lucide-react";
import { useProjects, type Project } from "../projects";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { cn } from "@/lib/utils";

export function ProjectEditor() {
  const editorOpen = useProjects((s) => s.editorOpen);
  const closeEditor = useProjects((s) => s.closeEditor);
  const projects = useProjects((s) => s.projects);
  const upsert = useProjects((s) => s.upsert);
  const remove = useProjects((s) => s.remove);
  const activeId = useProjects((s) => s.activeId);

  const [selId, setSelId] = useState<string | null>(null);
  useEffect(() => {
    if (editorOpen && !selId) setSelId(projects[0]?.id ?? null);
  }, [editorOpen, projects, selId]);

  if (!editorOpen) return null;
  const selected = projects.find((p) => p.id === selId) ?? null;

  const newProject = () => {
    const p: Project = { id: crypto.randomUUID(), name: "Новый проект", includeHosts: [], excludeHosts: [] };
    void upsert(p).then(() => setSelId(p.id));
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-6" onClick={closeEditor}>
      <div
        className="flex h-[70vh] w-[760px] overflow-hidden rounded-lg border border-border bg-background shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex w-52 shrink-0 flex-col border-r border-border">
          <div className="flex items-center gap-2 border-b border-border bg-card px-3 py-2">
            <span className="text-xs font-semibold text-muted-foreground">Проекты</span>
            <Button size="iconSm" variant="ghost" className="ml-auto" title="Новый проект" onClick={newProject}>
              <Plus />
            </Button>
          </div>
          <div className="min-h-0 flex-1 overflow-auto">
            {projects.map((p) => (
              <button
                key={p.id}
                onClick={() => setSelId(p.id)}
                className={cn(
                  "block w-full truncate px-3 py-2 text-left text-xs",
                  p.id === selId ? "bg-primary/15" : "hover:bg-accent",
                )}
              >
                {p.name}
                {p.id === activeId && <span className="ml-1 text-http-green">●</span>}
              </button>
            ))}
            {projects.length === 0 && (
              <div className="p-3 text-xs text-muted-foreground">Нет проектов — нажмите ＋</div>
            )}
          </div>
        </div>

        <div className="min-w-0 flex-1">
          {selected ? (
            <ProjectForm key={selected.id} project={selected} onSave={upsert} onDelete={() => void remove(selected.id).then(() => setSelId(null))} />
          ) : (
            <div className="flex h-full items-center justify-center p-8 text-center text-sm text-muted-foreground">
              Выберите проект или создайте новый. Проект ограничивает захват его хостами.
            </div>
          )}
        </div>

        <button
          className="absolute right-8 top-8 text-muted-foreground hover:text-foreground"
          onClick={closeEditor}
          title="Закрыть"
        >
          <X className="size-4" />
        </button>
      </div>
    </div>
  );
}

function ProjectForm({
  project,
  onSave,
  onDelete,
}: {
  project: Project;
  onSave: (p: Project) => Promise<void>;
  onDelete: () => void;
}) {
  const [draft, setDraft] = useState<Project>(project);
  const patch = (p: Partial<Project>) => setDraft((d) => ({ ...d, ...p }));

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border bg-card px-4 py-2">
        <Input
          value={draft.name}
          onChange={(e) => patch({ name: e.target.value })}
          className="h-7 w-56"
          placeholder="Имя проекта"
        />
        <div className="ml-auto flex items-center gap-1">
          <Button size="sm" onClick={() => void onSave(draft)}>
            Сохранить
          </Button>
          <Button size="iconSm" variant="ghost" title="Удалить проект" onClick={onDelete}>
            <Trash2 />
          </Button>
        </div>
      </div>
      <div className="min-h-0 flex-1 space-y-4 overflow-auto p-4">
        <HostList
          title="Отслеживать хосты (include)"
          hint="Голый домен ловит и поддомены; можно *.example.com"
          hosts={draft.includeHosts}
          onChange={(includeHosts) => patch({ includeHosts })}
        />
        <HostList
          title="Исключения (exclude)"
          hint="Приоритетнее include"
          hosts={draft.excludeHosts}
          onChange={(excludeHosts) => patch({ excludeHosts })}
        />
      </div>
    </div>
  );
}

function HostList({
  title,
  hint,
  hosts,
  onChange,
}: {
  title: string;
  hint: string;
  hosts: string[];
  onChange: (hosts: string[]) => void;
}) {
  const [input, setInput] = useState("");
  const add = () => {
    const h = input.trim();
    if (h && !hosts.includes(h)) onChange([...hosts, h]);
    setInput("");
  };
  return (
    <div>
      <div className="text-xs font-semibold">{title}</div>
      <div className="mb-1.5 text-[11px] text-muted-foreground">{hint}</div>
      <div className="mb-2 flex flex-wrap gap-1.5">
        {hosts.map((h) => (
          <span key={h} className="flex items-center gap-1 rounded bg-secondary px-2 py-0.5 font-mono text-xs">
            {h}
            <button className="text-muted-foreground hover:text-http-red" onClick={() => onChange(hosts.filter((x) => x !== h))}>
              <X className="size-3" />
            </button>
          </span>
        ))}
        {hosts.length === 0 && <span className="text-xs text-muted-foreground">пусто</span>}
      </div>
      <div className="flex gap-1">
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          placeholder="example.com"
          className="h-7 font-mono"
        />
        <Button size="sm" variant="outline" onClick={add}>
          <Plus />
        </Button>
      </div>
    </div>
  );
}
