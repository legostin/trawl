# Trawl Rule Editor — What You Can Do

Trawl sits between your client and the server as a MITM HTTP(S) proxy. A
**rule** is how you reach into that traffic: a glob pattern that selects
which requests it applies to, a **phase** that decides *when* it runs, and a
JavaScript **script** — executed in a sandboxed QuickJS engine — that reads
or rewrites the request, the response, or both. Rules can live on a specific
**project** or be **global**, applying across every project you work in.

This guide walks through the mental model, the phases, the standard
library, the JSONPath dialect that powers most of it, and the live tooling
the editor gives you while you write a rule. It closes with a handful of
worked, end-to-end examples.

If you just want copy-paste snippets, see
[`scripting-cookbook.md`](./scripting-cookbook.md).

## What a rule is

Every rule has three parts:

1. **Pattern** — a glob matched against `host + path`, e.g.
   `api.example.com/v3/users/*` or `*/auth/login*`. It decides which flows
   the rule is even considered for.
2. **Phase** — `request`, `response`, or `handler`. It decides at which
   point in the request lifecycle the script runs, and consequently what
   globals are available to it.
3. **Script** — plain JavaScript, run in an embedded QuickJS engine. No
   Node/browser APIs, no network access outside the functions Trawl exposes
   explicitly, no filesystem. This is a sandbox: the script can only affect
   traffic through the objects and functions described below.

A flow can be matched by several rules across phases (a `request` rule and a
`response` rule can both apply to the same exchange); within a phase, rules
run in order and each can further mutate what the previous one produced.

## The three phases

### `request` — rewrite before it leaves

Runs before the request reaches the server. You get a mutable `request`
object:

```js
request.method   // "GET", "POST", ...
request.url      // full URL
request.host
request.path
request.headers  // header bag
request.body     // raw body (string/bytes)
```

Mutate any of these in place and Trawl forwards the modified request. You
can also **short-circuit** the whole exchange — skip the network entirely —
by calling a mocking helper or `ctx.mock(...)`:

```js
// Short-circuit with a JSON mock — server is never contacted.
json({ maintenance: true });

// Or, more explicitly:
ctx.mock(json(503, { error: 'maintenance' }));
```

Two more escape hatches available in `request`:

- `ctx.abort()` — cancel the request outright (client sees a connection
  failure).
- `ctx.breakpoint()` — pause the flow so you can inspect/edit it by hand
  before it continues (see `list_paused` / `resolve_breakpoint` tooling).

### `response` — rewrite before the client sees it

Runs after the server has answered, before the response is delivered back
to the client. You get:

```js
response.status
response.headers
response.body
```

...plus the original `request` (read-only context — useful for deciding
what to do based on which endpoint was hit). This is the natural home for
"strip a field", "inject a value into every item", "poison one response for
testing" style rules — anything that reacts to what the *real* server sent
back.

### `handler` — you drive the exchange

The most powerful and most manual phase. Trawl does **not** perform the
request for you — your script does, by calling `send(request)` (blocking).
This means you can:

- Retry (`sendWithRetry`), delay (`delay(ms)`), or skip the real call
  entirely and fabricate a response.
- Make *additional* requests (auth refresh, a side lookup) before deciding
  what to return.
- Inspect the response and re-request if it doesn't look right.

A handler rule **must `return` a response** — that return value is what the
client receives. Forgetting to `return` is the single most common bug in
handler rules.

```js
const res = send(request);
return res;
```

`sendJsonRequest(request)` and `sendWithRetry(request, opts)` are only
meaningful here, since only the handler phase actually dispatches requests.

### `env` and the Variables API — state across phases and rules

`env` is available in every phase. It's a merged **global + project**
key/value store: reads see project values overriding global ones, and
writes go to whichever project is currently active. Use it to carry state
across separate requests — the classic case is capturing a login token in
one rule and injecting it as a bearer token in another (see the worked
example below).

The stdlib wraps it in three explicit helpers — prefer these over raw `env`
access, they make the intent readable and handle the missing-key case:

```js
setVariable('token', pickOne(response, 'data.accessToken')); // response phase
...
bearer(getVariable('token', ''));                            // request phase, later
deleteVariable('token');                                     // drop it
```

**When exactly is a variable persisted?** After the rule (or rule chain)
finishes running on **real traffic**, the modified `env` is written back:

- with an **active project** — to that project's variables on disk; a write to
  a key that came from the global env becomes a *project override* (the global
  value itself is untouched);
- with **no active project** — to the global variables.

Details worth knowing:

- **Non-string values are stringified** on writeback. `setVariable('n', 42)`
  is readable as `42` within the same run, but comes back as `"42"` on the next.
- **Dry-run never persists.** "Test on traffic" and MCP `test_rule` show what
  the script *would* write (the `env` field of the dry-run output) without
  saving anything.
- **Deletion caveat:** deleting a variable that only exists in the *global* env
  while a project is active does not persist — a project can override global
  values but not erase them. Deleting a project-level variable resurfaces the
  global one.

For state that should *not* survive an app restart (attempt counters, "fire
only once" flags), use the in-memory `counter()`/`once()`/`everyNth()` helpers
instead — see "State counters" below.

## The JSONPath superpower

Most of the body-editing stdlib (`patch`, `pick`, `removeAt`, `mergeAt`, ...)
takes a **JSONPath** string as its second argument. This is the real engine
behind rule scripting — it's what lets you say "every `price` field under
every advert item" without writing a loop.

Paths follow **RFC 9535 JSONPath**. The leading `$.` is optional in Trawl —
`items[*].price` and `$.items[*].price` are equivalent; use whichever reads
better to you.

### Syntax cheat sheet

| Syntax | Meaning |
|---|---|
| `$` | the root value |
| `.field` | child field access |
| `[0]` / `[-1]` | index by position (negative counts from the end) |
| `[0:3]` | slice — elements 0, 1, 2 |
| `[*]` | every element/value at this level |
| `$..field` | recursive descent — `field` at any depth |
| `[?@.type=='advert']` | filter — keep elements where the expression on `@` (the current element) is true |
| `[?@.price>1000 && @.isVip]` | filters support boolean logic (`&&`, `\|\|`, `!`) and comparisons |
| `length()`, `count()`, `match()`, `search()` | JSONPath functions, usable inside filters |
| `['key with spaces']` | bracket form for keys that aren't valid dot-identifiers |

A few concrete paths:

```
items[*].advertData.price          // price of every item
items[?@.type=='advert']           // only advert-type items
items[?@.price>1000 && @.isVip]    // expensive items belonging to VIP sellers
$..recommendationAnalyticsData     // that field, no matter how deep it's nested
data.items[0:5]                    // first five items
data['user id']                    // key containing a space
```

## The stdlib

All functions below are globals — no `require`/`import` needed.

### Body access (JSONPath-powered)

The workhorses. `target` may be a **message** (`request`, `response`, or the
object returned by `send`) or a **plain already-parsed object** — `patch`
and friends parse the body, apply the change, and re-serialize it back onto
the message automatically. You never need to call `JSON.parse`/`stringify`
yourself when using these.

- **`patch(target, path, valueOrFn) -> number`**
  Writes `valueOrFn` (or, if it's a function, applies it) to every node
  matched by `path`, and returns how many nodes were touched. **Zero
  matches throws** — this is Trawl's fail-closed default, so a typo'd path
  breaks the rule loudly instead of silently doing nothing.

  ```js
  patch(res, 'items[*].advertData.addDateFormatted', nowISO(null, '+05:00'));
  patch(res, "items[?@.type=='advert'].advertData.price", p => p * 2);
  ```

- **`tryPatch(target, path, valueOrFn)`**
  Same as `patch`, but zero matches is fine — use it for fields that are
  only sometimes present.

  ```js
  tryPatch(res, 'items[*].badge', 'sale'); // fine even if no item has this shape
  ```

- **`pick(target, path) -> any[]`**
  Collects every value matched by `path` into an array.

  ```js
  const prices = pick(res, 'items[*].advertData.price');
  ```

- **`pickOne(target, path) -> any|null`**
  Like `pick`, but returns just the first match, or `null` if there is none.

  ```js
  env.token = pickOne(response, 'data.accessToken');
  ```

- **`removeAt(target, path) -> number`**
  Deletes every matched node; returns the count removed.

  ```js
  removeAt(response, '$..recommendationAnalyticsData');
  ```

- **`mergeAt(target, path, obj) -> number`**
  Deep-merges `obj` into every matched node.

  ```js
  mergeAt(res, 'items[*]', { debugMark: uuid() });
  ```

- **`jsonBody(msg)`** — parse and return the message body as an object.
- **`setJsonBody(msg, obj)`** — serialize `obj` and write it back as the
  message body. You need this only when you mutate a parsed object by hand
  instead of using `patch`/`removeAt`/`mergeAt`, which do it for you.

  ```js
  const body = jsonBody(res);
  body.items.push({ synthetic: true });
  setJsonBody(res, body);
  ```

### Headers

- **`header(msg, name)`** — read a header value.
- **`hasHeader(msg, name)`** — boolean presence check.
- **`setHeader(msg, name, value)`** — set/overwrite a header.
- **`removeHeader(msg, name)`** — delete a header.
- **`bearer(token)`** — sets `Authorization: Bearer <token>` on `request`.

  ```js
  if (!hasHeader(request, 'X-Client-Version')) {
    setHeader(request, 'X-Client-Version', '9.9.9');
  }
  bearer(env.token);
  ```

### URL & query

- **`queryParam(req, name)`** — read a query parameter.
- **`setQueryParam(req, name, value)`** — set/add a query parameter.
- **`removeQueryParam(req, name)`** — remove a query parameter.
- **`rewriteHost(req, host)`** — swap the host, keeping path/query intact.
- **`rewritePath(req, from, to)`** — replace part of the path; `from` can be
  a string or a `RegExp`.
- **`pathSegments(req)`** — the path split into an array of segments.

  ```js
  rewriteHost(request, 'staging.example.com');
  rewritePath(request, '/v3/', '/v4/');
  setQueryParam(request, 'limit', 100);
  ```

### Mocks & responses

- **`json(obj)`** / **`json(status, obj)`** — build a JSON response
  (defaults to status 200 if omitted). In the `request` phase, calling this
  short-circuits the exchange.
- **`textResponse(status, body, contentType?)`** — build a plain-text (or
  custom content-type) response.
- **`httpError(status, msg?)`** — shorthand for an error response.
- **`delay(ms)`** — pause before continuing. **Handler-phase only** — calling
  it in `request`/`response` throws.

  ```js
  json({ featureFlags: { newUi: true }, maintenance: false });
  httpError(500, 'internal error (test)');
  delay(3000); // handler phase only
  ```

### Data generators

- **`uuid()`** — a random UUID v4 string.
- **`randomInt(a, b)`** — random integer, inclusive of both bounds.
- **`randomFrom(arr)`** — a random element of `arr`.
- **`randomFloat(a, b)`** — random float in `[a, b)`.
- **`randomBool(p?)`** — `true` with probability `p` (default 0.5) — the
  one-liner behind percentage-based mocks.
- **`nowISO(shift?, tz?)`** — current time as ISO-8601, optionally shifted
  and/or in a given timezone offset. `shift` is `±N` followed by
  `s|m|h|d`, e.g. `'+2d'`, `'-30m'`.

  ```js
  nowISO();                    // now, UTC
  nowISO('+2d', '+05:00');     // two days from now, +05:00 offset
  randomFrom(['A', 'B', 'C']);
  ```

For realistic mocks there is a small built-in faker — no external data, no
network:

- **`fakeName()`** / **`fakeEmail()`** / **`fakePhone(format?)`** — a
  plausible person. In `fakePhone`, every `#` in the format becomes a random
  digit (default format `'+1-555-###-####'`).
- **`lorem(nWords)`** — lorem-ipsum text for descriptions and long fields.
- **`fakeList(n, fn)`** — an array of `n` items built by `fn(i)`; the
  workhorse for list mocks.

  ```js
  json({
    users: fakeList(10, i => ({
      id: i + 1,
      name: fakeName(),
      email: fakeEmail(),
      bio: lorem(12),
    })),
  });
  ```

### Collections

- **`groupBy(arr, keyOrFn)`** — group array elements into an object keyed by
  a field name or a function.
- **`sortBy(arr, keyOrFn)`** — returns a **sorted copy** (does not mutate).
- **`uniqBy(arr, keyOrFn)`** — dedupe by a key.
- **`chunk(arr, n)`** — split into arrays of length `n`.
- **`sample(arr, n)`** — `n` random elements from `arr`.

  ```js
  const byType = groupBy(items, 'type');
  const cheapFirst = sortBy(items, i => i.advertData.price);
  ```

### Auth & encoding

Hashes and base64 run natively (Rust side) — fast and always available; no
`atob`/`crypto` polyfills needed.

- **`base64Encode(s)`** / **`base64Decode(s)`** — decode accepts standard
  *and* url-safe base64, with or without padding; invalid input throws.
- **`jwtDecode(token)`** — split and decode a JWT into
  `{ header, payload }` **without verifying the signature** (it's a
  debugging tool, not an auth check). Accepts a bare token or the whole
  `'Bearer <token>'` header value.
- **`sha256(s)`** / **`md5(s)`** — lowercase-hex digests.
- **`hmacSha256(key, s)`** — lowercase-hex HMAC-SHA256, for APIs that
  require signed requests.

  ```js
  const { payload } = jwtDecode(header(request, 'authorization'));
  if ((payload.exp || 0) * 1000 < Date.now()) notify('token expired');

  setHeader(request, 'X-Signature', hmacSha256(secret('hmac_key'), request.body));
  setHeader(request, 'Authorization', 'Basic ' + base64Encode('user:' + secret('pw')));
  ```

### Cookies & forms

Cookie helpers adapt to what you hand them: on a **request** they work on the
`Cookie` header, on a **response** they work on `Set-Cookie`.

- **`cookies(msg)`** — all cookies as an object (request: every pair of the
  `Cookie` header; response: the leading `name=value` of `Set-Cookie`).
- **`cookie(msg, name)`** — one value, `undefined` when absent.
- **`setCookie(msg, name, value, attrs?)`** — request: add/replace the pair;
  response: write a `Set-Cookie` header, with `attrs` supporting `path`,
  `domain`, `maxAge`, `expires`, `secure`, `httpOnly`, `sameSite`.
- **`removeCookie(msg, name)`** — request: drop the pair (and the header once
  empty); response: write a deletion instruction (`Max-Age=0`).

Limitation to know: the script header map holds **one value per header name**,
so a scripted response can carry only one `Set-Cookie`.

For `application/x-www-form-urlencoded` bodies (classic form logins, OAuth
token endpoints):

- **`formBody(msg)`** — the whole body as a decoded object.
- **`formParam(msg, name)`** — one field, `undefined` when absent.
- **`setFormParam(msg, name, value)`** / **`setFormBody(msg, obj)`** —
  re-encode the body (and set the content-type if missing).

  ```js
  setCookie(response, 'session', 'test', { path: '/', httpOnly: true });
  const user = formParam(request, 'username');
  ```

### State counters (in-memory)

For scenario mocks that need memory *between* requests — "fail the first N",
"fire only once", "every 5th request misbehaves":

- **`counter(name)`** — increment and return (first call → 1).
- **`resetCounter(name)`** — start over.
- **`once(name)`** — `true` only on the first call for this name.
- **`everyNth(name, n)`** — `true` on calls n, 2n, 3n, …

Two properties to keep in mind:

- State is **in-memory and per app session** — restarting Trawl resets every
  counter. That's deliberate: debugging scenarios shouldn't leak into
  tomorrow's session. For state that must survive, use `setVariable()`.
- **Dry-run is isolated**: "Test on traffic" runs against a fresh store, so
  `counter()` always returns 1 there and `once()` is always `true` —
  deterministic tests that never disturb the live counters.

  ```js
  const attempt = counter('warmup');
  if (attempt <= 3) return httpError(503, 'attempt ' + attempt);
  return send(request);
  ```

### Network (handler phase)

- **`send(req)`** — perform the request, blocking; returns
  `{status, headers, body}` (raw body — no `.data`).
- **`sendJsonRequest(req)`** — like `send`, but also parses the body onto
  `.data`.
- **`sendWithRetry(req, { retries, delay })`** — retries on failure with a
  delay between attempts (ms).
- **`sleep(ms)`** — plain async sleep, usable anywhere in `handler` scripts.

  ```js
  const res = send(request);
  return res;

  return sendWithRetry(request, { retries: 5, delay: 500 });
  ```

### Other

- **`secret(name)`** — read a value from the OS Keychain (for API keys,
  tokens you don't want inline in a script).
- **`notify(text, { channel, title })`** — send a notification, e.g. to
  flag when a rule fired on something interesting.

## Fail-closed philosophy

Trawl scripting is deliberately **fail-closed**: if a script throws — a
syntax error, a runtime error, or a `patch` call that matched zero nodes —
the entire flow fails with a diagnostic error rather than silently passing
traffic through unmodified. The error includes:

- the **rule name** that failed,
- the **script line** where it failed,
- a **shape summary** of the body involved (so you can see what fields
  actually existed, without needing to log the whole payload yourself).

This is why `patch` throws on zero matches by default — a path that quietly
matches nothing is almost always a bug (a field got renamed, the response
shape changed) and you want to know immediately, not have a rule that
silently does nothing for months. When a field is genuinely optional, say
so explicitly with `tryPatch`.

## The editor's live tooling

The rule editor isn't just a text box — it's built around your **actual
captured traffic**:

- **Segment autocomplete** — as you type a JSONPath, the editor suggests the
  next segment (`.field`, `[*]`, filter keys) based on the real shape of
  flows that matched your rule's pattern.
- **Live JSONPath syntax markers** — malformed paths are underlined as you
  type, before you ever save or run the rule.
- **Inline match count** — next to your path, an inline "→ N nodes" tells
  you exactly how many nodes it resolves to against the latest captured
  flow, so you can catch a `0`-match path (which would throw under
  fail-closed) before it ever runs for real.
- **Save-time validation** — both the JavaScript and every JSONPath
  expression in it are validated when you save; a rule that would fail
  immediately is blocked from being saved at all.
- **"Test on traffic" dry-run** — replays a captured response through your
  script and shows a **before/after diff** of the body plus the full
  **operation trace**, with no network calls and nothing persisted. This is
  the fastest way to iterate on a `patch`/`mergeAt` expression until it does
  exactly what you want.

## The execution trace

Every time a rule actually runs against live traffic, Trawl records a trace
of its operations — visible in the flow's details view. Typical entries:

```
patch → 3 nodes
send → 200 (184ms)
tryPatch → 0 nodes
```

This is invaluable for debugging "why didn't my rule do anything" — you can
see exactly how many nodes each `patch`/`removeAt`/`mergeAt` call touched,
and the status/timing of every `send` your handler made, without adding any
manual logging to the script.

## Worked end-to-end examples

### 1. Stamp a formatted date on every recommendation item

Goal: every item returned by the recommendations endpoint gets a
human-readable `addDateFormatted` field, computed at response time in a
fixed timezone.

```js
// phase: handler, pattern: app.kolesa.kz/v3/adverts/recommendation*
const res = send(request);
patch(res, 'items[*].advertData.addDateFormatted', nowISO(null, '+05:00'));
return res;
```

Using the inline match-count while writing `items[*].advertData` against a
captured recommendation flow confirms the path resolves before you ever
save the rule.

### 2. Redirect a host to staging and bump the API version

Goal: every request to the production API host is transparently redirected
to staging, and the version segment in the path is bumped from v3 to v4 —
useful for testing an upcoming API against production-shaped client
traffic.

```js
// phase: request, pattern: api.example.com/*
rewriteHost(request, 'staging.example.com');
rewritePath(request, '/v3/', '/v4/');
```

### 3. Capture an auth token, then inject it later

Goal: grab the access token the moment a login response comes back, store
it in `env`, and attach it to every subsequent outgoing request as a
bearer token — no more manually copying tokens between requests.

```js
// Rule A — phase: response, pattern: */auth/login*
env.token = pickOne(response, 'data.accessToken');
```

```js
// Rule B — phase: request, pattern: api.example.com/*
if (env.token) {
  bearer(env.token);
}
```

`env` persists to the active project, so the token survives across
requests (and across app restarts, as long as you stay in the same
project).

### 4. Emulate a flaky, slow endpoint

Goal: stress-test a client's timeout/retry handling by making one endpoint
intermittently slow and occasionally return an error.

```js
// phase: handler, pattern: */api/checkout*
if (randomInt(1, 100) <= 20) {
  httpError(503, 'simulated overload');
}
delay(randomInt(500, 4000));
return sendWithRetry(request, { retries: 2, delay: 300 });
```

Roughly one in five requests fails immediately with a 503; the rest are
delayed by a random amount up to 4 seconds and then sent for real, retrying
up to twice if the upstream call itself fails.

---

For more short, focused recipes in this same style, see
[`scripting-cookbook.md`](./scripting-cookbook.md).
