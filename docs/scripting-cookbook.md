# Trawl Rule Recipes

Every recipe is a ready-to-use rule: pattern, phase, script. For a full function
reference, see the Function library in the app; the JSONPath syntax cheat sheet
is there too.

## 1. Mock an endpoint entirely
phase: request, pattern: `*/api/config*`
```js
json({ featureFlags: { newUi: true }, maintenance: false });
```

## 2. Set a field on every array element
phase: handler, pattern: `app.kolesa.kz/v3/adverts/recommendation*`
```js
const res = send(request);
patch(res, 'items[*].advertData.addDateFormatted', nowISO(null, '+05:00'));
return res;
```

## 3. Change only elements matching a condition
phase: handler
```js
const res = send(request);
tryPatch(res, "items[?@.type=='advert'].advertData.price", p => p * 2);
return res;
```

## 4. Remove a field wherever it appears
phase: response
```js
removeAt(response, '$..recommendationAnalyticsData');
```

## 5. Redirect requests to staging
phase: request, pattern: `api.example.com/*`
```js
rewriteHost(request, 'staging.example.com');
```

## 6. Rewrite the API version in the path
phase: request
```js
rewritePath(request, '/v3/', '/v4/');
```

## 7. Emulate a slow network
phase: handler
```js
delay(3000);
return send(request);
```

## 8. Emulate a 500 error
phase: request
```js
httpError(500, 'internal error (test)');
```

## 9. Inject your own query parameter
phase: request
```js
setQueryParam(request, 'limit', 100);
```

## 10. Pull a token from the login response into env
phase: response, pattern: `*/auth/login*`
```js
env.token = pickOne(response, 'data.accessToken');
```

## 11. Inject a saved token into requests
phase: request
```js
bearer(env.token);
```

## 12. A/B: swap a fraction of responses
phase: handler
```js
const res = send(request);
if (randomInt(1, 100) <= 50) tryPatch(res, 'experiments.variant', 'B');
return res;
```

## 13. Enrich every array element
phase: handler
```js
const res = send(request);
mergeAt(res, 'items[*]', { debugMark: uuid() });
return res;
```

## 14. Keep only the first 3 elements in the response
phase: handler
```js
const res = send(request);
patch(res, 'items', items => items.slice(0, 3));
return res;
```

## 15. Retry a flaky upstream
phase: handler
```js
return sendWithRetry(request, { retries: 5, delay: 500 });
```

## 16. Fail the first 3 requests, then pass through
phase: handler
```js
const attempt = counter('warmup');
if (attempt <= 3) return httpError(503, 'warming up, attempt ' + attempt);
return send(request);
```

## 17. Mock an endpoint only once per session
phase: request
```js
if (once('first-config')) {
  json({ featureFlags: { newUi: true }, firstLaunch: true });
}
```

## 18. Fail every 5th request
phase: handler
```js
if (everyNth('flaky', 5)) return httpError(500, 'every 5th fails');
return send(request);
```

## 19. Decode and inspect a JWT from the Authorization header
phase: request
```js
const auth = header(request, 'authorization');
if (auth) {
  const { payload } = jwtDecode(auth);
  if ((payload.exp || 0) * 1000 - Date.now() < 60000) {
    notify('JWT expires in <60s (sub: ' + payload.sub + ')', { title: 'Auth' });
  }
}
```

## 20. Sign outgoing requests with HMAC-SHA256
phase: request
```js
setHeader(request, 'X-Signature', hmacSha256(secret('hmac_key'), request.body));
```

## 21. Add Basic auth from a secret
phase: request
```js
setHeader(request, 'Authorization', 'Basic ' + base64Encode('admin:' + secret('admin_pw')));
```

## 22. Rewrite the session cookie on responses
phase: response
```js
setCookie(response, 'session', 'test-session', { path: '/', httpOnly: true, sameSite: 'Lax' });
```

## 23. Strip a tracking cookie from all requests
phase: request
```js
removeCookie(request, '_tracking');
```

## 24. Capture credentials from a form login into variables
phase: request, pattern: `*/auth/login*`
```js
const user = formParam(request, 'username');
if (user !== undefined) setVariable('lastLoginUser', user);
```

## 25. Paginated fake list keyed by the page query param
phase: request
```js
const page = Number(queryParam(request, 'page') || 1);
json({
  page: page,
  items: fakeList(20, i => ({ id: (page - 1) * 20 + i + 1, name: fakeName(), description: lorem(8) })),
  hasMore: page < 5,
});
```

## 26. A/B: mock 10% of traffic
phase: handler
```js
if (randomBool(0.1)) return json({ variant: 'B', mocked: true });
return send(request);
```

## 27. Fake user directory with realistic data
phase: request
```js
json({
  users: fakeList(10, i => ({
    id: i + 1,
    name: fakeName(),
    email: fakeEmail(),
    phone: fakePhone(),
    bio: lorem(12),
  })),
});
```

## Common mistakes
- `send()` returns `{status, headers, body}` — it has NO `.data` field.
  Parsed JSON is given by `sendJsonRequest()` (`.data` field) or `jsonBody(res)`.
- Mutating `res.data` / a parsed object by itself does NOT change `body` —
  serialize it back with `setJsonBody(res, obj)`. `patch`/`removeAt`/`mergeAt`
  do this automatically.
- A handler rule must return a response: `return res;`.
- `patch` with 0 matches is an error (fail-closed). For optional fields, use `tryPatch`.
- `delay()` only works in the handler phase.
- `counter()`/`once()`/`everyNth()` state is in-memory and per app session — it
  resets on restart. Dry-run ("Test on traffic") uses an isolated store, so
  inside a dry-run `counter()` always starts at 1 and `once()` is always true.
- `setVariable()` persists only after a rule runs on real traffic; dry-run shows
  the change in its output but never saves it.
- Deleting a variable that only exists in the global env while a project is
  active does not persist the deletion (the project can only override, not
  erase, global variables).
- The script header map holds one value per header name, so a scripted response
  can carry only one `Set-Cookie`.
