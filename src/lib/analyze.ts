export interface FieldInfo {
  path: string;
  type: string;
  /** Пример значения (усечённый) из наблюдённых ответов. */
  example?: string;
  /** Значение поля различается между ответами (динамическое). */
  varying: boolean;
}

interface Acc {
  type: string;
  values: Set<string>;
  last: string;
}

function typeOf(v: unknown): string {
  if (v === null) return "null";
  if (Array.isArray(v)) return "array";
  return typeof v;
}

function valueString(v: unknown): string {
  return typeof v === "string" ? v : String(v);
}

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n) + "…" : s;
}

function walk(value: unknown, prefix: string, out: Map<string, Acc>) {
  const t = typeOf(value);
  if (t === "object") {
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      walk(v, prefix ? `${prefix}.${k}` : k, out);
    }
  } else if (t === "array") {
    if (prefix && !out.has(prefix)) out.set(prefix, { type: "array", values: new Set(), last: "" });
    for (const el of (value as unknown[]).slice(0, 5)) {
      walk(el, prefix ? `${prefix}[]` : "[]", out);
    }
  } else if (prefix) {
    const s = valueString(value);
    const acc = out.get(prefix) ?? { type: t, values: new Set<string>(), last: s };
    acc.type = t;
    acc.values.add(s);
    acc.last = s;
    out.set(prefix, acc);
  }
}

/** Собирает пути полей, типы, пример значения и признак «динамическое». */
export function analyzeJson(values: unknown[]): FieldInfo[] {
  const out = new Map<string, Acc>();
  for (const v of values) walk(v, "", out);
  return [...out.entries()]
    .map(([path, acc]) => ({
      path,
      type: acc.type,
      example: acc.last ? truncate(acc.last, 40) : undefined,
      varying: acc.values.size > 1,
    }))
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
