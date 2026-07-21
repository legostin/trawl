import { describe, it, expect, vi } from "vitest";
// Monaco (pulled in via BodyEditor) can't load in the node test env — stub it.
vi.mock("@monaco-editor/react", () => ({ default: () => null }));
vi.mock("../monaco-setup", () => ({}));
import { InterceptEditor } from "./InterceptEditor";

describe("InterceptEditor", () => {
  it("is a component", () => {
    expect(typeof InterceptEditor).toBe("function");
  });
});
