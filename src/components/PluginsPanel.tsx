import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { requestKeychainConsent } from "@/keychainConsent";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AlertTriangle, ArrowUpCircle, KeyRound, Package, Power, PowerOff, RefreshCw, Trash2 } from "lucide-react";
import { usePlugins, fetchCatalog, catalogInstallRepo, type CatalogEntry } from "@/plugins";
import { loadEnabledPlugins, loadPlugin } from "@/plugins/loader";
import { useToast } from "../toast";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

export function PluginsPanel() {
  const installed = usePlugins((s) => s.installed);
  const updates = usePlugins((s) => s.updates);
  const blockedUpdates = usePlugins((s) => s.blockedUpdates);
  const load = usePlugins((s) => s.load);
  const install = usePlugins((s) => s.install);
  const remove = usePlugins((s) => s.remove);
  const setEnabled = usePlugins((s) => s.setEnabled);
  const unregisterMode = usePlugins((s) => s.unregisterMode);
  const checkUpdates = usePlugins((s) => s.checkUpdates);
  const update = usePlugins((s) => s.update);
  const show = useToast((s) => s.show);
  const [repo, setRepo] = useState("");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);
  const [catalogError, setCatalogError] = useState<string | null>(null);

  /** First path segment of the input that looks like a non-github.com domain. */
  const ghost = (() => {
    const s = repo.trim().replace(/^https?:\/\//, "").replace(/^www\./, "");
    const first = s.split("/")[0] ?? "";
    return first.includes(".") && first !== "github.com" ? first : null;
  })();

  useEffect(() => {
    void load().then(() => usePlugins.getState().checkUpdates());
    void fetchCatalog()
      .then(setCatalog)
      .catch((e) => setCatalogError(e instanceof Error ? e.message : String(e)));
  }, [load]);

  const installCatalog = async (e: CatalogEntry) => {
    setBusy(true);
    try {
      await install(catalogInstallRepo(e));
      await loadEnabledPlugins();
      show(`Installed ${e.name}`);
    } catch (err) {
      show(`Install failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const runCheck = async () => {
    setChecking(true);
    try {
      await checkUpdates();
      const n = Object.keys(usePlugins.getState().updates).length;
      const b = Object.keys(usePlugins.getState().blockedUpdates).length;
      show(
        n || b
          ? [
              n ? `${n} update${n > 1 ? "s" : ""} available` : null,
              b ? `${b} need${b > 1 ? "" : "s"} a newer app` : null,
            ]
              .filter(Boolean)
              .join(", ")
          : "All plugins up to date",
      );
    } finally {
      setChecking(false);
    }
  };

  const applyUpdate = async (id: string, name: string) => {
    try {
      await update(id);
      await loadPlugin(id); // hot-swap the running plugin — no restart
      show(`Updated ${name}`);
    } catch (e) {
      show(`Update failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const toggleEnabled = async (id: string, name: string, wasEnabled: boolean) => {
    await setEnabled(id, !wasEnabled);
    if (wasEnabled) {
      unregisterMode(id); // hot-disable: drop its mode from the UI
      show(`Disabled ${name}`);
    } else {
      await loadPlugin(id); // hot-enable: load and register now
      show(`Enabled ${name}`);
    }
  };

  const add = async () => {
    const value = repo.trim();
    if (!value || busy) return;
    setBusy(true);
    try {
      if (ghost && token.trim()) {
        if (!(await requestKeychainConsent())) {
          setBusy(false);
          return;
        }
        await invoke("git_host_token_set", { host: ghost, token: token.trim() });
        setToken("");
      }
      const prev = new Set(usePlugins.getState().installed.map((p) => p.id));
      await install(value);
      // Manifest dependencies may have (re)installed other plugins — reload
      // every enabled bundle so all of them pick up their fresh code.
      await loadEnabledPlugins();
      const added = usePlugins.getState().installed.filter((p) => !prev.has(p.id));
      show(
        added.length > 1
          ? `Installed ${added.map((p) => p.name).join(", ")}`
          : `Installed ${value}`,
      );
      setRepo("");
    } catch (e) {
      show(`Install failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mx-auto h-full max-w-2xl overflow-auto p-6">
      <div className="mb-4 flex items-start justify-between gap-2">
        <div>
          <h2 className="text-lg font-semibold">Plugins</h2>
          <p className="text-sm text-muted-foreground">
            Extend Trawl with modes and tools from GitHub repositories.
          </p>
        </div>
        {installed.length > 0 && (
          <Button variant="outline" size="sm" onClick={() => void runCheck()} disabled={checking}>
            <RefreshCw className={checking ? "animate-spin" : undefined} />
            Check for updates
          </Button>
        )}
      </div>

      <div className="mb-4 flex items-start gap-2 rounded-lg border border-http-amber/40 bg-http-amber/10 px-3 py-2 text-xs">
        <AlertTriangle className="mt-0.5 size-4 shrink-0 text-http-amber" />
        <span>
          Plugins run with full access to the app and your captured traffic. Only install
          plugins from sources you trust; pin a tag or commit when you can.
        </span>
      </div>

      <div className={ghost ? "mb-2 flex gap-2" : "mb-6 flex gap-2"}>
        <Input
          value={repo}
          onChange={(e) => setRepo(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void add()}
          placeholder="owner/repo  ·  owner/repo@v1.0.0  ·  ghe.host/owner/repo"
        />
        <Button onClick={() => void add()} disabled={busy || !repo.trim()}>
          <Package />
          {busy ? "Installing…" : "Add"}
        </Button>
      </div>
      {ghost && (
        <div className="mb-6 flex flex-col gap-1.5">
          <div className="flex items-center gap-2">
            <KeyRound className="size-4 shrink-0 text-muted-foreground" />
            <Input
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void add()}
              placeholder={`${ghost} access token — optional, stored locally; needed for private repos`}
            />
          </div>
          <p className="pl-6 text-xs text-muted-foreground">
            How to get one: open{" "}
            <button
              type="button"
              className="cursor-pointer break-all font-mono text-primary underline underline-offset-2 hover:opacity-80"
              onClick={() =>
                void openUrl(`https://${ghost}/settings/tokens/new?scopes=repo&description=Trawl`)
              }
            >
              https://{ghost}/settings/tokens/new
            </button>{" "}
            (Settings → Developer settings → Personal access tokens → Tokens (classic) →
            Generate new token), keep only the <code>repo</code> scope, generate and copy the{" "}
            <code>ghp_…</code> value — GitHub shows it once.
          </p>
        </div>
      )}

      {installed.length === 0 ? (
        <p className="text-sm text-muted-foreground">No plugins installed yet.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {installed.map((p) => (
            <li
              key={p.id}
              className="flex items-center gap-3 rounded-lg border border-border bg-card px-3 py-2.5"
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate text-sm font-medium">{p.name}</span>
                  {p.version && (
                    <span className="rounded bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                      v{p.version}
                    </span>
                  )}
                  {!p.enabled && (
                    <span className="text-[10px] uppercase tracking-wide text-muted-foreground">
                      disabled
                    </span>
                  )}
                </div>
                <div className="truncate font-mono text-xs text-muted-foreground">
                  {p.host && p.host !== "github.com" ? `${p.host}/` : ""}
                  {p.repo}@{p.ref}
                </div>
                {p.description && (
                  <div className="mt-0.5 truncate text-xs text-muted-foreground">{p.description}</div>
                )}
              </div>
              {updates[p.id] && (
                <Button
                  variant="default"
                  size="sm"
                  title={`Update to v${updates[p.id]}`}
                  onClick={() => void applyUpdate(p.id, p.name)}
                >
                  <ArrowUpCircle />
                  Update to v{updates[p.id]}
                </Button>
              )}
              {blockedUpdates[p.id] && (
                <span
                  className="flex shrink-0 items-center gap-1 text-xs text-http-amber"
                  title={`v${blockedUpdates[p.id].version} requires plugin API ${blockedUpdates[p.id].apiVersion} — this app is older. Update the app to get it.`}
                >
                  <AlertTriangle className="size-3.5" />
                  v{blockedUpdates[p.id].version} needs app update
                </span>
              )}
              <Button
                variant="ghost"
                size="iconSm"
                title={p.enabled ? "Disable" : "Enable"}
                onClick={() => void toggleEnabled(p.id, p.name, p.enabled)}
              >
                {p.enabled ? <Power className="text-http-green" /> : <PowerOff />}
              </Button>
              <Button
                variant="ghost"
                size="iconSm"
                title="Remove plugin"
                onClick={() => void remove(p.id)}
              >
                <Trash2 />
              </Button>
            </li>
          ))}
        </ul>
      )}

      <div className="mt-8">
        <h3 className="mb-1 text-sm font-semibold">Catalog</h3>
        <p className="mb-3 text-xs text-muted-foreground">Public plugins you can install with one click.</p>
        {catalogError ? (
          <p className="text-xs text-muted-foreground">Couldn’t load the catalog: {catalogError}</p>
        ) : catalog.length === 0 ? (
          <p className="text-xs text-muted-foreground">Loading…</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {catalog.map((e) => {
              const isInstalled = installed.some((p) => p.id === e.id || p.repo === e.repo);
              return (
                <li
                  key={e.id}
                  className="flex items-center gap-3 rounded-lg border border-border bg-card px-3 py-2.5"
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="truncate text-sm font-medium">{e.name}</span>
                      {e.author && <span className="text-[10px] text-muted-foreground">by {e.author}</span>}
                      {e.tags?.map((t) => (
                        <span key={t} className="rounded bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                          {t}
                        </span>
                      ))}
                    </div>
                    <div className="truncate font-mono text-xs text-muted-foreground">{catalogInstallRepo(e)}</div>
                    {e.description && (
                      <div className="mt-0.5 text-xs text-muted-foreground">{e.description}</div>
                    )}
                  </div>
                  {isInstalled ? (
                    <span className="shrink-0 text-xs text-http-green">Installed</span>
                  ) : (
                    <Button size="sm" disabled={busy} onClick={() => void installCatalog(e)}>
                      <Package />
                      Install
                    </Button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}
