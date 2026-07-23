import { describe, expect, it } from "vitest";
import { segmentCandidates } from "./pathHints";
import type { FieldInfo } from "@/lib/analyze";

const f = (path: string, type = "string"): FieldInfo => ({ path, type, varying: false });
const FIELDS = [
  f("status"),
  f("items", "array"),
  f("items[].type"),
  f("items[].advertData.id", "number"),
  f("items[].advertData.title"),
];

describe("segmentCandidates", () => {
  it("empty prefix → top-level keys", () => {
    const labels = segmentCandidates("", FIELDS).map((c) => c.label).sort();
    expect(labels).toEqual(["items", "status"]);
    expect(segmentCandidates("", FIELDS).find((c) => c.label === "items")?.kind).toBe("array");
  });
  it("items[*]. → element fields", () => {
    const labels = segmentCandidates("items[*].", FIELDS).map((c) => c.label).sort();
    expect(labels).toEqual(["advertData", "type"]);
  });
  it("filter selector is equivalent to [*]", () => {
    const labels = segmentCandidates("items[?@.type=='a'].", FIELDS).map((c) => c.label).sort();
    expect(labels).toEqual(["advertData", "type"]);
  });
  it("deep prefix", () => {
    const labels = segmentCandidates("items[*].advertData.", FIELDS).map((c) => c.label).sort();
    expect(labels).toEqual(["id", "title"]);
  });
  it("leading $ is ignored", () => {
    expect(segmentCandidates("$.items[*].", FIELDS).map((c) => c.label).sort()).toEqual(["advertData", "type"]);
  });
});
