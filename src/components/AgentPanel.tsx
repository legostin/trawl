import { useEffect, useRef, useState } from "react";
import { AlertTriangle, RefreshCw, Sparkles } from "lucide-react";
import { useAgent } from "../agent/agentStore";
import { describeScreen } from "../agent/screenContext";
import type { ChatItem } from "../agent/chatItems";
import { useLayout } from "../layout";
import { useFlows } from "../store";
import { useProjects } from "../projects";
import { Button } from "./ui/button";
import { AgentMarkdown } from "./AgentMarkdown";
import { CopyableCommand } from "./CopyableCommand";

/** Where the agent is looking, and how much it may do there. Shown in the
 *  header so "it was reading the wrong repository" is never a late discovery. */
function CodeFolderNote() {
  const active = useProjects((s) => s.projects.find((p) => p.id === s.activeId) ?? null);
  const dir = active?.codeDir?.trim();
  if (!dir) return null;
  const name = dir.split("/").filter(Boolean).pop() ?? dir;
  return (
    <span className="truncate text-xs font-normal text-muted-foreground" title={dir}>
      {name} · {active?.codeWrite ? "can edit" : "read-only"}
    </span>
  );
}

export function AgentPanel() {
  const items = useAgent((s) => s.items);
  const running = useAgent((s) => s.running);
  const trawlConnected = useAgent((s) => s.trawlConnected);
  const init = useAgent((s) => s.init);
  const send = useAgent((s) => s.send);
  const interrupt = useAgent((s) => s.interrupt);
  const reset = useAgent((s) => s.reset);
  const harnesses = useAgent((s) => s.harnesses);
  const checkHarnesses = useAgent((s) => s.checkHarnesses);
  // Null means "not looked yet" — only an actual empty result is a verdict.
  const noHarness = harnesses !== null && harnesses.every((h) => h.path === null);

  const [draft, setDraft] = useState("");
  const mode = useLayout((s) => s.mode);
  const view = useFlows((s) => s.view);
  const flows = useFlows((s) => s.flows);
  const selectedId = useFlows((s) => s.selectedId);
  const projects = useProjects((s) => s.projects);
  const activeId = useProjects((s) => s.activeId);
  const activeProject = projects.find((p) => p.id === activeId) ?? null;
  const bottom = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void init();
    void checkHarnesses();
  }, [init, checkHarnesses]);

  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [items]);

  const submit = () => {
    const text = draft.trim();
    if (!text || running) return;
    const flow = flows.find((f) => f.id === selectedId) ?? null;
    const context = describeScreen({
      mode,
      view,
      flowCount: flows.length,
      project: activeProject ? { id: activeProject.id, name: activeProject.name } : null,
      // The oldest flow still in memory is where this session began; anything
      // older lives in history and is not what the user is looking at.
      sessionStartedMs: flows.length ? Math.min(...flows.map((f) => f.timestamp)) : null,
      selected: flow
        ? {
            id: flow.id,
            method: flow.method,
            url: `${flow.url.scheme}://${flow.url.host}${flow.url.path}`,
            // Flow has no `status` of its own — it lives on the response,
            // which is null until the response arrives.
            status: flow.response?.status ?? null,
          }
        : null,
    });
    setDraft("");
    void send(text, context);
  };

  return (
    <div className="flex h-full min-h-0 flex-col border-l border-border">
      <div className="flex items-center justify-between border-b border-border px-3 py-2 text-sm font-medium">
        <span className="flex items-baseline gap-2">
          Agent
          <CodeFolderNote />
        </span>
        <div className="flex items-center gap-1">
          {running ? (
            <Button variant="ghost" size="sm" onClick={() => void interrupt()}>
              Stop
            </Button>
          ) : (
            items.length > 0 && (
              <Button variant="ghost" size="sm" onClick={() => void reset()}>
                New
              </Button>
            )
          )}
        </div>
      </div>

      {trawlConnected === false && (
        <div className="flex items-start gap-2 border-b border-http-amber/40 bg-http-amber/10 px-3 py-2 text-xs">
          <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-http-amber" />
          <span>
            The agent started without Trawl's tools — it cannot see the captured traffic. Check the
            MCP server in Settings.
          </span>
        </div>
      )}

      {noHarness ? (
        <MissingHarness onRecheck={() => void checkHarnesses()} />
      ) : (
        <>
      <div className="min-h-0 flex-1 space-y-2 overflow-auto p-3 text-sm">
        {items.length === 0 && (
          <p className="text-muted-foreground">
            Ask about the captured traffic — which hosts fail, what a response contained, why a rule
            did not match.
          </p>
        )}
        {items.map((item, i) => (
          <ChatBubble key={i} item={item} />
        ))}
        <div ref={bottom} />
      </div>

      <div className="border-t border-border p-2">
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              submit();
            }
          }}
          rows={3}
          placeholder={running ? "Working…" : "Ask about the captured traffic…"}
          className="w-full resize-none rounded-md border border-border bg-background p-2 text-sm outline-none focus:border-primary"
        />
      </div>
        </>
      )}
    </div>
  );
}

/**
 * Shown before the user types anything, because discovering the requirement
 * through a failed send is a worse way to learn it.
 */
function MissingHarness({ onRecheck }: { onRecheck: () => void }) {
  return (
    <div className="min-h-0 flex-1 overflow-auto p-4">
      <div className="flex flex-col items-center gap-1 text-center text-muted-foreground">
        <Sparkles className="size-6 opacity-40" />
        <div className="text-sm font-medium text-foreground">No agent found</div>
        <p className="max-w-xs text-xs opacity-70">
          Trawl talks to a coding agent you already have installed — it runs on your own
          subscription and Trawl never asks for an API key. Install one of these, then check again.
        </p>
      </div>

      <div className="mt-4 space-y-3">
        <div>
          <div className="text-xs font-medium">Claude Code</div>
          <CopyableCommand cmd="npm install -g @anthropic-ai/claude-code" />
        </div>
        <div>
          <div className="text-xs font-medium">Codex CLI</div>
          <CopyableCommand cmd="npm install -g @openai/codex" />
        </div>
      </div>

      <p className="mt-3 text-xs text-muted-foreground opacity-70">
        Already installed? Trawl looks for it on the PATH your login shell provides, so a command
        installed only inside an activated environment will not be found.
      </p>

      <Button variant="secondary" size="sm" className="mt-3 w-full" onClick={onRecheck}>
        <RefreshCw className="size-3.5" /> Check again
      </Button>
    </div>
  );
}

function ChatBubble({ item }: { item: ChatItem }) {
  switch (item.type) {
    case "user":
      return (
        <div className="ml-6 whitespace-pre-wrap rounded-md bg-muted/40 px-3 py-2">{item.text}</div>
      );
    case "assistant":
      return <AgentMarkdown text={item.text} />;
    case "tool":
      return <ToolCall name={item.name} input={item.input} />;
    case "error":
      return (
        <div className="rounded-md border border-http-amber/40 bg-http-amber/10 px-3 py-2">
          {item.message}
        </div>
      );
  }
}

/**
 * A tool call, with what it was actually called with.
 *
 * The name alone is not enough to know what happened: "save_rule" and
 * "save_plugin" both mean code was written into the app, and the code is in the
 * arguments. Collapsed by default so a long conversation stays readable, and
 * expanded on the calls that carry code, since those are the ones worth seeing
 * at the moment they happen.
 */
function ToolCall({ name, input }: { name: string; input: unknown }) {
  const args = (input ?? {}) as Record<string, unknown>;
  const source = pluginSource(name, args);
  const rest = source
    ? { ...args, plugin: { ...(args.plugin as object), source: `… ${source.length} bytes, below` } }
    : args;
  const hasArgs = Object.keys(args).length > 0;

  return (
    <div className="text-xs">
      <details open={Boolean(source)}>
        <summary className="cursor-pointer list-none font-mono text-muted-foreground marker:content-none">
          → {name}
          {hasArgs && <span className="ml-1 opacity-50">▸</span>}
        </summary>
        {hasArgs && (
          <pre className="mt-1 max-h-40 overflow-auto rounded bg-muted/40 p-2 font-mono text-[11px] leading-snug">
            {JSON.stringify(rest, null, 2)}
          </pre>
        )}
        {source && (
          <pre className="mt-1 max-h-72 overflow-auto rounded border border-border bg-muted/40 p-2 font-mono text-[11px] leading-snug">
            {source}
          </pre>
        )}
      </details>
    </div>
  );
}

/** The plugin body a save_plugin call carries, if this is one. */
function pluginSource(name: string, args: Record<string, unknown>): string | null {
  if (name !== "save_plugin") return null;
  const plugin = args.plugin as { source?: unknown } | undefined;
  return typeof plugin?.source === "string" ? plugin.source : null;
}
