import { invoke } from "@tauri-apps/api/core";

export interface SendRequest {
  method: string;
  url: string;
  headers: [string, string][];
  body: string;
  /** Base64 raw body; overrides `body` when set (multipart/binary). */
  bodyB64?: string | null;
}

export interface SendResponse {
  status: number;
  headers: [string, string][];
  body: string;
  bodyIsText: boolean;
  durationMs: number;
  error: string | null;
}

/** One-shot HTTP send. `viaProxy` routes through the local proxy (also captured). */
export const sendRequest = (request: SendRequest, viaProxy = false): Promise<SendResponse> =>
  invoke("send_request", { request, viaProxy });
