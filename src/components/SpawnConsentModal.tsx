import { TerminalSquare } from "lucide-react";
import { useSpawnConsent } from "@/plugins/spawnConsent";
import { Button } from "./ui/button";

/** One-time explainer shown before a plugin starts its first child process,
 *  naming the plugin and the exact command. Mounted once. */
export function SpawnConsentModal() {
  const open = useSpawnConsent((s) => s.open);
  const pluginId = useSpawnConsent((s) => s.pluginId);
  const command = useSpawnConsent((s) => s.command);
  const confirm = useSpawnConsent((s) => s.confirm);
  const cancel = useSpawnConsent((s) => s.cancel);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-6" onClick={cancel}>
      <div
        className="w-[520px] rounded-lg border border-border bg-background p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="mb-3 flex items-center gap-2">
          <TerminalSquare className="size-4 text-primary" />
          <h2 className="text-base font-semibold">
            Plugin “{pluginId}” wants to run a program
          </h2>
        </div>
        <div className="space-y-2 text-sm text-muted-foreground">
          <p>It will run this command on your machine, with your permissions:</p>
          <pre className="overflow-x-auto rounded border border-border bg-muted/40 p-2 font-mono text-xs text-foreground">
            {command}
          </pre>
          <p>
            The process is stopped when the plugin is disabled and when Trawl quits. Trawl asks once per
            plugin — after this, “{pluginId}” can start programs without asking again.
          </p>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={cancel}>
            Cancel
          </Button>
          <Button size="sm" onClick={confirm}>
            Allow
          </Button>
        </div>
      </div>
    </div>
  );
}
