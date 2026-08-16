import { Puzzle } from "lucide-react";
import { usePlugins } from "@/plugins";
import { EmptyState } from "./EmptyState";
import { PluginErrorBoundary } from "./PluginErrorBoundary";

/** Renders the panel of the active plugin mode, isolated by an error boundary. */
export function PluginMode({ modeId }: { modeId: string }) {
  const mode = usePlugins((s) => s.modes.find((m) => m.id === modeId));
  // Panes stay mounted, so the boundary would otherwise never unmount and a
  // fixed plugin would keep showing its old crash until the app restarted.
  const reloads = usePlugins((s) => s.reloads);

  if (!mode) {
    return (
      <EmptyState
        icon={<Puzzle className="size-8" />}
        title="Loading plugin…"
        hint="If this persists, the plugin failed to load — check its install."
      />
    );
  }
  const Panel = mode.component;
  return (
    <PluginErrorBoundary
      key={reloads}
      fallback={(error) => (
        <EmptyState
          icon={<Puzzle className="size-8 text-http-red" />}
          title="Plugin crashed"
          hint={error}
        />
      )}
    >
      <Panel />
    </PluginErrorBoundary>
  );
}
