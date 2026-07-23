import type { Rule } from "../rules";

/** env key from a field path: last segment, made into a safe identifier. */
export function keyFromPath(path: string): string {
  const seg = path.split(".").pop() ?? path;
  return seg.replace(/\[\]/g, "").replace(/[^A-Za-z0-9_]/g, "_") || "value";
}

/** FieldInfo path (`items[].id`) → JSONPath (`items[*].id`). */
export function toJsonPath(path: string): string {
  return path.replace(/\[\]/g, "[*]");
}

/** Response rule: extracts a response field into the project env (pickOne by JSONPath). */
export function saveToEnvRule(pattern: string, path: string, projectId: string | null): Rule {
  const key = keyFromPath(path);
  return {
    id: crypto.randomUUID(),
    name: `env ${key}`.slice(0, 40),
    enabled: true,
    pattern,
    phase: "response",
    projectId,
    script:
      `const v = pickOne(response, '${toJsonPath(path)}');\n` +
      `if (v !== null) env['${key}'] = v;\n`,
  };
}

/** Response rule: replaces a field's value in the response (patch by JSONPath). */
export function overrideRule(
  pattern: string,
  path: string,
  example: string | undefined,
  projectId: string | null,
): Rule {
  const literal = example === undefined ? "'CHANGED'" : JSON.stringify(example);
  return {
    id: crypto.randomUUID(),
    name: `override ${path}`.slice(0, 40),
    enabled: true,
    pattern,
    phase: "response",
    projectId,
    script: `patch(response, '${toJsonPath(path)}', ${literal});\n`,
  };
}
