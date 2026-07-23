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

// ── JSONPath-ядро (RFC 9535, парсер на стороне Rust) ──
// Сообщение отличаем от распарсенного объекта по строковому body + headers.
function __isMsg(x) {
  return !!(x && typeof x === 'object' && typeof x.body === 'string' && typeof x.headers === 'object');
}
// Распарсенный док кэшируется на сообщении (non-enumerable — не попадёт в сериализацию).
function __doc(target) {
  if (!__isMsg(target)) return { doc: target, msg: null };
  if (target.__docCache === undefined) {
    Object.defineProperty(target, '__docCache', {
      value: jsonBody(target), writable: true, configurable: true, enumerable: false,
    });
  }
  return { doc: target.__docCache, msg: target };
}
function __syncDoc(d) {
  if (d.msg) { d.msg.__docCache = d.doc; setJsonBody(d.msg, d.doc); }
}
// Rust возвращает JSON Pointer'ы; разворачиваем в массивы сегментов (~0/~1 по RFC 6901).
function __locate(doc, path) {
  var res = JSON.parse(__native_jsonpath_locate(JSON.stringify(doc === undefined ? null : doc), String(path)));
  if (res.error) throw new Error('JSONPath "' + path + '": ' + res.error);
  return res.locations.map(function (ptr) {
    if (ptr === '') return [];
    return ptr.split('/').slice(1).map(function (s) { return s.replace(/~1/g, '/').replace(/~0/g, '~'); });
  });
}
function __getAt(doc, loc) {
  var v = doc;
  for (var i = 0; i < loc.length; i++) { if (v == null) return undefined; v = v[loc[i]]; }
  return v;
}
function __parentAt(doc, loc) {
  var v = doc;
  for (var i = 0; i < loc.length - 1; i++) { v = v[loc[i]]; }
  return v;
}
// Срез структуры верхнего уровня для диагностики: { status, items[20], … }
function __shape(v) {
  if (v === null || v === undefined) return String(v);
  if (Array.isArray(v)) return '[' + v.length + ' элементов]';
  if (typeof v !== 'object') return typeof v;
  var ks = Object.keys(v), parts = [];
  for (var i = 0; i < Math.min(ks.length, 8); i++) {
    var k = ks[i], x = v[k];
    parts.push(Array.isArray(x) ? k + '[' + x.length + ']' : k);
  }
  if (ks.length > 8) parts.push('…');
  return '{ ' + parts.join(', ') + ' }';
}
// Трасса операций (ctx.__trace подключается в Task 9; до того — тихий no-op).
function __traceOp(op, path, nodes) {
  try { if (typeof ctx !== 'undefined' && ctx.__trace) ctx.__trace.push({ op: op, path: String(path), nodes: nodes }); } catch (e) {}
}

function __applyPatch(name, target, path, valueOrFn, minMatches) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  if (locs.length < minMatches) {
    throw new Error(name + '("' + path + '"): 0 узлов. Тело: ' + __shape(d.doc));
  }
  for (var i = 0; i < locs.length; i++) {
    var loc = locs[i];
    if (loc.length === 0) {
      var nv = (typeof valueOrFn === 'function') ? valueOrFn(d.doc) : valueOrFn;
      if (d.msg) {
        d.doc = nv;                       // message: __syncDoc writes body below
      } else if (d.doc && typeof d.doc === 'object' && nv && typeof nv === 'object'
                 && Array.isArray(d.doc) === Array.isArray(nv)) {
        // plain object/array target: replace contents in place so the caller's ref updates
        if (Array.isArray(d.doc)) {
          d.doc.length = 0;
          for (var qi = 0; qi < nv.length; qi++) d.doc.push(nv[qi]);
        } else {
          for (var ok in d.doc) if (Object.prototype.hasOwnProperty.call(d.doc, ok)) delete d.doc[ok];
          for (var nk in nv) if (Object.prototype.hasOwnProperty.call(nv, nk)) d.doc[nk] = nv[nk];
        }
      } else {
        throw new Error('patch("$"): корневую замену на не-объект для распарсенного объекта сделать нельзя — присвойте результат сами');
      }
      continue;
    }
    var parent = __parentAt(d.doc, loc);
    var key = loc[loc.length - 1];
    parent[key] = (typeof valueOrFn === 'function') ? valueOrFn(parent[key]) : valueOrFn;
  }
  __syncDoc(d);
  __traceOp(name, path, locs.length);
  return locs.length;
}
// Записать значение/применить функцию во всех совпавших узлах. 0 узлов → ошибка.
function patch(target, path, valueOrFn) { return __applyPatch('patch', target, path, valueOrFn, 1); }
// То же, но отсутствие совпадений — не ошибка.
function tryPatch(target, path, valueOrFn) { return __applyPatch('tryPatch', target, path, valueOrFn, 0); }

// Все совпавшие значения массивом.
function pick(target, path) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  var out = [];
  for (var i = 0; i < locs.length; i++) out.push(__getAt(d.doc, locs[i]));
  __traceOp('pick', path, locs.length);
  return out;
}
// Первое совпавшее значение или null.
function pickOne(target, path) {
  var r = pick(target, path);
  return r.length ? r[0] : null;
}
// Удалить совпавшие узлы. Обратный порядок — чтобы splice не сдвигал ещё не обработанные индексы.
function removeAt(target, path) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  for (var i = locs.length - 1; i >= 0; i--) {
    var loc = locs[i];
    if (!loc.length) continue; // корень не удаляем
    var parent = __parentAt(d.doc, loc);
    var key = loc[loc.length - 1];
    if (Array.isArray(parent)) parent.splice(Number(key), 1);
    else delete parent[key];
  }
  __syncDoc(d);
  __traceOp('removeAt', path, locs.length);
  return locs.length;
}
function __deepMerge(dst, src) {
  for (var k in src) {
    var s = src[k];
    if (s && typeof s === 'object' && !Array.isArray(s) &&
        dst[k] && typeof dst[k] === 'object' && !Array.isArray(dst[k])) {
      __deepMerge(dst[k], s);
    } else {
      dst[k] = s;
    }
  }
  return dst;
}
// Deep-merge объекта в каждый совпавший узел. 0 узлов → ошибка.
function mergeAt(target, path, obj) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  if (!locs.length) throw new Error('mergeAt("' + path + '"): 0 узлов. Тело: ' + __shape(d.doc));
  for (var i = 0; i < locs.length; i++) {
    var n = __getAt(d.doc, locs[i]);
    if (n && typeof n === 'object') __deepMerge(n, obj);
  }
  __syncDoc(d);
  __traceOp('mergeAt', path, locs.length);
  return locs.length;
}

// ── URL/query ──
// request.url и request.path должны меняться согласованно.
function __syncUrl(req) {
  if (req.url) {
    var m = String(req.url).match(/^(https?:\/\/[^\/]+)/);
    if (m) req.url = m[1] + req.path;
  }
}
function __splitPath(req) {
  var p = req.path || '';
  var i = p.indexOf('?');
  return { base: i < 0 ? p : p.slice(0, i), query: i < 0 ? '' : p.slice(i + 1) };
}
function __joinPath(req, base, parts) {
  req.path = parts.length ? base + '?' + parts.join('&') : base;
  __syncUrl(req);
}
function setQueryParam(req, name, value) {
  var sp = __splitPath(req);
  var parts = sp.query ? sp.query.split('&') : [];
  var enc = encodeURIComponent, found = false;
  for (var i = 0; i < parts.length; i++) {
    if (decodeURIComponent(parts[i].split('=')[0]) === name) { parts[i] = enc(name) + '=' + enc(String(value)); found = true; }
  }
  if (!found) parts.push(enc(name) + '=' + enc(String(value)));
  __joinPath(req, sp.base, parts);
}
function removeQueryParam(req, name) {
  var sp = __splitPath(req);
  var parts = (sp.query ? sp.query.split('&') : []).filter(function (p) {
    return decodeURIComponent(p.split('=')[0]) !== name;
  });
  __joinPath(req, sp.base, parts);
}
// Меняет host и авторити в url. Заголовок Host не трогаем — им управляет прокси.
function rewriteHost(req, host) {
  req.host = String(host);
  if (req.url) req.url = String(req.url).replace(/^(https?:\/\/)[^\/]+/, '$1' + host);
}
// from: строка (заменяются все вхождения) или RegExp. Query-часть не затрагивается.
function rewritePath(req, from, to) {
  var sp = __splitPath(req);
  var np = (from instanceof RegExp) ? sp.base.replace(from, to) : sp.base.split(from).join(to);
  req.path = sp.query ? np + '?' + sp.query : np;
  __syncUrl(req);
}
function pathSegments(req) {
  return ((req.path || '').split('?')[0]).split('/')
    .filter(function (s) { return s.length > 0; })
    .map(decodeURIComponent);
}

// ── Моки и ответы ──
function __mockOrReturn(resp) {
  // request/response-фаза: ctx.mock существует → мок; handler: просто вернуть объект.
  if (typeof ctx !== 'undefined' && typeof ctx.mock === 'function') ctx.mock(resp);
  return resp;
}
// json({obj}) | json(status, {obj}) — JSON-ответ одной строкой.
function json(a, b) {
  var status = 200, obj = a;
  if (typeof a === 'number') { status = a; obj = b; }
  return __mockOrReturn({
    status: status,
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(obj === undefined ? null : obj),
  });
}
function textResponse(status, body, contentType) {
  return __mockOrReturn({
    status: status,
    headers: { 'content-type': contentType || 'text/plain; charset=utf-8' },
    body: String(body),
  });
}
function httpError(status, msg) {
  return json(status, { error: msg === undefined ? ('HTTP ' + status) : String(msg) });
}
// Блокирующая пауза. Только handler-фаза: request/response исполняются на общем потоке движка.
function delay(ms) {
  if (typeof __native_sleep !== 'function') {
    throw new Error('delay() доступен только в handler-фазе (phase: handler)');
  }
  __native_sleep(Math.max(0, Number(ms) || 0));
}
