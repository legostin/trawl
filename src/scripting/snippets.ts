export interface Snippet {
  label: string;
  code: string;
}

/** Full scripts — clicking replaces the whole editor content. */
export const TEMPLATES: Snippet[] = [
  {
    label: "Handler: send + retry",
    code:
      "// Handler: you perform the request and return the response.\n" +
      "let response = sendWithRetry(request, { retries: 3, delay: 1000 });\n" +
      "return response;\n",
  },
  {
    label: "Handler: edit JSON response",
    code:
      "const response = sendJsonRequest(request);\n" +
      "// response.data autocompletes from past responses\n" +
      "response.data.patched = true;\n" +
      "response.body = JSON.stringify(response.data);\n" +
      "return response;\n",
  },
  {
    label: "Request: edit JSON body",
    code:
      "const data = jsonBody(request) || {};\n" +
      "data.injected = true;\n" +
      "setJsonBody(request, data);\n",
  },
  {
    label: "Response: edit JSON body",
    code:
      "if (response) {\n" +
      "  const data = jsonBody(response) || {};\n" +
      "  data.patched = true;\n" +
      "  setJsonBody(response, data);\n" +
      "}\n",
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
];

/** Fragments — clicking inserts at the cursor. */
export const SNIPPETS: Snippet[] = [
  { label: "sendJsonRequest", code: "const response = sendJsonRequest(request);\n" },
  { label: "sendWithRetry", code: "const response = sendWithRetry(request, { retries: 3, delay: 1000 });\n" },
  { label: "set header", code: "setHeader(request, 'X-Debug', '1');\n" },
  { label: "get header", code: "header(request, 'authorization')" },
  { label: "bearer token", code: "bearer(env.token);\n" },
  { label: "json body", code: "const data = jsonBody(request) || {};\n" },
  { label: "set json body", code: "setJsonBody(request, data);\n" },
  { label: "query param", code: "queryParam(request, 'id')" },
  { label: "override status", code: "if (response) { response.status = 503; }\n" },
];
