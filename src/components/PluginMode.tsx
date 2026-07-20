import { Component, type ReactNode } from "react";
import { Puzzle } from "lucide-react";
import { usePlugins } from "@/plugins";
import { EmptyState } from "./EmptyState";

class PluginErrorBoundary extends Component<{ children: ReactNode }, { error: string | null }> {
  state = { error: null as string | null };
  static getDerivedStateFromError(err: unknown) {
    return { error: err instanceof Error ? err.message : String(err) };
  }
  render() {
    if (this.state.error) {
      return (
        <EmptyState
          icon={<Puzzle className="size-8 text-http-red" />}
          title="Plugin crashed"
          hint={this.state.error}
        />
      );
    }
    return this.props.children;
  }
}

/** Renders the panel of the active plugin mode, isolated by an error boundary. */
export function PluginMode({ modeId }: { modeId: string }) {
  const mode = usePlugins((s) => s.modes.find((m) => m.id === modeId));
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
    <PluginErrorBoundary>
      <Panel />
    </PluginErrorBoundary>
  );
}
