import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { STD_FN_DOCS, DOC_CATEGORIES } from "./stdlib-docs";
import { STD_DTS } from "./stdlib";

const stdlibJs = readFileSync(resolve(__dirname, "../../src-tauri/js/stdlib.js"), "utf8");
const implemented = new Set(
  [...stdlibJs.matchAll(/^function ([a-zA-Z]\w*)\(/gm)].map((m) => m[1]).filter((n) => !n.startsWith("__")),
);
// send/sleep are declared by the handler-phase wrapper in scripting.rs, not in stdlib.js.
const externals = new Set(["send", "sleep"]);

describe("stdlib docs sync", () => {
  it("every stdlib.js function is documented in the manifest", () => {
    const documented = new Set(STD_FN_DOCS.map((f) => f.name));
    for (const name of implemented) expect(documented, `no docs for ${name}`).toContain(name);
  });
  it("every manifest entry is implemented and declared in STD_DTS", () => {
    for (const f of STD_FN_DOCS) {
      if (externals.has(f.name)) continue;
      expect(implemented, `${f.name} is in the manifest but not in stdlib.js`).toContain(f.name);
      expect(STD_DTS, `${f.name} is not declared in STD_DTS`).toContain(`function ${f.name}(`);
    }
  });
  it("entry categories come from the fixed list", () => {
    for (const f of STD_FN_DOCS) expect(DOC_CATEGORIES).toContain(f.category);
  });
});
