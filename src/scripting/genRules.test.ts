import { describe, it, expect } from "vitest";
import { keyFromPath, saveToEnvRule, overrideRule } from "./genRules";

describe("keyFromPath", () => {
  it("uses the last segment as a safe key", () => {
    expect(keyFromPath("data.token")).toBe("token");
    expect(keyFromPath("user.api-key")).toBe("api_key");
    expect(keyFromPath("items[].id")).toBe("id");
  });
});

describe("saveToEnvRule", () => {
  it("builds a response rule extracting the field into env", () => {
    const r = saveToEnvRule("api.example.com/*", "auth.token", "proj");
    expect(r.phase).toBe("response");
    expect(r.projectId).toBe("proj");
    expect(r.pattern).toBe("api.example.com/*");
    expect(r.script).toContain("env['token'] = v");
    expect(r.script).toContain("data['auth']['token']");
  });
});

describe("overrideRule", () => {
  it("builds a response rule overriding the field value", () => {
    const r = overrideRule("api.example.com/*", "user.name", "Ada", null);
    expect(r.phase).toBe("response");
    expect(r.script).toContain("data['user']['name'] = \"Ada\"");
    expect(r.script).toContain("response.body = JSON.stringify(data)");
  });

  it("uses a placeholder when there is no example", () => {
    const r = overrideRule("*/*", "x", undefined, null);
    expect(r.script).toContain("'CHANGED'");
  });
});
