import { useEffect, useState } from "react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { deleteGitHostToken, listGitHosts, normalizeHost, setGitHostToken } from "@/gitHosts";

/** Access tokens for git hosts plugins are fetched from (macOS Keychain). */
export function GitHostsSection() {
  const [hosts, setHosts] = useState<string[]>([]);
  const [host, setHost] = useState("");
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = () =>
    listGitHosts()
      .then((h) => {
        setHosts(h);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  useEffect(() => {
    void refresh();
  }, []);

  const add = async () => {
    if (!token.trim()) return;
    try {
      await setGitHostToken(normalizeHost(host), token);
      setHost("");
      setToken("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section>
      <h3 className="mb-1 text-base font-semibold">Git hosts</h3>
      <p className="mb-3 text-sm text-muted-foreground">
        Tokens for installing and updating plugins, stored in the macOS Keychain. A{" "}
        <code>github.com</code> token raises the API rate limit that anonymous update checks
        can hit (HTTP 403). Re-add a host to change its token.
      </p>
      {error && <p className="mb-2 text-sm text-red-500">{error}</p>}
      <ul className="mb-3 space-y-1">
        {hosts.map((h) => (
          <li
            key={h}
            className="flex items-center justify-between rounded border border-border px-3 py-1.5 text-sm"
          >
            <span className="font-mono">{h}</span>
            <span className="flex items-center gap-2">
              <span className="text-muted-foreground">••••••••</span>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => void deleteGitHostToken(h).then(refresh).catch((e) => setError(String(e)))}
              >
                Delete
              </Button>
            </span>
          </li>
        ))}
        {hosts.length === 0 && <li className="text-sm text-muted-foreground">No tokens yet.</li>}
      </ul>
      <div className="flex gap-2">
        <Input
          placeholder="github.com"
          value={host}
          onChange={(e) => setHost(e.target.value)}
          className="w-48 font-mono"
        />
        <Input
          placeholder="token"
          type="password"
          value={token}
          onChange={(e) => setToken(e.target.value)}
        />
        <Button onClick={() => void add()}>Add</Button>
      </div>
    </section>
  );
}
