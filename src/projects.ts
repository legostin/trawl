import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useFlows } from "./store";
import { useToast } from "./toast";

export interface EnvVar {
  key: string;
  value: string;
}

export interface Project {
  id: string;
  name: string;
  includeHosts: string[];
  excludeHosts: string[];
  env: EnvVar[];
}

interface ProjectsFile {
  projects: Project[];
  activeId: string | null;
}

interface ProjectsState {
  projects: Project[];
  activeId: string | null;
  editorOpen: boolean;
  load: () => Promise<void>;
  setActive: (id: string | null) => Promise<void>;
  upsert: (project: Project) => Promise<void>;
  remove: (id: string) => Promise<void>;
  addHost: (projectId: string, host: string) => Promise<void>;
  openEditor: () => void;
  closeEditor: () => void;
}

export const useProjects = create<ProjectsState>((set, get) => ({
  projects: [],
  activeId: null,
  editorOpen: false,
  load: async () => {
    const f = await invoke<ProjectsFile>("list_projects");
    set({ projects: f.projects, activeId: f.activeId });
  },
  setActive: async (id) => {
    await invoke("set_active_project", { id });
    set({ activeId: id });
    // смена контекста — очищаем список (новый scope захвата)
    useFlows.getState().clearFlows();
  },
  upsert: async (project) => {
    const f = await invoke<ProjectsFile>("save_project", { project });
    set({ projects: f.projects, activeId: f.activeId });
    useToast.getState().show("Project saved");
  },
  remove: async (id) => {
    const f = await invoke<ProjectsFile>("delete_project", { id });
    set({ projects: f.projects, activeId: f.activeId });
  },
  addHost: async (projectId, host) => {
    const p = get().projects.find((x) => x.id === projectId);
    if (!p || p.includeHosts.includes(host)) return;
    await get().upsert({ ...p, includeHosts: [...p.includeHosts, host] });
  },
  openEditor: () => set({ editorOpen: true }),
  closeEditor: () => set({ editorOpen: false }),
}));
