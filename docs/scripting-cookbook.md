# Рецепты правил trawl

Каждый рецепт — готовое правило: паттерн, фаза, скрипт. Справка по функциям —
Function library в приложении, синтаксис путей — шпаргалка JSONPath там же.

## 1. Замокать эндпоинт целиком
phase: request, pattern: `*/api/config*`
```js
json({ featureFlags: { newUi: true }, maintenance: false });
```

## 2. Проставить поле во всех элементах массива
phase: handler, pattern: `app.kolesa.kz/v3/adverts/recommendation*`
```js
const res = send(request);
patch(res, 'items[*].advertData.addDateFormatted', nowISO(null, '+05:00'));
return res;
```

## 3. Изменить только элементы, подходящие под условие
phase: handler
```js
const res = send(request);
tryPatch(res, "items[?@.type=='advert'].advertData.price", p => p * 2);
return res;
```

## 4. Удалить поле везде, где встречается
phase: response
```js
removeAt(response, '$..recommendationAnalyticsData');
```

## 5. Редирект запросов на стейджинг
phase: request, pattern: `api.example.com/*`
```js
rewriteHost(request, 'staging.example.com');
```

## 6. Переписать версию API в пути
phase: request
```js
rewritePath(request, '/v3/', '/v4/');
```

## 7. Эмуляция медленной сети
phase: handler
```js
delay(3000);
return send(request);
```

## 8. Эмуляция 500-й ошибки
phase: request
```js
httpError(500, 'внутренняя ошибка (тест)');
```

## 9. Подложить свой query-параметр
phase: request
```js
setQueryParam(request, 'limit', 100);
```

## 10. Вытащить токен из ответа логина в env
phase: response, pattern: `*/auth/login*`
```js
env.token = pickOne(response, 'data.accessToken');
```

## 11. Подставить сохранённый токен в запросы
phase: request
```js
bearer(env.token);
```

## 12. A/B: часть ответов подменять
phase: handler
```js
const res = send(request);
if (randomInt(1, 100) <= 50) tryPatch(res, 'experiments.variant', 'B');
return res;
```

## 13. Обогатить каждый элемент массива
phase: handler
```js
const res = send(request);
mergeAt(res, 'items[*]', { debugMark: uuid() });
return res;
```

## 14. Оставить в ответе только первые 3 элемента
phase: handler
```js
const res = send(request);
patch(res, 'items', items => items.slice(0, 3));
return res;
```

## 15. Ретрай нестабильного апстрима
phase: handler
```js
return sendWithRetry(request, { retries: 5, delay: 500 });
```

## Частые ошибки
- `send()` возвращает `{status, headers, body}` — поля `.data` у него НЕТ.
  Парсенный JSON даёт `sendJsonRequest()` (поле `.data`) либо `jsonBody(res)`.
- Мутация `res.data`/распарсенного объекта сама по себе НЕ меняет `body` —
  сериализуйте назад через `setJsonBody(res, obj)`. `patch`/`removeAt`/`mergeAt`
  делают это автоматически.
- handler-правило обязано вернуть ответ: `return res;`.
- `patch` с 0 совпадений — ошибка (fail-closed). Для опциональных полей — `tryPatch`.
- `delay()` работает только в handler-фазе.
