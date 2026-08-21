import { describe, it, expect } from "vitest";
import { fitItems } from "./toolbar";

const w = [100, 80, 120, 60];

describe("fitting a toolbar into the space it has", () => {
  it("shows everything when everything fits", () => {
    expect(fitItems(w, 400, 30, [0, 0, 0, 0])).toEqual({ visible: [0, 1, 2, 3], overflow: [] });
  });

  it("counts the overflow button's own width", () => {
    // 100+80+120+60 = 360 into 320. Dropping the 60 leaves 300, which fits the
    // container but not the container minus the ⋯ — so a second one has to go.
    // Ignoring the menu's width is what makes such a boundary flutter.
    expect(fitItems(w, 320, 30, [0, 0, 0, 0]).overflow).toEqual([2, 3]);
  });

  it("hides the least important first, wherever it sits", () => {
    // The middle item is the expendable one; it goes even though the last is
    // further right.
    const fit = fitItems(w, 280, 30, [2, 2, 0, 2]);
    expect(fit.overflow).toEqual([2]);
    expect(fit.visible).toEqual([0, 1, 3]);
  });

  it("among equals, the rightmost goes first", () => {
    expect(fitItems(w, 340, 30, [1, 1, 1, 1]).overflow).toEqual([3]);
  });

  it("keeps the author's order in what stays visible", () => {
    const fit = fitItems(w, 260, 30, [3, 0, 3, 0]);
    expect(fit.visible).toEqual([0, 2]);
    expect(fit.overflow).toEqual([1, 3]);
  });

  it("puts everything in the menu when nothing fits", () => {
    const fit = fitItems(w, 40, 30, [0, 0, 0, 0]);
    expect(fit.visible).toEqual([]);
    expect(fit.overflow).toEqual([0, 1, 2, 3]);
  });

  it("shows everything before the first measurement", () => {
    // Width is 0 until the observer reports; hiding everything then showing it
    // again is a visible flash on every mount.
    expect(fitItems(w, 0, 30, [0, 0, 0, 0])).toEqual({ visible: [0, 1, 2, 3], overflow: [] });
  });

  it("an empty toolbar is not a menu", () => {
    expect(fitItems([], 100, 30, [])).toEqual({ visible: [], overflow: [] });
  });
});
