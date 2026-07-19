export interface FieldInfo {
  path: string;
  type: string;
}

function typeOf(v: unknown): string {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  return typeof v;
}

function walk(value: unknown, prefix: string, out: Map<string, string>) {
  const t = typeOf(value);
  if (t === "object") {
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      walk(v, prefix ? `${prefix}.${k}` : k, out);
    }
  } else if (t === "array") {
    if (prefix) out.set(prefix, "array");
    for (const el of (value as unknown[]).slice(0, 5)) {
      walk(el, prefix ? `${prefix}[]` : "[]", out);
    }
  } else if (prefix) {
    out.set(prefix, t);
  }
}

/** Собирает объединённый список путей полей и их типов по нескольким JSON-значениям. */
export function analyzeJson(values: unknown[]): FieldInfo[] {
  const out = new Map<string, string>();
  for (const v of values) walk(v, "", out);
  return [...out.entries()]
    .map(([path, type]) => ({ path, type }))
    .sort((a, b) => a.path.localeCompare(b.path));
}

/** JS-аксессор для пути: "users[].id" → data['users'][0]['id']. */
export function accessor(path: string): string {
  const parts = path.split(".").map((seg) =>
    seg
      .split("[]")
      .map((s, i) => (i === 0 ? `['${s}']` : "[0]"))
      .join(""),
  );
  return "data" + parts.join("");
}

/** glob-матч по строке (мирроринг серверных правил: `*`, `?`). */
export function matchGlob(pattern: string, target: string): boolean {
  let re = "^";
  for (const ch of pattern) {
    if (ch === "*") re += ".*";
    else if (ch === "?") re += ".";
    else if (".+()|[]{}^$\\".includes(ch)) re += "\\" + ch;
    else re += ch;
  }
  re += "$";
  try {
    return new RegExp(re).test(target);
  } catch {
    return false;
  }
}
