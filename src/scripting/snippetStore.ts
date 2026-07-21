import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { TEMPLATES, SNIPPETS } from "./snippets";

export type SnippetKind = "template" | "snippet";

export interface SnippetItem {
  id: string;
  label: string;
  code: string;
  kind: SnippetKind;
  builtin: boolean;
}

const BUILTIN: SnippetItem[] = [
  ...TEMPLATES.map((t) => ({ id: `builtin:t:${t.label}`, label: t.label, code: t.code, kind: "template" as const, builtin: true })),
  ...SNIPPETS.map((s) => ({ id: `builtin:s:${s.label}`, label: s.label, code: s.code, kind: "snippet" as const, builtin: true })),
];

interface UserItem {
  id: string;
  label: string;
  code: string;
  kind: SnippetKind;
}

interface SnippetsFile {
  items: UserItem[];
  usage: Record<string, number>;
}

interface SnippetsState {
  user: SnippetItem[];
  usage: Record<string, number>;
  load: () => Promise<void>;
  recordUse: (id: string) => void;
  add: (kind: SnippetKind, label: string, code: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

export const useSnippets = create<SnippetsState>((set, get) => {
  const persist = () => {
    const file: SnippetsFile = {
      items: get().user.map(({ id, label, code, kind }) => ({ id, label, code, kind })),
      usage: get().usage,
    };
    void invoke("save_snippets", { file });
  };

  return {
    user: [],
    usage: {},
    load: async () => {
      try {
        const f = await invoke<SnippetsFile>("get_snippets");
        set({
          user: f.items.map((i) => ({ ...i, builtin: false })),
          usage: f.usage ?? {},
        });
      } catch {
        /* not in Tauri */
      }
    },
    recordUse: (id) => {
      set((s) => ({ usage: { ...s.usage, [id]: (s.usage[id] ?? 0) + 1 } }));
      persist();
    },
    add: async (kind, label, code) => {
      const item: SnippetItem = { id: `user:${kind}:${crypto.randomUUID()}`, label, code, kind, builtin: false };
      set((s) => ({ user: [...s.user, item] }));
      persist();
    },
    remove: async (id) => {
      set((s) => {
        const usage = { ...s.usage };
        delete usage[id];
        return { user: s.user.filter((i) => i.id !== id), usage };
      });
      persist();
    },
  };
});

export { BUILTIN as BUILTIN_SNIPPETS };
