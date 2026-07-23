// Standard library for rules. Injected before every script (both phases).
// Autocomplete declarations: src/scripting/stdlib.ts (STD_DTS) — keep in sync.
// Functions prefixed with __ are internal and don't show up in help.

// ── Headers ──
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

// ── Body ──
function jsonBody(msg) { try { return JSON.parse((msg && msg.body) || 'null'); } catch (e) { return null; } }
function setJsonBody(msg, obj) {
  msg.body = JSON.stringify(obj);
  if (msg.__docCache !== undefined) { try { delete msg.__docCache; } catch (e) { msg.__docCache = undefined; } }
  if (!hasHeader(msg, 'content-type')) setHeader(msg, 'content-type', 'application/json');
}

// ── Request ──
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

// ── handler phase ──
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

// ── Misc ──
function secret(name) {
  var v = __native_secret(String(name));
  return (v === undefined || v === null) ? null : v;
}
function notify(text, opts) {
  opts = opts || {};
  ctx.__notifications.push({ text: String(text), channel: opts.channel, title: opts.title });
}

// ── JSONPath core (RFC 9535, parser lives on the Rust side) ──
// Distinguish a message from a parsed object by its string body + headers.
function __isMsg(x) {
  return !!(x && typeof x === 'object' && typeof x.body === 'string' && typeof x.headers === 'object');
}
// The parsed doc is cached on the message (non-enumerable — won't end up in serialization).
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
// Rust returns JSON Pointers; expand them into arrays of segments (~0/~1 per RFC 6901).
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
// Top-level structure shape for diagnostics: { status, items[20], … }
function __shape(v) {
  if (v === null || v === undefined) return String(v);
  if (Array.isArray(v)) return '[' + v.length + ' elements]';
  if (typeof v !== 'object') return typeof v;
  var ks = Object.keys(v), parts = [];
  for (var i = 0; i < Math.min(ks.length, 8); i++) {
    var k = ks[i], x = v[k];
    parts.push(Array.isArray(x) ? k + '[' + x.length + ']' : k);
  }
  if (ks.length > 8) parts.push('…');
  return '{ ' + parts.join(', ') + ' }';
}
// Operation trace (ctx.__trace is wired up in Task 9; until then this is a silent no-op).
function __traceOp(op, path, nodes) {
  try { if (typeof ctx !== 'undefined' && ctx.__trace) ctx.__trace.push({ op: op, path: String(path), nodes: nodes }); } catch (e) {}
}

function __applyPatch(name, target, path, valueOrFn, minMatches) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  if (locs.length < minMatches) {
    throw new Error(name + '("' + path + '"): 0 nodes. Body: ' + __shape(d.doc));
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
        throw new Error('patch("$"): root replacement of a parsed object with a non-object is not supported — assign the result yourself');
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
// Write a value/apply a function at every matched node. 0 nodes → error.
function patch(target, path, valueOrFn) { return __applyPatch('patch', target, path, valueOrFn, 1); }
// Same, but no matches is not an error.
function tryPatch(target, path, valueOrFn) { return __applyPatch('tryPatch', target, path, valueOrFn, 0); }

// All matched values as an array.
function pick(target, path, __op) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  var out = [];
  for (var i = 0; i < locs.length; i++) out.push(__getAt(d.doc, locs[i]));
  __traceOp(__op || 'pick', path, locs.length);
  return out;
}
// First matched value, or null.
function pickOne(target, path) {
  var r = pick(target, path, 'pickOne');
  return r.length ? r[0] : null;
}
// Remove matched nodes. Reverse order so splice doesn't shift indices not yet processed.
function removeAt(target, path) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  for (var i = locs.length - 1; i >= 0; i--) {
    var loc = locs[i];
    if (!loc.length) continue; // don't remove the root
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
// Deep-merge an object into every matched node. 0 nodes → error.
function mergeAt(target, path, obj) {
  var d = __doc(target);
  var locs = __locate(d.doc, path);
  if (!locs.length) throw new Error('mergeAt("' + path + '"): 0 nodes. Body: ' + __shape(d.doc));
  for (var i = 0; i < locs.length; i++) {
    var n = __getAt(d.doc, locs[i]);
    if (n && typeof n === 'object') __deepMerge(n, obj);
  }
  __syncDoc(d);
  __traceOp('mergeAt', path, locs.length);
  return locs.length;
}

// ── URL/query ──
// request.url and request.path must change together.
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
// Changes the host and authority in the url. Doesn't touch the Host header — the proxy manages it.
function rewriteHost(req, host) {
  req.host = String(host);
  if (req.url) req.url = String(req.url).replace(/^(https?:\/\/)[^\/]+/, '$1' + host);
}
// from: a string (all occurrences replaced) or a RegExp. The query part is not affected.
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

// ── Mocks and responses ──
function __mockOrReturn(resp) {
  // request/response phase: ctx.mock exists → mock; handler: just return the object.
  if (typeof ctx !== 'undefined' && typeof ctx.mock === 'function') ctx.mock(resp);
  return resp;
}
// json({obj}) | json(status, {obj}) — a JSON response in one line.
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
// Blocking pause. Handler phase only: request/response run on the engine's shared thread.
function delay(ms) {
  if (typeof __native_sleep !== 'function') {
    throw new Error('delay() is only available in the handler phase (phase: handler)');
  }
  __native_sleep(Math.max(0, Number(ms) || 0));
}

// ── Data generation ──
function uuid() {
  var hex = '0123456789abcdef', s = [];
  for (var i = 0; i < 36; i++) s[i] = hex[Math.floor(Math.random() * 16)];
  s[14] = '4';
  s[19] = hex[(parseInt(s[19], 16) & 0x3) | 0x8];
  s[8] = s[13] = s[18] = s[23] = '-';
  return s.join('');
}
// Integer from [a, b] inclusive.
function randomInt(a, b) { return Math.floor(Math.random() * (b - a + 1)) + a; }
function randomFrom(arr) { return arr[Math.floor(Math.random() * arr.length)]; }
// nowISO('+2d', '+05:00') → "2026-07-25T…+05:00"; without tz — UTC with 'Z'.
function nowISO(shift, tz) {
  var ms = Date.now();
  if (shift !== undefined && shift !== null) {
    var m = String(shift).match(/^([+-])(\d+)([smhd])$/);
    if (!m) throw new Error('nowISO: shift like "+2d", "-30m", "+1h", "+10s"');
    var mult = { s: 1e3, m: 6e4, h: 36e5, d: 864e5 }[m[3]];
    ms += (m[1] === '-' ? -1 : 1) * Number(m[2]) * mult;
  }
  var offMin = 0;
  if (tz !== undefined && tz !== null) {
    var t = String(tz).match(/^([+-])(\d\d):(\d\d)$/);
    if (!t) throw new Error('nowISO: tz like "+05:00"');
    offMin = (t[1] === '-' ? -1 : 1) * (Number(t[2]) * 60 + Number(t[3]));
  }
  var d = new Date(ms + offMin * 6e4);
  function p(n) { return (n < 10 ? '0' : '') + n; }
  var iso = d.getUTCFullYear() + '-' + p(d.getUTCMonth() + 1) + '-' + p(d.getUTCDate()) +
    'T' + p(d.getUTCHours()) + ':' + p(d.getUTCMinutes()) + ':' + p(d.getUTCSeconds());
  return iso + (tz ? tz : 'Z');
}

// ── Collections ──
function __keyFn(key) {
  return typeof key === 'function' ? key : function (x) { return x == null ? undefined : x[key]; };
}
function groupBy(arr, key) {
  var f = __keyFn(key), r = {};
  for (var i = 0; i < arr.length; i++) {
    var g = String(f(arr[i]));
    (r[g] = r[g] || []).push(arr[i]);
  }
  return r;
}
// Returns a sorted copy (doesn't touch the original array).
function sortBy(arr, key) {
  var f = __keyFn(key);
  return arr.slice().sort(function (a, b) {
    var x = f(a), y = f(b);
    return x < y ? -1 : x > y ? 1 : 0;
  });
}
function uniqBy(arr, key) {
  var f = __keyFn(key), seen = {}, r = [];
  for (var i = 0; i < arr.length; i++) {
    var k = String(f(arr[i]));
    if (!seen[k]) { seen[k] = 1; r.push(arr[i]); }
  }
  return r;
}
function chunk(arr, n) {
  var r = [];
  for (var i = 0; i < arr.length; i += n) r.push(arr.slice(i, i + n));
  return r;
}
// n random elements without repeats (Fisher–Yates on a copy).
function sample(arr, n) {
  var a = arr.slice();
  for (var i = a.length - 1; i > 0; i--) {
    var j = Math.floor(Math.random() * (i + 1));
    var t = a[i]; a[i] = a[j]; a[j] = t;
  }
  return a.slice(0, Math.min(n === undefined ? 1 : n, a.length));
}
