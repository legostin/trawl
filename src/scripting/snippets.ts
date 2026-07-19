export interface Snippet {
  label: string;
  code: string;
}

export const SNIPPETS: Snippet[] = [
  {
    label: "Handler: send + retry",
    code:
      "let response = send(request);\n" +
      "while (response.status === 429) {\n" +
      "  sleep(1000);\n" +
      "  response = send(request);\n" +
      "}\n" +
      "return response;\n",
  },
  {
    label: "Handler: edit response",
    code:
      "const response = send(request);\n" +
      "const data = JSON.parse(response.body || '{}');\n" +
      "data.patched = true;\n" +
      "response.body = JSON.stringify(data);\n" +
      "return response;\n",
  },
  {
    label: "Request header",
    code: "request.headers['X-Debug'] = '1';\n",
  },
  {
    label: "Edit JSON request",
    code:
      "const data = JSON.parse(request.body || '{}');\n" +
      "data.injected = true;\n" +
      "request.body = JSON.stringify(data);\n",
  },
  {
    label: "Mock response",
    code:
      "ctx.mock({\n" +
      "  status: 200,\n" +
      "  headers: { 'content-type': 'application/json' },\n" +
      "  body: JSON.stringify({ ok: true, mocked: true }),\n" +
      "});\n",
  },
  {
    label: "Edit JSON response",
    code:
      "if (response) {\n" +
      "  const data = JSON.parse(response.body || '{}');\n" +
      "  data.patched = true;\n" +
      "  response.body = JSON.stringify(data);\n" +
      "}\n",
  },
  {
    label: "Override status",
    code: "if (response) { response.status = 503; }\n",
  },
];
