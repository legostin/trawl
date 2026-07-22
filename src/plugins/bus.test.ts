import { describe, it, expect, vi } from "vitest";
import { EventBus } from "./bus";

describe("EventBus", () => {
  it("delivers emitted payloads to subscribers", () => {
    const bus = new EventBus();
    const cb = vi.fn();
    bus.on("x", cb);
    bus.emit("x", 42);
    expect(cb).toHaveBeenCalledWith(42);
  });

  it("unsubscribe stops delivery", () => {
    const bus = new EventBus();
    const cb = vi.fn();
    const off = bus.on("x", cb);
    off();
    bus.emit("x", 1);
    expect(cb).not.toHaveBeenCalled();
  });

  it("supports multiple handlers and isolates types", () => {
    const bus = new EventBus();
    const a = vi.fn();
    const b = vi.fn();
    bus.on("x", a);
    bus.on("y", b);
    bus.emit("x");
    expect(a).toHaveBeenCalledOnce();
    expect(b).not.toHaveBeenCalled();
  });

  it("a throwing handler does not block the others", () => {
    const bus = new EventBus();
    const boom = vi.fn(() => {
      throw new Error("boom");
    });
    const ok = vi.fn();
    bus.on("x", boom);
    bus.on("x", ok);
    expect(() => bus.emit("x")).not.toThrow();
    expect(ok).toHaveBeenCalledOnce();
  });
});

describe("event registry", () => {
  it("known() lists described events with their meta", () => {
    const b = new EventBus();
    b.describe("core:x", { description: "d", payloadType: "{ a: number }", source: "core" });
    expect(b.known()).toEqual([
      {
        type: "core:x",
        description: "d",
        payloadType: "{ a: number }",
        source: "core",
        lastPayload: undefined,
      },
    ]);
  });

  it("emit() records the last payload, and undeclared events appear in known()", () => {
    const b = new EventBus();
    b.emit("p:evt", { n: 1 });
    b.emit("p:evt", { n: 2 });
    expect(b.known()).toEqual([{ type: "p:evt", lastPayload: { n: 2 } }]);
  });

  it("declared meta merges with the observed payload, sorted by type", () => {
    const b = new EventBus();
    b.describe("b:evt", { payloadType: "{ ok: boolean }" });
    b.emit("b:evt", { ok: true });
    b.emit("a:evt", 42);
    expect(b.known().map((e) => e.type)).toEqual(["a:evt", "b:evt"]);
    expect(b.known()[1]).toMatchObject({ payloadType: "{ ok: boolean }", lastPayload: { ok: true } });
  });
});
