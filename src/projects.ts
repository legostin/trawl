import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useFlows } from "./store";
import { useToast } from "./toast";
import { matchGlob } from "@/lib/analyze";

export interface EnvVar {
  key: string;
  value: string;
}

/** Ключи проекта, перекрывающие одноимённые глобальные переменные. */
export function overriddenKeys(globalEnv: EnvVar[], projectEnv: EnvVar[]): Set<string> {
  const g = new Set(globalEnv.map((e) => e.key).filter(Boolean));
  return new Set(projectEnv.map((e) => e.key).filter((k) => k && g.has(k)));
}

export interface Project {
  id: string;
  name: string;
  includeHosts: string[];
  excludeHosts: string[];
  env: EnvVar[];
}

/** Mirrors backend host_matches: bare domain also matches subdomains; `*` = glob. */
export function hostMatches(entry: string, host: string): boolean {
  const e = entry.trim();
  if (!e) return false;
  if (e.includes("*") || e.includes("?")) return matchGlob(e, host);
  return host === e || host.endsWith(`.${e}`);
}

export function projectTracks(project: Project, host: string): boolean {
  if (project.excludeHosts.some((e) => hostMatches(e, host))) return false;
  return project.includeHosts.some((e) => hostMatches(e, host));
}

interface ProjectsFile {
  projects: Project[];
  activeId: string | null;
  globalEnv?: EnvVar[];
}

interface ProjectsState {
  projects: Project[];
  activeId: string | null;
  globalEnv: EnvVar[];
  editorOpen: boolean;
  variablesOpen: boolean;
  load: () => Promise<void>;
  setActive: (id: string | null) => Promise<void>;
  upsert: (project: Project) => Promise<void>;
  remove: (id: string) => Promise<void>;
  saveGlobalEnv: (env: EnvVar[]) => Promise<void>;
  addHost: (projectId: string, host: string) => Promise<void>;
  openEditor: () => void;
  closeEditor: () => void;
  openVariables: () => void;
  closeVariables: () => void;
}

export const useProjects = create<ProjectsState>((set, get) => ({
  projects: [],
  activeId: null,
  globalEnv: [],
  editorOpen: false,
  variablesOpen: false,
  load: async () => {
    const f = await invoke<ProjectsFile>("list_projects");
    set({ projects: f.projects, activeId: f.activeId, globalEnv: f.globalEnv ?? [] });
  },
  setActive: async (id) => {
    await invoke("set_active_project", { id });
    set({ activeId: id });
    // смена контекста — очищаем список (новый scope захвата)
    useFlows.getState().clearFlows();
  },
  upsert: async (project) => {
    const f = await invoke<ProjectsFile>("save_project", { project });
    set({ projects: f.projects, activeId: f.activeId, globalEnv: f.globalEnv ?? [] });
    useToast.getState().show("Project saved");
  },
  remove: async (id) => {
    const f = await invoke<ProjectsFile>("delete_project", { id });
    set({ projects: f.projects, activeId: f.activeId, globalEnv: f.globalEnv ?? [] });
  },
  saveGlobalEnv: async (env) => {
    const f = await invoke<ProjectsFile>("save_global_env", { env });
    set({ projects: f.projects, activeId: f.activeId, globalEnv: f.globalEnv ?? [] });
  },
  addHost: async (projectId, host) => {
    const p = get().projects.find((x) => x.id === projectId);
    if (!p || p.includeHosts.includes(host)) return;
    await get().upsert({ ...p, includeHosts: [...p.includeHosts, host] });
  },
  openEditor: () => set({ editorOpen: true }),
  closeEditor: () => set({ editorOpen: false }),
  openVariables: () => set({ variablesOpen: true }),
  closeVariables: () => set({ variablesOpen: false }),
}));
