import { useEffect, useState } from "react";
import { Plus, Trash2, X } from "lucide-react";
import { useProjects, type EnvVar, type Project } from "../projects";
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
    const p: Project = {
      id: crypto.randomUUID(),
      name: "New project",
      includeHosts: [],
      excludeHosts: [],
      env: [],
    };
    void upsert(p).then(() => setSelId(p.id));
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-6" onClick={closeEditor}>
      <div
        className="flex h-[72vh] w-[780px] overflow-hidden rounded-lg border border-border bg-background shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex w-52 shrink-0 flex-col border-r border-border">
          <div className="flex items-center gap-2 border-b border-border bg-card px-3 py-2">
            <span className="text-xs font-semibold text-muted-foreground">Projects</span>
            <Button size="iconSm" variant="ghost" className="ml-auto" title="New project" onClick={newProject}>
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
              <div className="p-3 text-xs text-muted-foreground">No projects yet — press ＋</div>
            )}
          </div>
        </div>

        <div className="min-w-0 flex-1">
          {selected ? (
            <ProjectForm
              key={selected.id}
              project={selected}
              onSave={upsert}
              onDelete={() => void remove(selected.id).then(() => setSelId(null))}
            />
          ) : (
            <div className="flex h-full items-center justify-center p-8 text-center text-sm text-muted-foreground">
              Select a project or create a new one. A project scopes capture to its hosts.
            </div>
          )}
        </div>

        <button
          className="absolute right-8 top-8 text-muted-foreground hover:text-foreground"
          onClick={closeEditor}
          title="Close"
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
          placeholder="Project name"
        />
        <div className="ml-auto flex items-center gap-1">
          <Button size="sm" onClick={() => void onSave(draft)}>
            Save
          </Button>
          <Button size="iconSm" variant="ghost" title="Delete project" onClick={onDelete}>
            <Trash2 />
          </Button>
        </div>
      </div>
      <div className="min-h-0 flex-1 space-y-5 overflow-auto p-4">
        <HostList
          title="Tracked hosts (include)"
          hint="A bare domain also matches subdomains; wildcards like *.example.com work too."
          hosts={draft.includeHosts}
          onChange={(includeHosts) => patch({ includeHosts })}
        />
        <HostList
          title="Excluded hosts"
          hint="Takes priority over include."
          hosts={draft.excludeHosts}
          onChange={(excludeHosts) => patch({ excludeHosts })}
        />
        <EnvList env={draft.env} onChange={(env) => patch({ env })} />
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
        {hosts.length === 0 && <span className="text-xs text-muted-foreground">empty</span>}
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

function EnvList({ env, onChange }: { env: EnvVar[]; onChange: (env: EnvVar[]) => void }) {
  const setAt = (i: number, patch: Partial<EnvVar>) =>
    onChange(env.map((e, idx) => (idx === i ? { ...e, ...patch } : e)));
  return (
    <div>
      <div className="text-xs font-semibold">Environment variables</div>
      <div className="mb-1.5 text-[11px] text-muted-foreground">
        Available in scripts as <code className="rounded bg-secondary px-1 font-mono">env.KEY</code>; scripts can
        also write to them (values persist across requests).
      </div>
      <div className="space-y-1">
        {env.map((e, i) => (
          <div key={i} className="flex items-center gap-1">
            <Input
              value={e.key}
              onChange={(ev) => setAt(i, { key: ev.target.value })}
              placeholder="KEY"
              className="h-7 w-40 font-mono"
            />
            <Input
              value={e.value}
              onChange={(ev) => setAt(i, { value: ev.target.value })}
              placeholder="value"
              className="h-7 flex-1 font-mono"
            />
            <Button
              size="iconSm"
              variant="ghost"
              title="Remove"
              onClick={() => onChange(env.filter((_, idx) => idx !== i))}
            >
              <X className="size-3" />
            </Button>
          </div>
        ))}
      </div>
      <Button
        size="sm"
        variant="outline"
        className="mt-1.5"
        onClick={() => onChange([...env, { key: "", value: "" }])}
      >
        <Plus />
        Add variable
      </Button>
    </div>
  );
}
