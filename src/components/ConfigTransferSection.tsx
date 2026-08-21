import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "./ui/button";
import { useProjects } from "../projects";
import { useRules } from "../rules";

interface ImportSummary {
  projects: number;
  rules: number;
  breakpoints: number;
  snippets: number;
  renamed: string[];
}

const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** Move a setup to another machine, or hand it to a colleague.
 *
 *  Variable *values* are never in the file — only their names, so the receiver
 *  sees what to fill in without anyone's token travelling by accident. */
export function ConfigTransferSection() {
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const reloadProjects = useProjects((s) => s.load);
  const reloadRules = useRules((s) => s.load);

  const exportAll = async () => {
    setError(null);
    setNote(null);
    const path = await saveDialog({ defaultPath: "trawl-config.trawl.json" });
    if (!path) return;
    try {
      await invoke("export_config", { path, projectId: null });
      setNote(`Written to ${path}`);
    } catch (e) {
      setError(String(e));
    }
  };

  const importFile = async () => {
    setError(null);
    setNote(null);
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "Trawl config", extensions: ["json"] }],
    });
    if (typeof picked !== "string") return;
    try {
      const s = await invoke<ImportSummary>("import_config", { path: picked });
      const parts = [
        count(s.projects, "project", "projects"),
        count(s.rules, "rule", "rules"),
        count(s.breakpoints, "breakpoint", "breakpoints"),
        count(s.snippets, "snippet", "snippets"),
      ];
      const renamed = s.renamed.length
        ? ` Names were taken, so these arrived as: ${s.renamed.join(", ")}.`
        : "";
      setNote(`Imported ${parts.join(", ")}.${renamed}`);
      await Promise.all([reloadProjects(), reloadRules()]);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <section>
      <h3 className="text-sm font-medium">Configuration</h3>
      <p className="mt-0.5 text-sm text-muted-foreground">
        Projects, rules, breakpoints and snippets in one file. Variable values stay on this
        machine — only their names travel, so a token is never shared by accident. Importing
        adds; it never overwrites or removes what is already here.
      </p>
      <div className="mt-3 flex items-center gap-2">
        <Button variant="outline" size="sm" onClick={() => void exportAll()}>
          Export everything…
        </Button>
        <Button variant="outline" size="sm" onClick={() => void importFile()}>
          Import…
        </Button>
      </div>
      {note && <p className="mt-2 text-xs text-muted-foreground">{note}</p>}
      {error && <p className="mt-2 text-xs text-http-red">{error}</p>}
    </section>
  );
}
