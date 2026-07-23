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

## Common mistakes
- `send()` returns `{status, headers, body}` — it has NO `.data` field.
  Parsed JSON is given by `sendJsonRequest()` (`.data` field) or `jsonBody(res)`.
- Mutating `res.data` / a parsed object by itself does NOT change `body` —
  serialize it back with `setJsonBody(res, obj)`. `patch`/`removeAt`/`mergeAt`
  do this automatically.
- A handler rule must return a response: `return res;`.
- `patch` with 0 matches is an error (fail-closed). For optional fields, use `tryPatch`.
- `delay()` only works in the handler phase.
