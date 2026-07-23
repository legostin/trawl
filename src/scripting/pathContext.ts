/** stdlib functions whose 2nd argument is a JSONPath literal.
 *  Kept in sync with extract_path_literals in src-tauri/src/scripting.rs. */
export const PATH_FNS = ["patch", "tryPatch", "pick", "pickOne", "removeAt", "mergeAt"] as const;

const OPEN_RE =
  /\b(patch|tryPatch|pick|pickOne|removeAt|mergeAt)\s*\(\s*[^,()'"]+,\s*(['"])((?:\\.|(?!\2).)*)$/;

/** Is the cursor (column, 1-based) inside an unclosed string literal path? */
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

/** All literal paths in the script with coordinates (1-based, for Monaco). */
export function extractPathLiterals(script: string): PathLiteral[] {
  const out: PathLiteral[] = [];
  script.split("\n").forEach((lineText, i) => {
    LITERAL_RE.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = LITERAL_RE.exec(lineText))) {
      const raw = m[1] ?? m[2];
      const start = m.index + m[0].length - raw.length - 1; // 0-based index of the path's first character
      out.push({ path: raw, line: i + 1, startColumn: start + 1, endColumn: start + 1 + raw.length });
    }
  });
  return out;
}
