// Стандартная библиотека правил. Инжектируется перед каждым скриптом (обе фазы).
// Декларации для автокомплита: src/scripting/stdlib.ts (STD_DTS) — менять синхронно.
// Функции с префиксом __ — внутренние, в справку не попадают.

// ── Заголовки ──
function __lc(o) { var r = {}; for (var k in o) { r[k.toLowerCase()] = k; } return r; }
function header(msg, name) {
  if (!msg || !msg.headers) return undefined;
  var k = __lc(msg.headers)[String(name).toLowerCase()];
  return k ? msg.headers[k] : undefined;
}
function hasHeader(msg, name) { return header(msg, name) !== undefined; }
function setHeader(msg, name, value) {
  if (!msg.headers) msg.headers = {};
  var k = __lc(msg.headers)[String(name).toLowerCase()];
  if (k) delete msg.headers[k];
  msg.headers[name] = String(value);
}
function removeHeader(msg, name) {
  if (!msg || !msg.headers) return;
  var k = __lc(msg.headers)[String(name).toLowerCase()];
  if (k) delete msg.headers[k];
}

// ── Тело ──
function jsonBody(msg) { try { return JSON.parse((msg && msg.body) || 'null'); } catch (e) { return null; } }
function setJsonBody(msg, obj) {
  msg.body = JSON.stringify(obj);
  if (!hasHeader(msg, 'content-type')) setHeader(msg, 'content-type', 'application/json');
}

// ── Запрос ──
function bearer(token) { setHeader(request, 'authorization', 'Bearer ' + token); }
function queryParam(req, name) {
  var q = (req.path || '').split('?')[1] || '';
  var parts = q.split('&');
  for (var i = 0; i < parts.length; i++) {
    var kv = parts[i].split('=');
    if (decodeURIComponent(kv[0]) === name) return decodeURIComponent((kv[1] || '').replace(/\+/g, ' '));
  }
  return undefined;
}

// ── handler-фаза ──
function sendJsonRequest(req) {
  var r = send(req);
  try { r.data = JSON.parse(r.body || 'null'); } catch (e) { r.data = null; }
  return r;
}
function sendWithRetry(req, opts) {
  opts = opts || {};
  var max = opts.retries || 3;
  var delay = opts.delay || 1000;
  var r = send(req);
  var n = 0;
  while (n < max && (r.status === 429 || r.status >= 500)) { sleep(delay); r = send(req); n++; }
  return r;
}

// ── Прочее ──
function secret(name) {
  var v = __native_secret(String(name));
  return (v === undefined || v === null) ? null : v;
}
function notify(text, opts) {
  opts = opts || {};
  ctx.__notifications.push({ text: String(text), channel: opts.channel, title: opts.title });
}
