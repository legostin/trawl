/** Функции stdlib, у которых 2-й аргумент — JSONPath-литерал.
 *  Синхронизировано с extract_path_literals в src-tauri/src/scripting.rs. */
export const PATH_FNS = ["patch", "tryPatch", "pick", "pickOne", "removeAt", "mergeAt"] as const;

const OPEN_RE =
  /\b(patch|tryPatch|pick|pickOne|removeAt|mergeAt)\s*\(\s*[^,()'"]+,\s*(['"])((?:\\.|(?!\2).)*)$/;

/** Курсор (column, 1-based) внутри незакрытого строкового литерала-пути? */
export function pathArgContext(line: string, column: number): { fn: string; prefix: string } | null {
  const before = line.slice(0, column - 1);
  const m = before.match(OPEN_RE);
  return m ? { fn: m[1], prefix: m[3] } : null;
}

export interface PathLiteral {
  path: string;
  line: number;
  startColumn: number;
  endColumn: number;
}

const LITERAL_RE =
  /\b(?:patch|tryPatch|pick|pickOne|removeAt|mergeAt)\s*\(\s*[^,()'"]+,\s*(?:'((?:\\.|[^'\\])*)'|"((?:\\.|[^"\\])*)")/g;

/** Все литеральные пути в скрипте с координатами (1-based, для Monaco). */
export function extractPathLiterals(script: string): PathLiteral[] {
  const out: PathLiteral[] = [];
  script.split("\n").forEach((lineText, i) => {
    LITERAL_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = LITERAL_RE.exec(lineText))) {
      const raw = m[1] ?? m[2];
      const start = m.index + m[0].length - raw.length - 1; // 0-based индекс первого символа пути
      out.push({ path: raw, line: i + 1, startColumn: start + 1, endColumn: start + 1 + raw.length });
    }
  });
  return out;
}
