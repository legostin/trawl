import { describe, it, expect } from "vitest";
import { buildDomainTree } from "./tree";
import type { Flow } from "./types";

function make(id: number, host: string, path: string, method = "GET", status = 200): Flow {
  return {
    id,
    timestamp: 0,
    method,
    url: { scheme: "https", host, port: 443, path },
    request: { headers: [], body: [], bodyIsText: true },
    response: { status, headers: [], body: [], bodyIsText: true },
    timings: { sent: null, ttfb: null, done: null },
    state: "completed",
    error: null,
    appliedRules: [],
    ruleTrace: [],
  };
}

describe("buildDomainTree", () => {
  it("groups flows by host", () => {
    const tree = buildDomainTree([make(1, "a.com", "/"), make(2, "b.com", "/")]);
    expect(tree.map((h) => h.label)).toEqual(["a.com", "b.com"]);
  });

  it("nests path segments and attaches leaves at the terminal segment", () => {
    const tree = buildDomainTree([
      make(1, "api.com", "/v1/users"),
      make(2, "api.com", "/v1/orders"),
    ]);
    expect(tree).toHaveLength(1);
    const host = tree[0];
    expect(host.label).toBe("api.com");
    const v1 = host.children[0];
    expect(v1.label).toBe("v1");
    expect(v1.children.map((c) => c.label).sort()).toEqual(["orders", "users"]);
    const users = v1.children.find((c) => c.label === "users")!;
    expect(users.leaves.map((l) => l.flowId)).toEqual([1]);
  });

  it("counts leaves in the subtree", () => {
    const tree = buildDomainTree([
      make(1, "api.com", "/v1/users"),
      make(2, "api.com", "/v1/users", "POST", 201),
      make(3, "api.com", "/v1/orders"),
    ]);
    expect(tree[0].count).toBe(3);
    const v1 = tree[0].children[0];
    expect(v1.count).toBe(3);
    const users = v1.children.find((c) => c.label === "users")!;
    expect(users.count).toBe(2);
  });

  it("strips query string from the path", () => {
    const tree = buildDomainTree([make(1, "api.com", "/search?q=1")]);
    const search = tree[0].children[0];
    expect(search.label).toBe("search");
    expect(search.leaves).toHaveLength(1);
  });

  it("attaches root-path requests directly to the host node", () => {
    const tree = buildDomainTree([make(1, "api.com", "/")]);
    expect(tree[0].leaves).toHaveLength(1);
    expect(tree[0].children).toHaveLength(0);
  });
});
