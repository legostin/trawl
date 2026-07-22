import { useEffect, useState } from "react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { deleteSecret, listSecrets, setSecret } from "@/secrets";

/** App-wide named secrets (stored in the macOS Keychain). */
export function SecretsSection() {
  const [names, setNames] = useState<string[]>([]);
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);

  const refresh = () =>
    listSecrets()
      .then((n) => {
        setNames(n);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  useEffect(() => {
    void refresh();
  }, []);

  const add = async () => {
    const n = name.trim();
    if (!n || !value.trim()) return;
    try {
      await setSecret(n, value);
      setName("");
      setValue("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section>
      <h3 className="mb-1 text-base font-semibold">Secrets</h3>
      <p className="mb-3 text-sm text-muted-foreground">
        App-wide named secrets, stored in the macOS Keychain. Available to rule scripts via{" "}
        <code>secret('NAME')</code> and to plugins. Re-add a name to change its value.
      </p>
      {error && <p className="mb-2 text-sm text-red-500">{error}</p>}
      <ul className="mb-3 space-y-1">
        {names.map((n) => (
          <li
            key={n}
            className="flex items-center justify-between rounded border border-border px-3 py-1.5 text-sm"
          >
            <span className="font-mono">{n}</span>
            <span className="flex items-center gap-2">
              <span className="text-muted-foreground">••••••••</span>
              <Button variant="ghost" size="sm" onClick={() => void deleteSecret(n).then(refresh).catch((e) => setError(String(e)))}>
                Delete
              </Button>
            </span>
          </li>
        ))}
        {names.length === 0 && <li className="text-sm text-muted-foreground">No secrets yet.</li>}
      </ul>
      <div className="flex gap-2">
        <Input
          placeholder="NAME"
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="w-48 font-mono"
        />
        <Input
          placeholder="value"
          type="password"
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
        <Button onClick={() => void add()}>Add</Button>
      </div>
    </section>
  );
}
