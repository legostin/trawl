export interface Snippet {
  label: string;
  code: string;
}

export const SNIPPETS: Snippet[] = [
  {
    label: "Handler: send + ретрай",
    code:
      "let response = send(request);\n" +
      "while (response.status === 429) {\n" +
      "  sleep(1000);\n" +
      "  response = send(request);\n" +
      "}\n" +
      "return response;\n",
  },
  {
    label: "Handler: правка ответа",
    code:
      "const response = send(request);\n" +
      "const data = JSON.parse(response.body || '{}');\n" +
      "data.patched = true;\n" +
      "response.body = JSON.stringify(data);\n" +
      "return response;\n",
  },
  {
    label: "Заголовок запроса",
    code: "request.headers['X-Debug'] = '1';\n",
  },
  {
    label: "Правка JSON-запроса",
    code:
      "const data = JSON.parse(request.body || '{}');\n" +
      "data.injected = true;\n" +
      "request.body = JSON.stringify(data);\n",
  },
  {
    label: "Мок ответа",
    code:
      "ctx.mock({\n" +
      "  status: 200,\n" +
      "  headers: { 'content-type': 'application/json' },\n" +
      "  body: JSON.stringify({ ok: true, mocked: true }),\n" +
      "});\n",
  },
  {
    label: "Правка JSON-ответа",
    code:
      "if (response) {\n" +
      "  const data = JSON.parse(response.body || '{}');\n" +
      "  data.patched = true;\n" +
      "  response.body = JSON.stringify(data);\n" +
      "}\n",
  },
  {
    label: "Подмена статуса",
    code: "if (response) { response.status = 503; }\n",
  },
];
