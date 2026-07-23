import { describe, it, expect } from "vitest";
import { keyFromPath, toJsonPath, saveToEnvRule, overrideRule } from "./genRules";

describe("keyFromPath", () => {
  it("uses the last segment as a safe key", () => {
    expect(keyFromPath("data.token")).toBe("token");
    expect(keyFromPath("user.api-key")).toBe("api_key");
    expect(keyFromPath("items[].id")).toBe("id");
  });
});

describe("toJsonPath", () => {
  it("converts FieldInfo array markers to JSONPath wildcards", () => {
    expect(toJsonPath("auth.token")).toBe("auth.token");
    expect(toJsonPath("items[].id")).toBe("items[*].id");
    expect(toJsonPath("items[].advertData[].price")).toBe("items[*].advertData[*].price");
  });
});

describe("saveToEnvRule", () => {
  it("builds a response rule extracting the field into env via pickOne", () => {
    const r = saveToEnvRule("api.example.com/*", "auth.token", "proj");
    expect(r.phase).toBe("response");
    expect(r.projectId).toBe("proj");
    expect(r.pattern).toBe("api.example.com/*");
    expect(r.script).toContain("pickOne(response, 'auth.token')");
    expect(r.script).toContain("if (v !== null) env['token'] = v");
  });

  it("converts array paths to JSONPath wildcards", () => {
    const r = saveToEnvRule("*/*", "items[].id", null);
    expect(r.script).toContain("pickOne(response, 'items[*].id')");
  });
});

describe("overrideRule", () => {
  it("builds a response rule overriding the field value via patch", () => {
    const r = overrideRule("api.example.com/*", "user.name", "Ada", null);
    expect(r.phase).toBe("response");
    expect(r.script).toContain("patch(response, 'user.name', \"Ada\")");
    expect(r.script).not.toContain("JSON.stringify");
  });

  it("uses a placeholder when there is no example", () => {
    const r = overrideRule("*/*", "x", undefined, null);
    expect(r.script).toContain("'CHANGED'");
  });

  it("converts array paths to JSONPath wildcards", () => {
    const r = overrideRule("*/*", "items[].price", "0", null);
    expect(r.script).toContain("patch(response, 'items[*].price'");
  });
});
