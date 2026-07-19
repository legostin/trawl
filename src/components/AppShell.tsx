import { useEffect } from "react";
import { useFlows } from "../store";
import { TopBar } from "./TopBar";
import { StatusBar } from "./StatusBar";
import { SetupPanel } from "./SetupPanel";
import { FilterBar } from "./FilterBar";
import { TrafficList } from "./TrafficList";
import { FlowDetail } from "./FlowDetail";
import { ResizableGroup, ResizablePanel, ResizableHandle } from "./ui/resizable";

export function AppShell() {
  const init = useFlows((s) => s.init);
  const view = useFlows((s) => s.view);

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    init().then((c) => (cleanup = c));
    return () => cleanup?.();
  }, [init]);

  return (
    <div className="flex h-full flex-col bg-background text-foreground">
      <TopBar />

      <main className="min-h-0 flex-1">
        {view === "setup" ? (
          <SetupPanel />
        ) : (
          <ResizableGroup direction="horizontal" className="h-full">
            <ResizablePanel defaultSize={45} minSize={25} className="flex min-h-0 flex-col">
              <FilterBar />
              <div className="min-h-0 flex-1">
                <TrafficList />
              </div>
            </ResizablePanel>
            <ResizableHandle />
            <ResizablePanel minSize={30} className="min-h-0">
              <FlowDetail />
            </ResizablePanel>
          </ResizableGroup>
        )}
      </main>

      <StatusBar />
    </div>
  );
}
