import builtin from "./builtin-snippets.json";

export interface Snippet {
  label: string;
  code: string;
}

// Built-in templates & snippets are bundled from builtin-snippets.json (parsed
// automatically, not user-editable). User-defined ones live separately on disk
// (snippets.json) — see snippetStore.ts.

/** Full scripts — applying one replaces the whole editor content. */
export const TEMPLATES: Snippet[] = builtin.templates;

/** Fragments — applying one inserts at the cursor. */
export const SNIPPETS: Snippet[] = builtin.snippets;
