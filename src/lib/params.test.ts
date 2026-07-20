import { describe, it, expect } from "vitest";
import { parseUrlEncoded, queryParams, isFormEncoded, formParams } from "./params";
import type { HttpMessage } from "@/types";

function msg(headers: [string, string][], body: string): HttpMessage {
  return { headers, body, bodyIsText: true };
}

describe("parseUrlEncoded", () => {
  it("parses name/value pairs", () => {
    expect(parseUrlEncoded("a=1&b=2")).toEqual([
      ["a", "1"],
      ["b", "2"],
    ]);
  });

  it("decodes percent-encoding and plus as space", () => {
    expect(parseUrlEncoded("q=hello+world&city=S%C3%A3o")).toEqual([
      ["q", "hello world"],
      ["city", "São"],
    ]);
  });

  it("keeps duplicate keys and empty values", () => {
    expect(parseUrlEncoded("tag=a&tag=b&flag=")).toEqual([
      ["tag", "a"],
      ["tag", "b"],
      ["flag", ""],
    ]);
  });

  it("handles a key with no equals sign", () => {
    expect(parseUrlEncoded("bare")).toEqual([["bare", ""]]);
  });

  it("returns empty for empty input", () => {
    expect(parseUrlEncoded("")).toEqual([]);
  });
});

describe("queryParams", () => {
  it("extracts params after the first ?", () => {
    expect(queryParams("/search?q=cats&page=2")).toEqual([
      ["q", "cats"],
      ["page", "2"],
    ]);
  });

  it("returns empty when there is no query", () => {
    expect(queryParams("/plain/path")).toEqual([]);
  });
});

describe("form params", () => {
  it("detects urlencoded content-type", () => {
    expect(isFormEncoded(msg([["Content-Type", "application/x-www-form-urlencoded"]], ""))).toBe(
      true,
    );
    expect(isFormEncoded(msg([["Content-Type", "application/json"]], "{}"))).toBe(false);
  });

  it("parses a urlencoded body", () => {
    const m = msg(
      [["content-type", "application/x-www-form-urlencoded; charset=utf-8"]],
      "user=bob&pass=hunter2",
    );
    expect(formParams(m)).toEqual([
      ["user", "bob"],
      ["pass", "hunter2"],
    ]);
  });

  it("returns empty for non-form bodies", () => {
    expect(formParams(msg([["content-type", "application/json"]], '{"a":1}'))).toEqual([]);
  });
});
