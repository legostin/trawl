import { invoke } from "@tauri-apps/api/core";

export interface SetupInfo {
  lanIp: string | null;
  port: number;
  certHost: string;
}

export const getSetupInfo = (): Promise<SetupInfo> => invoke<SetupInfo>("get_setup_info");
export const getCaPem = (): Promise<string> => invoke<string>("get_ca_pem");
export const caCertPath = (): Promise<string> => invoke<string>("ca_cert_path");

// Setup actions (macOS)
export const revealCaCert = (): Promise<void> => invoke<void>("reveal_ca_cert");
export const trustCaMacos = (): Promise<void> => invoke<void>("trust_ca_macos");
export const trustCaCommand = (): Promise<string> => invoke<string>("trust_ca_command");
export const setSystemProxy = (enable: boolean): Promise<void> =>
  invoke<void>("set_system_proxy", { enable });
export const systemProxyEnabled = (): Promise<boolean> => invoke<boolean>("system_proxy_enabled");
export const installCaIosSimulator = (): Promise<void> => invoke<void>("install_ca_ios_simulator");
export const iosSimulatorBooted = (): Promise<boolean> => invoke<boolean>("ios_simulator_booted");
export const launchChromeProxy = (): Promise<void> => invoke<void>("launch_chrome_proxy");
