import { FolderCog, Moon, Play, Search, Square, Sun, Trash2 } from "lucide-react";
import { useFlows } from "../store";
import { useProjects } from "../projects";
import { useLayout } from "../layout";
import { useTheme } from "./ThemeProvider";
import { UpdateButton } from "./UpdateButton";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import { Segmented } from "./ui/segmented";
import type { View } from "../store";

export function TopBar() {
  const running = useFlows((s) => s.running);
  const proxyAddr = useFlows((s) => s.proxyAddr);
  const toggleProxy = useFlows((s) => s.toggleProxy);
  const view = useFlows((s) => s.view);
  const setView = useFlows((s) => s.setView);
  const query = useFlows((s) => s.filter.query);
  const setFilter = useFlows((s) => s.setFilter);
  const clearFlows = useFlows((s) => s.clearFlows);
  const projects = useProjects((s) => s.projects);
  const activeId = useProjects((s) => s.activeId);
  const setActive = useProjects((s) => s.setActive);
  const openEditor = useProjects((s) => s.openEditor);
  const mode = useLayout((s) => s.mode);
  const { theme, toggle } = useTheme();
  const isTraffic = mode === "traffic";

  return (
    <header className="flex h-11 items-center gap-3 border-b border-border bg-card px-3">
      <Button
        variant={running ? "destructive" : "default"}
        size="sm"
        onClick={() => void toggleProxy()}
      >
        {running ? <Square className="fill-current" /> : <Play className="fill-current" />}
        {running ? "Stop" : "Start"}
      </Button>

      {running && proxyAddr && (
        <span className="flex items-center gap-1.5 rounded-md bg-secondary px-2 py-1 text-xs text-muted-foreground">
          <span className="relative flex size-2">
            <span className="absolute inline-flex size-full animate-ping rounded-full bg-http-green opacity-75" />
            <span className="relative inline-flex size-2 rounded-full bg-http-green" />
          </span>
          {proxyAddr}
        </span>
      )}

      {isTraffic && (
        <div className="relative ml-2 max-w-xs flex-1">
          <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            data-search-input
            value={query}
            onChange={(e) => setFilter({ query: e.target.value })}
            placeholder="Search host / URL…"
            className="pl-7"
          />
        </div>
      )}

      <div className="ml-auto flex items-center gap-2">
        <div className="flex items-center gap-1" title="Active project">
          <Select
            value={activeId ?? ""}
            onChange={(e) => void setActive(e.target.value || null)}
          >
            <option value="">All domains</option>
            {projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </Select>
          <Button variant="ghost" size="iconSm" title="Projects" onClick={openEditor}>
            <FolderCog />
          </Button>
        </div>
        {isTraffic && (
          <>
            <Button variant="ghost" size="iconSm" title="Clear list" onClick={() => clearFlows()}>
              <Trash2 />
            </Button>
            <Segmented<View>
              value={view}
              onChange={setView}
              options={[
                { value: "traffic", label: "Traffic" },
                { value: "rules", label: "Rules" },
                { value: "setup", label: "Setup" },
              ]}
            />
          </>
        )}
        <UpdateButton />
        <Button variant="ghost" size="iconSm" title="Theme" onClick={toggle}>
          {theme === "dark" ? <Sun /> : <Moon />}
        </Button>
      </div>
    </header>
  );
}
