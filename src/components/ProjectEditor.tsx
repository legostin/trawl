import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { Plus, Trash2, X } from "lucide-react";
import { useProjects, type Project } from "../projects";
import { Button } from "./ui/button";
import { EnvList } from "./EnvList";
import { Input } from "./ui/input";
import { cn } from "@/lib/utils";

/** Hand this one project to a colleague. Variable values stay behind. */
function ExportProject({ id, name }: { id: string; name: string }) {
  const [note, setNote] = useState<string | null>(null);

  const run = async () => {
    const path = await saveDialog({
      defaultPath: `${name.replace(/[^\w.-]+/g, "-").toLowerCase() || "project"}.trawl.json`,
    });
    if (!path) return;
    try {
      await invoke("export_config", { path, projectId: id });
      setNote(`Written to ${path}`);
    } catch (e) {
      setNote(String(e));
    }
  };

  return (
    <section className="mt-4">
      <h3 className="text-xs font-medium">Share this project</h3>
      <p className="mt-0.5 text-xs text-muted-foreground">
        Its hosts, rules and breakpoints in one file. Variable values stay on this machine —
        only their names travel.
      </p>
      <Button className="mt-2" variant="outline" size="sm" onClick={() => void run()}>
        Export project…
      </Button>
      {note && <p className="mt-2 text-xs text-muted-foreground">{note}</p>}
    </section>
  );
}

/** The repository the agent works in, and how much it may do there. */
function CodeFolder({
  dir,
  write,
  onChange,
}: {
  dir: string | null;
  write: boolean;
  onChange: (dir: string | null, write: boolean) => void;
}) {
  const pick = async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Project code" });
    if (typeof picked === "string") onChange(picked, write);
  };

  return (
    <section className="mt-4">
      <h3 className="text-xs font-medium">Agent code folder</h3>
      <p className="mt-0.5 text-xs text-muted-foreground">
        The built-in agent works in this folder, so it can read the code behind the traffic it
        sees. Without one it only sees traffic.
      </p>
      <div className="mt-2 flex items-center gap-2">
        <Button variant="outline" size="sm" onClick={() => void pick()}>
          {dir ? "Change…" : "Choose folder…"}
        </Button>
        {dir && (
          <>
            <span className="truncate font-mono text-xs text-muted-foreground" title={dir}>
              {dir}
            </span>
            <Button variant="outline" size="sm" onClick={() => onChange(null, false)}>
              Clear
            </Button>
          </>
        )}
      </div>
      {dir && (
        <label className="mt-2 flex items-center gap-2 text-xs">
          <input type="checkbox" checked={write} onChange={(e) => onChange(dir, e.target.checked)} />
          Let the agent edit files here
          <span className="text-muted-foreground">
            — off by default; it can always read. Running commands is never granted.
          </span>
        </label>
      )}
    </section>
  );
}

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
  // Persist immediately — used for host add/remove so domains never get lost
  // when the editor is closed without pressing Save.
  const commit = (p: Partial<Project>) => {
    const next = { ...draft, ...p };
    setDraft(next);
    void onSave(next);
  };
  const closeEditor = useProjects((s) => s.closeEditor);

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
          <Button
            size="sm"
            onClick={async () => {
              await onSave(draft);
              closeEditor();
            }}
          >
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
          hint="A bare domain also matches subdomains; wildcards like *.example.com work too. Changes save automatically."
          hosts={draft.includeHosts}
          onChange={(includeHosts) => commit({ includeHosts })}
        />
        <HostList
          title="Excluded hosts"
          hint="Takes priority over include. Changes save automatically."
          hosts={draft.excludeHosts}
          onChange={(excludeHosts) => commit({ excludeHosts })}
        />
        <EnvList
          env={draft.env}
          onChange={(env) => patch({ env })}
          hint="Available in scripts as env.KEY; scripts can also write to them (values persist across requests)."
        />
        <ExportProject id={draft.id} name={draft.name} />
        <CodeFolder
          dir={draft.codeDir ?? null}
          write={draft.codeWrite ?? false}
          onChange={(codeDir, codeWrite) => commit({ codeDir, codeWrite })}
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
        {hosts.length === 0 && <span className="text-xs text-muted-foreground">empty</span>}
      </div>
      <div className="flex gap-1">
        <Input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          onBlur={add}
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
