# Writing a Trawl plugin from inside the app

This is the dialect for plugins written **in the app**, by the assistant, with
`save_plugin`. It is not the same workflow as `docs/plugins.md`, which
describes building a plugin in a repository with Vite and JSX. **There is no
build step here.** What you write is what runs.

Read this before your first `save_plugin` call.

## The five rules

1. **No JSX.** Nothing compiles it. Use `host.react.createElement`, aliased to
   `h`, as in the example below.
2. **No `import` / `export`.** The file is injected as a classic `<script>`,
   not a module. Everything you need hangs off `window.__TRAWL__`.
3. **Hooks come from `host.react`**, never a bare `React`. The host shares one
   React instance with you; a second one breaks hooks at runtime.
4. **The file re-executes on every reload.** Keep top-level code idempotent.
   Re-registering is safe by design — `registerMode` replaces by id.
5. **No top-level `await`.**

## A complete plugin

This is a whole, working `source`. Nothing is elided.

```js
(function () {
  var host = window.__TRAWL__;
  if (!host) return;
  var h = host.react.createElement;

  function Panel() {
    var st = host.react.useState([]);
    var rows = st[0], setRows = st[1];

    host.react.useEffect(function () {
      host.flows.query({ statusClass: "5xx" }, 20).then(setRows);
    }, []);

    return h("div", { className: "h-full overflow-auto p-4" },
      h("h2", { className: "text-lg font-semibold" }, "Recent 5xx"),
      rows.length === 0
        ? h("p", { className: "mt-2 text-sm text-muted-foreground" }, "Nothing yet.")
        : h("ul", { className: "mt-2 space-y-1 text-sm" },
            rows.map(function (r) {
              return h("li", { key: r.id, className: "font-mono text-xs" },
                r.method + " " + r.url.host + r.url.path + " → " + (r.response ? r.response.status : "—"));
            })));
  }

  host.registerMode({ id: "errors", label: "Errors", component: Panel });
})();
```

Two details in there that are easy to get wrong: `component` takes the function
itself, not an element (`Panel`, not `h(Panel)`), and a flow's status lives on
`flow.response`, which is `null` until the response arrives.

## Styling

Use the host's own Tailwind classes so the panel looks like the rest of the
app: `text-muted-foreground`, `border-border`, `bg-muted/40`, `text-foreground`.
`host.ui` exposes the host's components (`Button`, `Input`, `Select`, …) —
prefer them over hand-rolled markup.

## Registering an MCP tool

A plugin can give *you* a new tool:

```js
  host.mcp.registerTool({
    name: "slowest",
    description: "The slowest requests captured so far.",
    inputSchema: { type: "object", properties: { limit: { type: "number" } } },
    handler: function (args) { return host.flows.query({}, args.limit || 10); }
  });
```

Two constraints, both of which will bite otherwise:

- It must be called **synchronously during init**, at the top level. Calling it
  later throws, because that is how the host knows which plugin the tool
  belongs to.
- The tool is not callable in the turn that created it. MCP clients re-list
  their tools on notification; tell the user to ask again rather than retrying
  in a loop.

## What to do when it does not work

`save_plugin` parses the source before writing, so a syntax error comes back to
you immediately with a line. A *runtime* error — a misspelled hook, a property
on `undefined` — happens later, in the window. Call `list_plugins` with the
plugin's id: `lastLoadError` carries what the injection threw.

Do not guess at the API surface. `get_plugin_reference` returns
`src/plugins/api.ts` verbatim; it is the authoritative list of what `host`
offers, and it carries the host API version.

## Restraint

A plugin runs with the host's full authority — it shares the app's `window`. It
can read Keychain secrets, spawn processes and make arbitrary network calls.
Write what the user asked for and nothing more. Do not reach for `host.secrets`
or `host.process` unless that is precisely what they asked you to build, and
say plainly in the chat when you do.
