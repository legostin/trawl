import { invoke } from "@tauri-apps/api/core";

export const listSecrets = (): Promise<string[]> => invoke("secrets_list");
export const getSecret = (name: string): Promise<string | null> => invoke("secret_get", { name });
export const setSecret = (name: string, value: string): Promise<void> =>
  invoke("secret_set", { name, value });
export const deleteSecret = (name: string): Promise<void> => invoke("secret_delete", { name });
