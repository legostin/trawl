export interface FieldInfo {
  path: string;
  type: string;
  /** Example value (truncated) from observed responses. */
  example?: string;
  /** The field's value differs between responses (dynamic). */
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

/** Collects field paths, types, an example value, and a "dynamic" flag. */
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

/** JS accessor for a path: "users[].id" → data['users'][0]['id']. */
export function accessor(path: string): string {
  const parts = path.split(".").map((seg) =>
    seg
      .split("[]")
      .map((s, i) => (i === 0 ? `['${s}']` : "[0]"))
      .join(""),
  );
  return "data" + parts.join("");
}

/** Builds a TS type literal from analyzed fields, for `response.data.…` autocomplete.
 *  e.g. [{path:"user.name",type:"string"}, {path:"items[].id",type:"number"}]
 *       → "{ user: { name: string }; items: Array<{ id: number }> }" */
export function fieldsToType(fields: FieldInfo[]): string {
  interface Node {
    children: Record<string, Node>;
    type?: string;
    array?: boolean;
    elemType?: string;
  }
  const root: Node = { children: {} };
  const ts = (t?: string): string =>
    t === "number" ? "number" : t === "boolean" ? "boolean" : t === "string" ? "string" : "any";

  for (const f of fields) {
    const segs = f.path.split(".");
    let node = root;
    segs.forEach((raw, i) => {
      const isArr = raw.endsWith("[]");
      const key = isArr ? raw.slice(0, -2) : raw;
      if (!key) return;
      const child = (node.children[key] ??= { children: {} });
      const isLeaf = i === segs.length - 1;
      if (isArr) {
        child.array = true;
        if (isLeaf) child.elemType = f.type;
        node = child;
      } else if (isLeaf) {
        child.type = f.type;
      } else {
        node = child;
      }
    });
  }

  const emit = (node: Node): string => {
    const keys = Object.keys(node.children);
    if (keys.length === 0) return "{ [key: string]: any }";
    const body = keys
      .map((k) => {
        const c = node.children[k];
        let t: string;
        if (Object.keys(c.children).length > 0) {
          t = emit(c);
          if (c.array) t = `Array<${t}>`;
        } else if (c.array) {
          t = `${c.elemType ? ts(c.elemType) : "any"}[]`;
        } else {
          t = ts(c.type);
        }
        const safe = /^[A-Za-z_$][\w$]*$/.test(k) ? k : JSON.stringify(k);
        return `${safe}: ${t}`;
      })
      .join("; ");
    return `{ ${body} }`;
  };

  return emit(root);
}

/** glob match against a string (mirrors server rules: `*`, `?`). */
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
