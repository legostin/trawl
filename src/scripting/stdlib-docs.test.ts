import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { STD_FN_DOCS, DOC_CATEGORIES } from "./stdlib-docs";
import { STD_DTS } from "./stdlib";

const stdlibJs = readFileSync(resolve(__dirname, "../../src-tauri/js/stdlib.js"), "utf8");
const implemented = new Set(
  [...stdlibJs.matchAll(/^function ([a-zA-Z]\w*)\(/gm)].map((m) => m[1]).filter((n) => !n.startsWith("__")),
);
// send/sleep объявляются обёрткой handler-фазы в scripting.rs, не в stdlib.js.
const externals = new Set(["send", "sleep"]);

describe("stdlib docs sync", () => {
  it("каждая функция stdlib.js задокументирована в манифесте", () => {
    const documented = new Set(STD_FN_DOCS.map((f) => f.name));
    for (const name of implemented) expect(documented, `нет доки для ${name}`).toContain(name);
  });
  it("каждая запись манифеста реализована и объявлена в STD_DTS", () => {
    for (const f of STD_FN_DOCS) {
      if (externals.has(f.name)) continue;
      expect(implemented, `${f.name} есть в манифесте, но не в stdlib.js`).toContain(f.name);
      expect(STD_DTS, `${f.name} не объявлен в STD_DTS`).toContain(`function ${f.name}(`);
    }
  });
  it("категории записей — из фиксированного списка", () => {
    for (const f of STD_FN_DOCS) expect(DOC_CATEGORIES).toContain(f.category);
  });
});
