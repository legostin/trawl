import { useEffect, useRef, useState } from "react";
import { AlertTriangle } from "lucide-react";
import { useAgent } from "../agent/agentStore";
import { describeScreen } from "../agent/screenContext";
import type { ChatItem } from "../agent/chatItems";
import { useLayout } from "../layout";
import { useFlows } from "../store";
import { useProjects } from "../projects";
import { Button } from "./ui/button";
import { AgentMarkdown } from "./AgentMarkdown";

export function AgentPanel() {
  const items = useAgent((s) => s.items);
  const running = useAgent((s) => s.running);
  const trawlConnected = useAgent((s) => s.trawlConnected);
  const init = useAgent((s) => s.init);
  const send = useAgent((s) => s.send);
  const interrupt = useAgent((s) => s.interrupt);
  const reset = useAgent((s) => s.reset);

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
  }, [init]);

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
        <span>Agent</span>
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
      return <div className="font-mono text-xs text-muted-foreground">→ {item.name}</div>;
    case "error":
      return (
        <div className="rounded-md border border-http-amber/40 bg-http-amber/10 px-3 py-2">
          {item.message}
        </div>
      );
  }
}
