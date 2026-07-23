import { SecretsSection } from "./SecretsSection";
import { GitHostsSection } from "./GitHostsSection";
import { McpSection } from "./McpSection";

/** App configuration (not part of the connection guide): secrets, git host
 *  tokens, and the MCP server. Each section owns its own persistence; the
 *  panel only lays them out with a consistent vertical rhythm. */
export function SettingsPanel() {
  return (
    <div className="mx-auto h-full max-w-2xl overflow-auto p-6">
      <h2 className="mb-1 text-lg font-semibold">Settings</h2>
      <p className="mb-6 text-sm text-muted-foreground">
        App-wide configuration: secrets, git host tokens, and the MCP server for AI agents.
      </p>

      <div className="space-y-8">
        <SecretsSection />
        <GitHostsSection />
        <McpSection />
      </div>
    </div>
  );
}
