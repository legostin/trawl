import type { Rule } from "../rules";
import { accessor } from "@/lib/analyze";

/** env-ключ из пути поля: последний сегмент, безопасный идентификатор. */
export function keyFromPath(path: string): string {
  const seg = path.split(".").pop() ?? path;
  return seg.replace(/\[\]/g, "").replace(/[^A-Za-z0-9_]/g, "_") || "value";
}

/** Response-правило: извлекает поле ответа в env проекта. */
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
      "const data = JSON.parse(response.body || '{}');\n" +
      `const v = ${accessor(path)};\n` +
      `if (v !== undefined) env['${key}'] = v;\n`,
  };
}

/** Response-правило: подменяет значение поля в ответе. */
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
    script:
      "const data = JSON.parse(response.body || '{}');\n" +
      `${accessor(path)} = ${literal};\n` +
      "response.body = JSON.stringify(data);\n",
  };
}
