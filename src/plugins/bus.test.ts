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
