import { useEffect, useState } from "react";
import { AlertTriangle, ArrowUpCircle, Package, Power, PowerOff, RefreshCw, Trash2 } from "lucide-react";
import { usePlugins } from "@/plugins";
import { loadPlugin } from "@/plugins/loader";
import { useToast } from "../toast";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

export function PluginsPanel() {
  const installed = usePlugins((s) => s.installed);
  const updates = usePlugins((s) => s.updates);
  const load = usePlugins((s) => s.load);
  const install = usePlugins((s) => s.install);
  const remove = usePlugins((s) => s.remove);
  const setEnabled = usePlugins((s) => s.setEnabled);
  const checkUpdates = usePlugins((s) => s.checkUpdates);
  const update = usePlugins((s) => s.update);
  const show = useToast((s) => s.show);
  const [repo, setRepo] = useState("");
  const [busy, setBusy] = useState(false);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    void load().then(() => usePlugins.getState().checkUpdates());
  }, [load]);

  const runCheck = async () => {
    setChecking(true);
    try {
      await checkUpdates();
      const n = Object.keys(usePlugins.getState().updates).length;
      show(n ? `${n} update${n > 1 ? "s" : ""} available` : "All plugins up to date");
    } finally {
      setChecking(false);
    }
  };

  const applyUpdate = async (id: string, name: string) => {
    try {
      await update(id);
      show(`Updated ${name} — restart to apply`);
    } catch (e) {
      show(`Update failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const add = async () => {
    const value = repo.trim();
    if (!value || busy) return;
    setBusy(true);
    try {
      await install(value);
      const list = usePlugins.getState().installed;
      const id = list[list.length - 1]?.id;
      if (id) await loadPlugin(id);
      show(`Installed ${value}`);
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

      <div className="mb-6 flex gap-2">
        <Input
          value={repo}
          onChange={(e) => setRepo(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void add()}
          placeholder="owner/repo  ·  owner/repo@v1.0.0  ·  GitHub URL"
        />
        <Button onClick={() => void add()} disabled={busy || !repo.trim()}>
          <Package />
          {busy ? "Installing…" : "Add"}
        </Button>
      </div>

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
                  title={`Update to v${updates[p.id]} (applies on restart)`}
                  onClick={() => void applyUpdate(p.id, p.name)}
                >
                  <ArrowUpCircle />
                  Update to v{updates[p.id]}
                </Button>
              )}
              <Button
                variant="ghost"
                size="iconSm"
                title={p.enabled ? "Disable (takes effect on restart)" : "Enable (takes effect on restart)"}
                onClick={() => void setEnabled(p.id, !p.enabled)}
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
    </div>
  );
}
