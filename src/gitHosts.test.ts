import { describe, it, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { normalizeHost } from "./gitHosts";

describe("normalizeHost", () => {
  it("defaults empty input to github.com", () => {
    expect(normalizeHost("")).toBe("github.com");
    expect(normalizeHost("   ")).toBe("github.com");
  });

  it("strips scheme, www and any path", () => {
    expect(normalizeHost("https://github.example.org")).toBe("github.example.org");
    expect(normalizeHost("http://www.github.com/owner/repo")).toBe("github.com");
    expect(normalizeHost("github.com/")).toBe("github.com");
  });

  it("keeps a bare host as-is", () => {
    expect(normalizeHost("github.example.org")).toBe("github.example.org");
  });
});
