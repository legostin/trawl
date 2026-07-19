import { invoke } from "@tauri-apps/api/core";

export interface SetupInfo {
  lanIp: string | null;
  port: number;
  certHost: string;
}

export const getSetupInfo = (): Promise<SetupInfo> => invoke<SetupInfo>("get_setup_info");
export const getCaPem = (): Promise<string> => invoke<string>("get_ca_pem");
export const caCertPath = (): Promise<string> => invoke<string>("ca_cert_path");
