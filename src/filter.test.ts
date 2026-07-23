import { describe, it, expect } from "vitest";
import { flowMatches, emptyFilter, type FlowFilter } from "./filter";
import type { Flow } from "./types";

function make(partial: Partial<Flow> & { host?: string; path?: string; status?: number }): Flow {
  const { host = "example.com", path = "/", status, ...rest } = partial;
  return {
    id: 1,
    timestamp: 0,
    method: "GET",
    url: { scheme: "http", host, port: 80, path },
    request: { headers: [], body: [], bodyIsText: true },
    response:
      status === undefined ? null : { status, headers: [], body: [], bodyIsText: true },
    timings: { sent: null, ttfb: null, done: null },
    state: "completed",
    error: null,
    appliedRules: [],
    ruleTrace: [],
    ...rest,
  };
}

describe("flowMatches", () => {
  it("empty filter matches everything", () => {
    expect(flowMatches(make({}), emptyFilter)).toBe(true);
  });

  it("query matches host case-insensitively", () => {
    const f: FlowFilter = { ...emptyFilter, query: "EXAMPLE" };
    expect(flowMatches(make({ host: "example.com" }), f)).toBe(true);
    expect(flowMatches(make({ host: "other.org" }), f)).toBe(false);
  });

  it("query matches path", () => {
    const f: FlowFilter = { ...emptyFilter, query: "/api/users" };
    expect(flowMatches(make({ path: "/api/users?page=1" }), f)).toBe(true);
    expect(flowMatches(make({ path: "/health" }), f)).toBe(false);
  });

  it("method filter is exact", () => {
    const f: FlowFilter = { ...emptyFilter, method: "POST" };
    expect(flowMatches(make({ method: "POST" }), f)).toBe(true);
    expect(flowMatches(make({ method: "GET" }), f)).toBe(false);
  });

  it("status class matches the right bucket", () => {
    const f: FlowFilter = { ...emptyFilter, statusClass: "4xx" };
    expect(flowMatches(make({ status: 404 }), f)).toBe(true);
    expect(flowMatches(make({ status: 200 }), f)).toBe(false);
  });

  it("status class excludes flows without a response", () => {
    const f: FlowFilter = { ...emptyFilter, statusClass: "2xx" };
    expect(flowMatches(make({ status: undefined }), f)).toBe(false);
  });

  it("combined filters are AND", () => {
    const f: FlowFilter = { query: "example", method: "GET", statusClass: "2xx" };
    expect(flowMatches(make({ host: "example.com", method: "GET", status: 200 }), f)).toBe(true);
    expect(flowMatches(make({ host: "example.com", method: "GET", status: 500 }), f)).toBe(false);
  });
});
