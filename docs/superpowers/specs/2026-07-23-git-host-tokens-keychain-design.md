# Git host tokens: Keychain storage + Settings UI

**Date:** 2026-07-23
**Status:** approved

## Problem

Plugin update checks 403 on machines without a stored GitHub token: anonymous
`api.github.com` calls share a 60 req/h per-IP quota. A raw-host fallback for
token-less github.com fetches is already merged, but there is still no way to
set a github.com token from the UI — the token input in PluginsPanel appears
only for enterprise hosts — and stored tokens live in plaintext
`git-hosts.json`.

## Decision

Store git-host tokens in the macOS Keychain and add a **Git hosts** section to
the Settings panel.

## Storage (backend)

- Keychain entry per host: account `git-host:<host>`, existing service
  `trawl`. Reuses `secrets::get/set/delete`-style `keyring::Entry` access but
  bypasses the `secrets.json` index — git-host tokens do not appear in the
  Secrets section and are not readable via `secret('NAME')` in rule scripts.
  Plugins keep access through the existing `gitHosts.token()` API.
- `git-hosts.json` changes format: a JSON **array of host names** (the
  Keychain cannot enumerate entries, so an index is required). It no longer
  contains tokens.
- Public function signatures are unchanged: `host_token(data_dir, host)`,
  `save_git_host_token(data_dir, host, token)` (empty token = delete). The
  Tauri commands `git_host_token_set/has/get` are untouched.
- New command `git_hosts_list() -> Vec<String>` returning configured host
  names, no values.

## Migration

On first read, if `git-hosts.json` parses as the old `{host: token}` map:
move each token into the Keychain, rewrite the file as a host-name array.
Silent; plaintext tokens leave the disk. A file that parses as an array is
already migrated; a missing/corrupt file yields an empty list.

## UI

New `GitHostsSection.tsx` in `SettingsPanel` between Secrets and MCP,
mirroring `SecretsSection`:

- List of configured hosts: `github.com  ••••••••  [Delete]`.
- Add row: host input (placeholder `github.com`; empty input defaults to
  `github.com`, leading `https://` and `www.` are stripped) + password token
  input + Add button.
- Section description explains that a token raises the GitHub API rate limit
  used by plugin install/update checks.
- The enterprise-host token field in PluginsPanel stays as-is; it writes
  through the same backend command.

## Testing

- **Rust:** the in-memory mock Keychain currently inside `secrets.rs` tests
  moves to a shared `#[cfg(test)]` module. Tests: token set/get/delete
  roundtrip through the new storage; migration of an old-format map file
  (tokens land in Keychain, file rewritten as array); `git_hosts_list`
  ordering/contents; empty-token deletion removes the host from the index.
- **Frontend:** unit test for host-input normalization (empty → github.com,
  `https://`/`www.` stripped). Section rendering logic mirrors the already
  shipped SecretsSection and is not separately tested.

## Out of scope

- No change to the raw-host fallback merged earlier.
- No PluginsPanel redesign.
- No token validation against the GitHub API.
