/** TypeScript-декларации API скриптов — подаются в Monaco для автокомплита. */
export const API_DTS = `
/** HTTP-запрос, доступный в правиле. Мутируйте поля напрямую. */
interface HttpCatchRequest {
  /** Метод: GET, POST, … */
  method: string;
  /** Полный URL запроса. */
  url: string;
  /** Хост, напр. "api.example.com". */
  host: string;
  /** Путь с query, напр. "/v1/users?page=1". */
  path: string;
  /**
   * Заголовки как объект. Пример:
   *   request.headers['Authorization'] = 'Bearer ' + token;
   */
  headers: Record<string, string>;
  /** Тело как строка (текстовое). Для JSON: JSON.parse(request.body). */
  body: string;
}

/** HTTP-ответ, доступный в фазе response. */
interface HttpCatchResponse {
  /** Код статуса, напр. 200. */
  status: number;
  headers: Record<string, string>;
  /** Тело как строка. */
  body: string;
}

interface HttpCatchMock {
  status?: number;
  headers?: Record<string, string>;
  body?: string;
}

interface HttpCatchCtx {
  request: HttpCatchRequest;
  /** Есть только в фазе response. */
  response?: HttpCatchResponse;
  /**
   * Немедленно вернуть синтетический ответ (мок), не обращаясь к серверу.
   * Пример: ctx.mock({ status: 200, body: JSON.stringify({ ok: true }) });
   */
  mock(response: HttpCatchMock): void;
  /** Оборвать запрос с ошибкой 502. */
  abort(reason?: string): void;
}

/** Контекст текущего потока. */
declare const ctx: HttpCatchCtx;
/** Ярлык для ctx.request. */
declare const request: HttpCatchRequest;
/** Ярлык для ctx.response (в фазе response). */
declare const response: HttpCatchResponse;

// ── handler-режим (фаза "handler") ──

/**
 * Синхронно выполняет реальный HTTP-запрос и возвращает ответ.
 * Доступно только в фазе handler. Без аргумента шлёт текущий request.
 * Пример ретрая:
 *   let r = send(request);
 *   while (r.status === 429) { sleep(1000); r = send(request); }
 *   return r;
 */
declare function send(req?: HttpCatchRequest): HttpCatchResponse;

/** Блокирующая пауза (мс), для ретраев/поллинга в фазе handler. */
declare function sleep(ms: number): void;
`;
