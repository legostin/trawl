import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { cn } from "@/lib/utils";

export const ResizableGroup = PanelGroup;
export const ResizablePanel = Panel;

export function ResizableHandle({ className }: { className?: string }) {
  return (
    <PanelResizeHandle
      className={cn(
        "relative w-px shrink-0 bg-border outline-none transition-colors",
        "before:absolute before:inset-y-0 before:-left-1 before:-right-1 before:content-['']",
        "data-[resize-handle-state=hover]:bg-primary data-[resize-handle-state=drag]:bg-primary",
        className,
      )}
    />
  );
}
