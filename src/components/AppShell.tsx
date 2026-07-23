import { useEffect, type ReactNode } from "react";
import { useFlows } from "../store";
import { useProjects } from "../projects";
import { useUpdater } from "../updater";
import { useLayout } from "../layout";
import { usePlugins } from "../plugins";
import { bootstrapPlugins } from "../plugins/bootstrap";
import { useKeyboardShortcuts } from "../hooks/useKeyboardShortcuts";
import { Sidebar } from "./Sidebar";
import { PluginsPanel } from "./PluginsPanel";
import { PluginMode } from "./PluginMode";
import { TopBar } from "./TopBar";
import { StatusBar } from "./StatusBar";
import { SetupPanel } from "./SetupPanel";
import { SettingsPanel } from "./SettingsPanel";
import { RulesView } from "./RulesView";
import { BreakpointsView } from "./BreakpointsView";
import { FilterBar } from "./FilterBar";
import { ListPanel } from "./ListPanel";
import { FlowDetail } from "./FlowDetail";
import { ProjectEditor } from "./ProjectEditor";
import { VariablesPanel } from "./VariablesPanel";
import { Toast } from "./Toast";
import { ResizableGroup, ResizablePanel, ResizableHandle } from "./ui/resizable";

export function AppShell() {
  const init = useFlows((s) => s.init);
  const view = useFlows((s) => s.view);
  const detailCollapsed = useFlows((s) => s.detailCollapsed);
  const loadProjects = useProjects((s) => s.load);
  const mode = useLayout((s) => s.mode);
  const pluginModes = usePlugins((s) => s.modes);
  useKeyboardShortcuts();

  const builtin =
    mode === "traffic" || mode === "setup" || mode === "settings" || mode === "plugins";
  const isUnknownMode = !builtin && !pluginModes.some((m) => m.id === mode);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    init().then((c) => (cleanup = c));
    void loadProjects();
    // Silent update check on launch (no-op in dev / when offline).
    void useUpdater.getState().check(true);
    // Install the plugin host API and load enabled plugins.
    void bootstrapPlugins();
    return () => cleanup?.();
  }, [init, loadProjects]);

  return (
    <div className="flex h-full bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar />

        <main className="min-h-0 flex-1">
          {/* Every panel stays mounted; only the active one is shown. Switching
              modes/views therefore preserves each panel's state (open tabs, scroll,
              in-progress edits, plugin editor contents). */}
          <Pane show={mode === "traffic" && view === "traffic"}>
            <ResizableGroup direction="horizontal" className="h-full">
              <ResizablePanel
                id="list"
                order={1}
                defaultSize={45}
                minSize={25}
                className="flex min-h-0 flex-col"
              >
                <FilterBar />
                <ListPanel />
              </ResizablePanel>
              {!detailCollapsed && (
                <>
                  <ResizableHandle />
                  <ResizablePanel id="detail" order={2} minSize={30} className="min-h-0">
                    <FlowDetail />
                  </ResizablePanel>
                </>
              )}
            </ResizableGroup>
          </Pane>

          <Pane show={mode === "traffic" && view === "rules"}>
            <RulesView />
          </Pane>

          <Pane show={mode === "traffic" && view === "breakpoints"}>
            <BreakpointsView />
          </Pane>

          <Pane show={mode === "setup"}>
            <SetupPanel />
          </Pane>

          <Pane show={mode === "settings"}>
            <SettingsPanel />
          </Pane>

          <Pane show={mode === "plugins"}>
            <PluginsPanel />
          </Pane>

          {pluginModes.map((m) => (
            <Pane key={m.id} show={mode === m.id}>
              <PluginMode modeId={m.id} />
            </Pane>
          ))}

          {isUnknownMode && (
            <Pane show>
              <PluginMode modeId={mode} />
            </Pane>
          )}
        </main>

        <StatusBar />
      </div>

      <ProjectEditor />
      <VariablesPanel />
      <Toast />
    </div>
  );
}

/** A full-size panel that stays mounted; only shown when `show` is true. */
function Pane({ show, children }: { show: boolean; children: ReactNode }) {
  return (
    <div className="h-full" style={{ display: show ? "block" : "none" }}>
      {children}
    </div>
  );
}
