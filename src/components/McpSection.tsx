import { useEffect, useState } from "react";
import { Plug, RefreshCw } from "lucide-react";
import {
  getMcpConfig,
  mcpAddCommand,
  mcpServerStatus,
  regenMcpToken,
  setMcpConfig,
  type McpConfig,
  type McpServerStatus,
} from "../mcp";
import { CopyableCommand } from "./CopyableCommand";
import { PulseDot } from "./PulseDot";
import { Button } from "./ui/button";

export function McpSection() {
  const [cfg, setCfg] = useState<McpConfig | null>(null);
  const [status, setStatus] = useState<McpServerStatus | null>(null);
  const [port, setPort] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = async () => {
    const c = await getMcpConfig();
    setCfg(c);
    setPort(String(c.port));
    setStatus(await mcpServerStatus());
  };

  useEffect(() => {
    void refresh();
  }, []);

  if (!cfg) return null;

  const apply = async (enabled: boolean, p: number) => {
    setBusy(true);
    try {
      setCfg(await setMcpConfig(enabled, p));
      setStatus(await mcpServerStatus());
    } finally {
      setBusy(false);
    }
  };

  // Server should be up but isn't (or errored) — mirrors PulseDot's usage
  // elsewhere in the app as an "needs attention" marker, not a generic status light.
  const needsAttention = cfg.enabled && (!status?.running || Boolean(status?.error));

  return (
    <div className="mb-3 rounded-lg border border-border bg-card p-4">
      <div className="mb-1.5 flex items-center gap-2">
        <span className="flex size-6 items-center justify-center rounded-full bg-primary text-primary-foreground">
          <Plug className="size-3.5" />
        </span>
        <h3 className="text-sm font-semibold">MCP server</h3>
        {needsAttention && <PulseDot />}
        <label className="ml-auto flex items-center gap-1.5 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={cfg.enabled}
            disabled={busy}
            onChange={(e) => void apply(e.target.checked, cfg.port)}
          />
          Enabled
        </label>
      </div>
      <div className="pl-8 text-sm text-muted-foreground [&_code]:rounded [&_code]:bg-secondary [&_code]:px-1 [&_code]:font-mono [&_code]:text-foreground">
        <p>
          Connect an AI agent (Claude Code, Cursor…) to inspect traffic and manage rules over MCP.
        </p>
        {status?.error && <p className="mt-1.5 text-http-red">{status.error}</p>}
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <span>Port</span>
          <input
            className="w-20 rounded border border-border bg-background px-1.5 py-0.5 font-mono text-foreground"
            value={port}
            disabled={busy}
            onChange={(e) => setPort(e.target.value)}
            onBlur={() => {
              const p = Number(port);
              if (Number.isInteger(p) && p > 0 && p < 65536 && p !== cfg.port) {
                void apply(cfg.enabled, p);
              } else {
                setPort(String(cfg.port));
              }
            }}
          />
          <Button
            size="sm"
            variant="outline"
            disabled={busy}
            onClick={async () => {
              setBusy(true);
              try {
                setCfg(await regenMcpToken());
                setStatus(await mcpServerStatus());
              } finally {
                setBusy(false);
              }
            }}
          >
            <RefreshCw className="size-3" />
            Regenerate token
          </Button>
        </div>
        <CopyableCommand cmd={mcpAddCommand(cfg.port, cfg.token)} />
      </div>
    </div>
  );
}
