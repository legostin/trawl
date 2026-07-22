# Trawl

A modern HTTP(S) debugging proxy for macOS — think Charles or Proxyman, with a
clean native UI. Capture and inspect traffic (HTTPS included, via a locally
generated CA), rewrite requests and responses with embedded JavaScript, pause
and edit live traffic with **breakpoints**, mock endpoints, and organise work
into **projects** with their own rules, tracked hosts and environment variables.
Extend it with **plugins**.

Built with **Tauri 2** (Rust core + React/TypeScript UI).

---

## Download

Grab the latest signed DMG from the **[Releases](https://github.com/legostin/trawl/releases/latest)** page.

| Platform | Package | Notes |
| --- | --- | --- |
| **macOS 12+** (Apple Silicon **and** Intel) | `Trawl_x.y.z_universal.dmg` | Universal binary. Signed + notarized. |

Trawl **auto-updates**: it checks on launch and offers an in-app *Update to
vX.Y.Z* button when a newer release is available.

---

## Quick start

1. Launch Trawl and press **Start** (the proxy listens on `0.0.0.0:8729`).
2. Point a client at the proxy — for a quick test, use the **Setup** tab’s
   guided flows for this Mac, Chrome, the iOS Simulator, the Android Emulator,
   or a physical phone.
3. To decrypt **HTTPS**, install Trawl’s CA certificate: through the proxy, open
   `http://trawl/` to download it, then trust it (the Setup tab automates this
   per platform).
4. Traffic streams into the list. Click a request to inspect it.

---

## Features

- **Live capture** of HTTP/HTTPS with a sequence list and a domain tree.
- **HTTPS interception** via an on-device CA, with guided setup for this Mac,
  Chrome, the iOS Simulator, the Android Emulator, and physical phones.
- **Request detail**: Query / Form / Headers / Body / Cookies tabs, cURL export,
  and one-click mocks.
- **JavaScript rules** (request / response / handler phases) in a Monaco editor
  with autocomplete, saved library functions, synchronous `send()` / `sleep()`,
  access to Keychain secrets via `secret()`, and notifications via `notify()`.
- **Interactive breakpoints** — pause a matching request or response in flight,
  edit method / URL / query / headers / body (with JSON/format-aware editing and
  file substitution), then **Execute**, **Abort**, or **Respond locally**.
  Configurable auto-continue timeout and a “pause others while intercepting”
  option. Arm from a Breakpoints list or from a rule via `ctx.breakpoint()`.
- **Projects**: scoped capture (only tracked hosts are recorded), per-project
  rules and breakpoints, and read/write environment variables usable from scripts
  and as `{{VAR}}` in match patterns.
- **Persistent flow DB** (SQLite) for querying and aggregating captured traffic.
- **Plugins**: add whole new modes and toolbar actions. See
  **[docs/plugins.md](docs/plugins.md)**.

---

## Rules & scripting

A rule matches a `host/path` glob (e.g. `api.example.com/*`, `*/v1/*`, or
`{{API_HOST}}/v1/*` using a project variable) and runs a small JavaScript in one
of three phases:

- **request** — mutate the outgoing request (`request.headers`, `request.body`, …),
  `ctx.mock(...)` a response, `ctx.abort(...)`, or `ctx.breakpoint()` to pause it.
- **response** — mutate the response before it reaches the client.
- **handler** — take full control: you call `send(request)` yourself and return
  the response (retries, transforms, synthetic responses…).

Helpers like `setHeader`, `jsonBody`, `bearer`, and `sendJsonRequest` are
built-in; add your own reusable functions in the **Function library**.

---

## Breakpoints

Open the **Breakpoints** view and add a definition (host/path pattern, method,
and whether to pause the request phase, the response phase, or both). While a
flow is paused it appears in the Traffic view for live editing:

- **Request phase** fires *before* any rule — edit and **Execute** to send the
  (edited) request down the rule chain, **Respond locally** to return your own
  response without hitting the server, or **Abort**.
- **Response phase** fires *after* all rules (including handler rules) — edit the
  status/headers/body and **Execute**, or **Abort**.

Header settings: an **auto-continue timeout** (seconds; `0` = hold forever) and
**pause others** (hold new requests while any flow is paused). A red pulsing dot
on the Traffic tab signals a flow is waiting.

---

## MCP server

Trawl embeds an MCP server (Streamable HTTP, `127.0.0.1:9910`, bearer token) so
AI agents can inspect captured traffic, manage rewrite rules, projects and
breakpoints, and resolve paused requests. Grab the ready-made
`claude mcp add …` command in **Settings → MCP server**. Plugins can contribute
their own tools via `__TRAWL__.mcp.registerTool` (see docs/plugins.md).

---

## Development

Prerequisites: Node 20+, pnpm 9+, Rust (stable), and the Tauri prerequisites for
macOS.

```sh
pnpm install
pnpm tauri dev      # run the desktop app
```

Checks:

```sh
pnpm exec tsc --noEmit    # typecheck
pnpm exec vitest run      # frontend unit tests
cd src-tauri && cargo test
```

The default proxy listens on **`0.0.0.0:8729`**. Point a client at it, then open
`http://trawl/` through the proxy to download the CA certificate.

---

## Building & releasing (signed + notarized DMG)

Releases are produced by GitHub Actions (`.github/workflows/release.yml`) on a
version tag:

```sh
# bump "version" in package.json, src-tauri/tauri.conf.json and src-tauri/Cargo.toml
git tag v0.3.0
git push origin v0.3.0
```

The workflow builds a universal (`aarch64` + `x86_64`) macOS bundle, signs it
with a Developer ID Application certificate, notarizes it with Apple, signs the
auto-update artifacts, and **publishes** a GitHub Release with the DMG and the
`latest.json` update feed.

### Required repository secrets

Add these under **Settings → Secrets and variables → Actions**. Signing needs a
paid Apple Developer account.

| Secret | What it is |
| --- | --- |
| `APPLE_CERTIFICATE` | Base64 of your Developer ID Application `.p12` (`base64 -i cert.p12 \| pbcopy`) |
| `APPLE_CERTIFICATE_PASSWORD` | Password protecting that `.p12` |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `KEYCHAIN_PASSWORD` | Any random string; used for the temporary CI keychain |
| `APPLE_ID` | Your Apple ID email (for notarization) |
| `APPLE_PASSWORD` | An app-specific password ([account.apple.com](https://account.apple.com) → Sign-In and Security → App-Specific Passwords) |
| `APPLE_TEAM_ID` | Your 10-character Apple Team ID |
| `TAURI_SIGNING_PRIVATE_KEY` | The updater signing private key (contents of the file from `tauri signer generate`) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password for that updater key |

Without the Apple secrets the build still runs but the DMG is unsigned; macOS
Gatekeeper will warn users until signing is configured. Without the
`TAURI_SIGNING_*` secrets the updater feed is not signed and auto-update won’t
work.

---

## Auto-update

The app checks for updates on launch and shows an **Update to vX.Y.Z** button in
the top bar when a newer release is available; clicking it downloads, installs
and relaunches. There’s also a manual **Check for updates** control. Updates use
the Tauri updater plugin; the feed is served from the GitHub Release’s
`latest.json`.

Updates are cryptographically verified with a **minisign** key that is separate
from Apple code signing. Generate it once:

```sh
pnpm tauri signer generate -w ~/.tauri/trawl-updater.key
```

Put the printed **public key** into `src-tauri/tauri.conf.json`
(`plugins.updater.pubkey`), and add the private key + its password as the
`TAURI_SIGNING_*` secrets above.

### Cutting an update

An update is only offered when the released version is greater than the installed
one:

```sh
# bump the version in package.json, tauri.conf.json and Cargo.toml (e.g. 0.3.0 -> 0.3.1)
git tag v0.3.1
git push origin v0.3.1
```

The workflow publishes the release with a signed `latest.json`; installed copies
pick it up on their next launch. Only **published** (non-draft, non-prerelease)
releases are seen by the updater, which is why the workflow publishes directly.

---

## Plugins

Trawl can be extended with plugins that add new top-level modes and request
toolbar actions, and that read live/persisted traffic through a host API.
See **[docs/plugins.md](docs/plugins.md)** for the plugin model, the
`window.__TRAWL__` API reference, the event catalogue, and a step-by-step guide
to writing and installing one.
