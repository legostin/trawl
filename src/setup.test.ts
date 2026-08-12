import { describe, it, expect, vi } from "vitest";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { nextSetupInfo, proxyLocation, type SetupInfo } from "./setup";

const info = (lanIp: string | null): SetupInfo => ({ lanIp, port: 8729, certHost: "trawl" });

describe("nextSetupInfo", () => {
  it("keeps the previous object when the IP has not changed", () => {
    const prev = info("10.0.0.5");
    expect(nextSetupInfo(prev, info("10.0.0.5"))).toBe(prev);
  });

  it("takes the new object when the network hands out a different IP", () => {
    const next = info("192.168.1.42");
    expect(nextSetupInfo(info("10.0.0.5"), next)).toBe(next);
  });

  it("takes the new object when the network disappears", () => {
    const next = info(null);
    expect(nextSetupInfo(info("10.0.0.5"), next)).toBe(next);
  });

  it("takes the new object on the first poll", () => {
    const next = info("10.0.0.5");
    expect(nextSetupInfo(null, next)).toBe(next);
  });

  it("keeps the previous object while there is still no network", () => {
    const prev = info(null);
    expect(nextSetupInfo(prev, info(null))).toBe(prev);
  });
});

describe("proxyLocation", () => {
  it("pairs the IP with the port", () => {
    expect(proxyLocation(info("10.0.0.5"))).toBe("10.0.0.5:8729");
  });

  it("names the port alone when there is no network", () => {
    expect(proxyLocation(info(null))).toBe("port 8729");
  });

  it("names the default port before the info has loaded", () => {
    expect(proxyLocation(null)).toBe("port 8729");
  });
});
