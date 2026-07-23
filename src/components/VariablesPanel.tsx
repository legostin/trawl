import { useEffect, useState, type ReactNode } from "react";
import { X } from "lucide-react";
import { overriddenKeys, useProjects, type EnvVar } from "../projects";
import { EnvList } from "./EnvList";
import { cn } from "@/lib/utils";

const GLOBAL = "__global__";

export function VariablesPanel() {
  const open = useProjects((s) => s.variablesOpen);
  const close = useProjects((s) => s.closeVariables);
  const projects = useProjects((s) => s.projects);
  const activeId = useProjects((s) => s.activeId);
  const globalEnv = useProjects((s) => s.globalEnv);
  const saveGlobalEnv = useProjects((s) => s.saveGlobalEnv);
  const upsert = useProjects((s) => s.upsert);

  const [scope, setScope] = useState<string>(GLOBAL);
  const project = projects.find((p) => p.id === scope) ?? null;
  // выбранный проект удалили — откат на Global
  useEffect(() => {
    if (scope !== GLOBAL && !project) setScope(GLOBAL);
  }, [scope, project]);

  if (!open) return null;

  const env = project ? project.env : globalEnv;
  const commit = (next: EnvVar[]) => {
    if (project) void upsert({ ...project, env: next });
    else void saveGlobalEnv(next);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-6" onClick={close}>
      <div
        className="flex h-[72vh] w-[780px] overflow-hidden rounded-lg border border-border bg-background shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex w-52 shrink-0 flex-col border-r border-border">
          <div className="flex items-center gap-2 border-b border-border bg-card px-3 py-2">
            <span className="text-xs font-semibold text-muted-foreground">Variables</span>
          </div>
          <div className="min-h-0 flex-1 overflow-auto">
            <ScopeButton selected={scope === GLOBAL} onClick={() => setScope(GLOBAL)}>
              Global
            </ScopeButton>
            {projects.map((p) => (
              <ScopeButton key={p.id} selected={p.id === scope} onClick={() => setScope(p.id)}>
                {p.name}
                {p.id === activeId && <span className="ml-1 text-http-green">●</span>}
              </ScopeButton>
            ))}
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-auto p-4">
          <ScopeEnv
            key={scope}
            env={env}
            overrideKeys={project ? overriddenKeys(globalEnv, project.env) : undefined}
            hint={
              project
                ? "Project variables. On a key clash they override Global. Changes save on blur."
                : "Available everywhere — with no active project and merged under every project (project keys win). Changes save on blur."
            }
            onCommit={commit}
          />
        </div>

        <button
          className="absolute right-8 top-8 text-muted-foreground hover:text-foreground"
          onClick={close}
          title="Close"
        >
          <X className="size-4" />
        </button>
      </div>
    </div>
  );
}

function ScopeButton({
  selected,
  onClick,
  children,
}: {
  selected: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "block w-full truncate px-3 py-2 text-left text-xs",
        selected ? "bg-primary/15" : "hover:bg-accent",
      )}
    >
      {children}
    </button>
  );
}

/** Черновик env области: типизация буферится, коммит — на blur и при +/− строки. */
function ScopeEnv({
  env,
  hint,
  overrideKeys,
  onCommit,
}: {
  env: EnvVar[];
  hint: string;
  overrideKeys?: Set<string>;
  onCommit: (env: EnvVar[]) => void;
}) {
  const [draft, setDraft] = useState(env);
  useEffect(() => setDraft(env), [env]);
  const change = (next: EnvVar[]) => {
    setDraft(next);
    if (next.length !== draft.length) onCommit(next); // добавление/удаление строки
  };
  const commitIfDirty = () => {
    if (JSON.stringify(draft) !== JSON.stringify(env)) onCommit(draft);
  };
  return (
    <div onBlur={commitIfDirty}>
      <EnvList env={draft} onChange={change} hint={hint} overrideKeys={overrideKeys} />
    </div>
  );
}
