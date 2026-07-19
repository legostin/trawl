import { useEffect } from "react";
import { useFlows } from "../store";
import { useProjects } from "../projects";
import { useKeyboardShortcuts } from "../hooks/useKeyboardShortcuts";
import { TopBar } from "./TopBar";
import { StatusBar } from "./StatusBar";
import { SetupPanel } from "./SetupPanel";
import { RulesView } from "./RulesView";
import { FilterBar } from "./FilterBar";
import { ListPanel } from "./ListPanel";
import { FlowDetail } from "./FlowDetail";
import { ProjectEditor } from "./ProjectEditor";
import { Toast } from "./Toast";
import { ResizableGroup, ResizablePanel, ResizableHandle } from "./ui/resizable";

export function AppShell() {
  const init = useFlows((s) => s.init);
  const view = useFlows((s) => s.view);
  const detailCollapsed = useFlows((s) => s.detailCollapsed);
  const loadProjects = useProjects((s) => s.load);
  useKeyboardShortcuts();

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    init().then((c) => (cleanup = c));
    void loadProjects();
    return () => cleanup?.();
  }, [init, loadProjects]);

  return (
    <div className="flex h-full flex-col bg-background text-foreground">
      <TopBar />

      <main className="min-h-0 flex-1">
        {view === "setup" ? (
          <SetupPanel />
        ) : view === "rules" ? (
          <RulesView />
        ) : (
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
        )}
      </main>

      <StatusBar />
      <ProjectEditor />
      <Toast />
    </div>
  );
}
