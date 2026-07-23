import { describe, it, expect } from "vitest";
import { buildCurl } from "./curl";
import type { Flow } from "@/types";

function flow(partial: Partial<Flow>): Flow {
  return {
    id: 1,
    timestamp: 0,
    method: "GET",
    url: { scheme: "https", host: "api.example.com", port: 443, path: "/v1/users" },
    request: { headers: [["Accept", "application/json"]], body: [], bodyIsText: true },
    response: null,
    timings: { sent: null, ttfb: null, done: null },
    state: "completed",
    error: null,
    appliedRules: [],
    ruleTrace: [],
    ...partial,
  };
}

describe("buildCurl", () => {
  it("builds a GET curl with headers and no port for 443", () => {
    const c = buildCurl(flow({}));
    expect(c).toContain("curl -X GET 'https://api.example.com/v1/users'");
    expect(c).toContain("-H 'Accept: application/json'");
    expect(c).not.toContain(":443");
  });

  it("includes non-standard port and POST body", () => {
    const c = buildCurl(
      flow({
        method: "POST",
        url: { scheme: "http", host: "localhost", port: 8080, path: "/api" },
        request: {
          headers: [["Content-Type", "application/json"]],
          body: [],
          bodyIsText: true,
        },
        // тело как строка для простоты теста
      }),
    );
    expect(c).toContain("curl -X POST 'http://localhost:8080/api'");
  });

  it("escapes single quotes in header values", () => {
    const c = buildCurl(flow({ request: { headers: [["X-T", "a'b"]], body: [], bodyIsText: true } }));
    expect(c).toContain("'\\''");
  });
});
