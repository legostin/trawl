export type FlowState = "pending" | "completed" | "error" | "paused";

export interface UrlParts {
  scheme: string;
  host: string;
  port: number;
  path: string;
}

export type Header = [name: string, value: string];

export interface HttpMessage {
  headers: Header[];
  /** serde_bytes передаёт тело как массив байт-чисел (или строку) — см. нормализацию в UI. */
  body: number[] | string;
  bodyIsText: boolean;
}

export interface ResponseMessage {
  status: number;
  headers: Header[];
  body: number[] | string;
  bodyIsText: boolean;
}

export interface Timings {
  sent: number | null;
  ttfb: number | null;
  done: number | null;
}

export interface Flow {
  id: number;
  timestamp: number;
  method: string;
  url: UrlParts;
  request: HttpMessage;
  response: ResponseMessage | null;
  timings: Timings;
  state: FlowState;
  error: string | null;
}
