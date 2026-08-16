import { describe, it, expect, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { parseTrawlLink, allowTrawlLinks } from "./links";

describe("allowTrawlLinks", () => {
  it("lets our own scheme through, which the markdown renderer would strip", () => {
    // react-markdown's safe list is http/https/irc/mailto/xmpp; anything else
    // becomes an empty href, which is why these links did not click.
    expect(allowTrawlLinks("trawl:flow/42")).toBe("trawl:flow/42");
    expect(allowTrawlLinks("trawl://artifact/a.csv")).toBe("trawl://artifact/a.csv");
  });

  it("keeps ordinary links working", () => {
    expect(allowTrawlLinks("https://example.com/x")).toBe("https://example.com/x");
    expect(allowTrawlLinks("mailto:a@b.c")).toBe("mailto:a@b.c");
  });

  it("still strips a scheme that could run code", () => {
    // The answer is written partly from captured traffic, so this must not
    // become "allow everything" just to let one scheme through.
    expect(allowTrawlLinks("javascript:alert(1)")).toBe("");
    expect(allowTrawlLinks("JaVaScRiPt:alert(1)")).toBe("");
    expect(allowTrawlLinks("data:text/html;base64,PHNjcmlwdD4=")).toBe("");
  });
});

describe("parseTrawlLink", () => {
  it("points at a captured flow", () => {
    expect(parseTrawlLink("trawl:flow/42")).toEqual({ kind: "flow", id: 42 });
  });

  it("points at a rule, whose id is not a number", () => {
    expect(parseTrawlLink("trawl:rule/r-7ab3")).toEqual({ kind: "rule", id: "r-7ab3" });
  });

  it("points at an artifact, keeping the extension", () => {
    expect(parseTrawlLink("trawl:artifact/errors.csv")).toEqual({
      kind: "artifact",
      id: "errors.csv",
    });
  });

  it("tolerates the slashed form, which is what a model tends to write", () => {
    expect(parseTrawlLink("trawl://flow/42")).toEqual({ kind: "flow", id: 42 });
  });

  it("leaves ordinary links alone so they still open in a browser", () => {
    expect(parseTrawlLink("https://example.com")).toBeNull();
    expect(parseTrawlLink("mailto:a@b.c")).toBeNull();
    expect(parseTrawlLink(undefined)).toBeNull();
  });

  it("refuses a flow id that is not a number rather than selecting NaN", () => {
    expect(parseTrawlLink("trawl:flow/abc")).toBeNull();
  });

  it("refuses a kind it does not know", () => {
    expect(parseTrawlLink("trawl:secret/1")).toBeNull();
  });

  it("refuses an artifact name that climbs out of its directory", () => {
    // The name reaches the artifacts dir as a path component.
    expect(parseTrawlLink("trawl:artifact/../../etc/passwd")).toBeNull();
  });
});
