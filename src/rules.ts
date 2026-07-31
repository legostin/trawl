import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "./toast";

export type Phase = "request" | "response" | "both" | "handler";

export interface Rule {
  id: string;
  name: string;
  enabled: boolean;
  pattern: string;
  phase: Phase;
  script: string;
  /** Rule's project; null = global. */
  projectId: string | null;
}

interface RulesState {
  rules: Rule[];
  selectedId: string | null;
  library: string;
  editingLibrary: boolean;
  load: () => Promise<void>;
  select: (id: string | null) => void;
  editLibrary: () => void;
  upsert: (rule: Rule) => Promise<void>;
  remove: (id: string) => Promise<void>;
  saveLibrary: (source: string) => Promise<void>;
}

export const useRules = create<RulesState>((set) => ({
  rules: [],
  selectedId: null,
  library: "",
  editingLibrary: false,
  load: async () => {
    try {
      const [rules, library] = await Promise.all([
        invoke<Rule[]>("list_rules"),
        invoke<string>("get_library"),
      ]);
      set({ rules, library });
    } catch (e) {
      // An unreadable rules.json used to look exactly like having no rules,
      // which is the worst way to be told your work is unreachable. The list
      // keeps whatever it had; the message says what actually happened.
      useToast.getState().show(`Правила не прочитаны: ${String(e)}`);
    }
  },
  select: (id) => set({ selectedId: id, editingLibrary: false }),
  editLibrary: () => set({ editingLibrary: true, selectedId: null }),
  upsert: async (rule) => {
    try {
      const rules = await invoke<Rule[]>("save_rule", { rule });
      set({ rules, selectedId: rule.id, editingLibrary: false });
    } catch (e) {
      // Backend rejects conflicts (e.g. two enabled rules on the same params).
      useToast.getState().show(String(e));
    }
  },
  remove: async (id) => {
    const rules = await invoke<Rule[]>("delete_rule", { id });
    set({ rules, selectedId: null });
  },
  saveLibrary: async (source) => {
    await invoke("save_library", { source });
    set({ library: source });
  },
}));
