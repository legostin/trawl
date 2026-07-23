import { invoke } from "@tauri-apps/api/core";

/** User host input → bare hostname; empty means github.com. */
export const normalizeHost = (input: string): string => {
  const bare = input.trim().replace(/^https?:\/\//, "").replace(/^www\./, "");
  return bare.split("/")[0] || "github.com";
};

export const listGitHosts = (): Promise<string[]> => invoke("git_hosts_list");
export const setGitHostToken = (host: string, token: string): Promise<void> =>
  invoke("git_host_token_set", { host, token });
export const deleteGitHostToken = (host: string): Promise<void> =>
  invoke("git_host_token_set", { host, token: "" });
