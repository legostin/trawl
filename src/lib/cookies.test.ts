import { describe, it, expect } from "vitest";
import { parseRequestCookies, parseResponseCookies } from "./cookies";
import type { Header } from "@/types";

describe("parseRequestCookies", () => {
  it("splits the Cookie header into name/value pairs", () => {
    const h: Header[] = [["Cookie", "sid=abc; theme=dark; empty="]];
    expect(parseRequestCookies(h)).toEqual([
      { name: "sid", value: "abc", attrs: [], raw: "sid=abc" },
      { name: "theme", value: "dark", attrs: [], raw: "theme=dark" },
      { name: "empty", value: "", attrs: [], raw: "empty=" },
    ]);
  });

  it("is case-insensitive and empty when absent", () => {
    expect(parseRequestCookies([["cookie", "a=1"]])[0].name).toBe("a");
    expect(parseRequestCookies([["X", "y"]])).toEqual([]);
  });
});

describe("parseResponseCookies", () => {
  it("parses each Set-Cookie with attributes", () => {
    const h: Header[] = [
      ["Set-Cookie", "sid=abc; Path=/; HttpOnly; Secure; SameSite=Lax"],
      ["set-cookie", "lang=en; Domain=example.com"],
    ];
    const cookies = parseResponseCookies(h);
    expect(cookies).toHaveLength(2);
    expect(cookies[0].name).toBe("sid");
    expect(cookies[0].value).toBe("abc");
    expect(cookies[0].attrs).toEqual([
      ["Path", "/"],
      ["HttpOnly", ""],
      ["Secure", ""],
      ["SameSite", "Lax"],
    ]);
    expect(cookies[1].name).toBe("lang");
    expect(cookies[1].attrs).toEqual([["Domain", "example.com"]]);
  });
});
