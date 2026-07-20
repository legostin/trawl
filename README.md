# Trawl

A modern HTTP(S) proxy inspector for macOS — a Charles-style debugging proxy with
a clean UI. Capture and inspect traffic (including HTTPS via a locally-generated CA),
rewrite requests and responses with embedded JavaScript, mock endpoints, and organise
work into projects with their own rules, tracked hosts and environment variables.

Built with **Tauri 2** (Rust core + React/TypeScript UI).

## Features

- Live capture of HTTP/HTTPS traffic with a sequence list and a domain tree.
- HTTPS interception via an on-device CA, with guided setup for this Mac, Chrome,
  the iOS Simulator, the Android Emulator, and physical phones.
- Request detail with **Query / Form / Headers / Body** tabs, cURL export, and mocks.
- JavaScript rules (request/response/handler phases) with a Monaco editor,
  autocomplete, saved library functions, and synchronous `send()` / `sleep()`.
- Projects: scoped capture (only tracked hosts are recorded), per-project rules,
  and read/write environment variables usable from scripts.

## Development

Prerequisites: Node 20+, pnpm 9+, Rust (stable), and the Tauri prerequisites for macOS.

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

## Building & releasing (signed + notarized DMG)

Releases are produced by GitHub Actions (`.github/workflows/release.yml`) on a version tag:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The workflow builds a universal (`aarch64` + `x86_64`) macOS bundle, signs it with a
Developer ID Application certificate, notarizes it with Apple, signs the auto-update
artifacts, and **publishes** a GitHub Release with the DMG and the `latest.json` update feed.

### Required repository secrets

Add these under **Settings → Secrets and variables → Actions**. Signing needs a paid
Apple Developer account.

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

Without the Apple secrets the build still runs but the DMG is unsigned; macOS Gatekeeper
will warn users until signing is configured. Without the `TAURI_SIGNING_*` secrets the
updater feed is not signed and auto-update won't work.

## Auto-update

The app checks for updates on launch and shows an **Update to vX.Y.Z** button in the top
bar when a newer release is available; clicking it downloads, installs and relaunches.
There's also a manual **Check for updates** control. Updates use the Tauri updater plugin;
the update feed is served from the GitHub Release's `latest.json`.

Updates are cryptographically verified with a **minisign** key that is separate from Apple
code signing. Generate it once:

```sh
pnpm tauri signer generate -w ~/.tauri/trawl-updater.key
```

Put the printed **public key** into `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`),
and add the private key + its password as the `TAURI_SIGNING_*` secrets above.

### Cutting an update

An update is only offered when the released version is greater than the installed one:

```sh
# bump "version" in tauri.conf.json and package.json (e.g. 0.1.0 -> 0.1.1)
git tag v0.1.1
git push origin v0.1.1
```

The workflow publishes the release with a signed `latest.json`; installed copies pick it up
on their next launch. Only **published** (non-draft, non-prerelease) releases are seen by
the updater, which is why the workflow publishes directly.
