import { Component, type ReactNode } from "react";

/**
 * Contains a throwing plugin component.
 *
 * React error boundaries only catch render, so this is not a sandbox — an
 * exception from an event handler or a timer still escapes. What it does stop
 * is the case that takes the whole window with it: a plugin's icon or panel
 * throwing during render, which without a boundary unmounts the entire tree and
 * leaves no UI to remove the plugin with.
 *
 * Give it a `key` that changes when plugins reload, or a plugin that has been
 * fixed keeps showing its old crash until the app restarts.
 */
export class PluginErrorBoundary extends Component<
  { children: ReactNode; fallback?: (error: string) => ReactNode },
  { error: string | null }
> {
  state = { error: null as string | null };

  static getDerivedStateFromError(err: unknown) {
    return { error: err instanceof Error ? err.message : String(err) };
  }

  render() {
    if (this.state.error) {
      return this.props.fallback ? this.props.fallback(this.state.error) : null;
    }
    return this.props.children;
  }
}
