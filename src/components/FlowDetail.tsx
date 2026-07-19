import { useState } from "react";
import {
  Copy,
  FileCode2,
  FlaskConical,
  FolderPlus,
  MousePointerClick,
  TerminalSquare,
} from "lucide-react";
import { useFlows } from "../store";
import { useRules, type Rule } from "../rules";
import { useProjects } from "../projects";
import { MethodBadge, StatusBadge } from "./badges";
import { HeadersTable } from "./HeadersTable";
import { BodyViewer } from "./BodyViewer";
import { EmptyState } from "./EmptyState";
import { TabBar } from "./ui/tabs";
import { Button } from "./ui/button";
import { buildCurl } from "@/lib/curl";
import { bodyToText } from "@/lib/body";
import { bodyLength, formatBytes, durationMs, formatDuration, formatClock } from "@/lib/format";
import type { Flow } from "@/types";

type Tab = "overview" | "request" | "response" | "timing";

function headerValue(headers: [string, string][], name: string): string | undefined {
  return headers.find(([k]) => k.toLowerCase() === name.toLowerCase())?.[1];
}

/** Паттерн из потока: точный путь + `*`, чтобы ловить query-варианты и подсказки. */
function patternFromFlow(flow: Flow): string {
  const path = flow.url.path.split("?")[0];
  return `${flow.url.host}${path}*`;
}

/** Правило-handler, повторяющее запрос (заготовка для правки). */
function ruleFromFlow(flow: Flow): Rule {
  const path = flow.url.path.split("?")[0];
  return {
    id: crypto.randomUUID(),
    name: `${flow.method} ${path}`.slice(0, 40),
    enabled: true,
    pattern: patternFromFlow(flow),
    phase: "handler",
    script: "let response = send(request);\n// modify request/response as needed\nreturn response;\n",
    projectId: null,
  };
}

/** Правило-мок, возвращающее пойманный ответ. */
function mockRuleFromFlow(flow: Flow): Rule {
  const path = flow.url.path.split("?")[0];
  const status = flow.response?.status ?? 200;
  const ct = headerValue(flow.response?.headers ?? [], "content-type") ?? "application/json";
  const body = bodyToText(flow.response);
  return {
    id: crypto.randomUUID(),
    name: `mock ${path}`.slice(0, 40),
    enabled: true,
    pattern: patternFromFlow(flow),
    phase: "request",
    projectId: null,
    script:
      `ctx.mock({\n` +
      `  status: ${status},\n` +
      `  headers: { 'content-type': ${JSON.stringify(ct)} },\n` +
      `  body: ${JSON.stringify(body)},\n` +
      `});\n`,
  };
}

export function FlowDetail() {
  const flow = useFlows((s) => s.flows.find((f) => f.id === s.selectedId) ?? null);
  const setView = useFlows((s) => s.setView);
  const upsertRule = useRules((s) => s.upsert);
  const activeId = useProjects((s) => s.activeId);
  const addHost = useProjects((s) => s.addHost);
  const [tab, setTab] = useState<Tab>("overview");

  const createRule = async (rule: Rule) => {
    await upsertRule({ ...rule, projectId: activeId ?? null });
    setView("rules");
  };

  if (!flow) {
    return (
      <EmptyState
        icon={<MousePointerClick className="size-8" />}
        title="Select a request"
        hint="Click a row in the list or a tree node to see details."
      />
    );
  }

  const { scheme, host, port, path } = flow.url;
  const showPort = port !== 80 && port !== 443;
  const url = `${scheme}://${host}${showPort ? `:${port}` : ""}${path}`;
  const reqSize = bodyLength(flow.request);
  const resSize = bodyLength(flow.response);
  const dur = durationMs(flow.timings.sent, flow.timings.done);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-start gap-2 border-b border-border bg-card px-3 py-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <MethodBadge method={flow.method} className="text-xs" />
            <StatusBadge status={flow.response?.status} className="text-xs" />
            {flow.state === "error" && (
              <span className="text-xs text-http-red">{flow.error ?? "error"}</span>
            )}
          </div>
          <div className="mt-1 break-all font-mono text-xs text-muted-foreground">{url}</div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {activeId && (
            <Button
              variant="outline"
              size="sm"
              title="Add host to the active project"
              onClick={() => void addHost(activeId, flow.url.host)}
            >
              <FolderPlus />To project
            </Button>
          )}
          <Button
            variant="outline"
            size="sm"
            title="Create a rule from this request"
            onClick={() => void createRule(ruleFromFlow(flow))}
          >
            <FileCode2 />
            Rule
          </Button>
          <Button
            variant="outline"
            size="sm"
            title="Create a mock from this response"
            disabled={!flow.response}
            onClick={() => void createRule(mockRuleFromFlow(flow))}
          >
            <FlaskConical />
            Mock
          </Button>
          <Button
            variant="outline"
            size="sm"
            title="Copy as cURL"
            onClick={() => void navigator.clipboard.writeText(buildCurl(flow))}
          >
            <TerminalSquare />
            cURL
          </Button>
          <Button
            variant="ghost"
            size="iconSm"
            title="Copy URL"
            onClick={() => void navigator.clipboard.writeText(url)}
          >
            <Copy />
          </Button>
        </div>
      </div>

      <TabBar<Tab>
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "overview", label: "Overview" },
          { value: "request", label: "Request" },
          { value: "response", label: "Response" },
          { value: "timing", label: "Timing" },
        ]}
      />

      <div className="min-h-0 flex-1 overflow-auto">
        {tab === "overview" && (
          <dl className="grid grid-cols-[120px_1fr] gap-x-3 gap-y-1.5 p-3 text-xs">
            <dt className="text-muted-foreground">Method</dt>
            <dd>
              <MethodBadge method={flow.method} />
            </dd>
            <dt className="text-muted-foreground">Status</dt>
            <dd>
              <StatusBadge status={flow.response?.status} />
            </dd>
            <dt className="text-muted-foreground">Host</dt>
            <dd className="font-mono break-all">{host}</dd>
            <dt className="text-muted-foreground">Path</dt>
            <dd className="font-mono break-all">{path}</dd>
            <dt className="text-muted-foreground">Time</dt>
            <dd className="font-mono">{formatClock(flow.timestamp)}</dd>
            <dt className="text-muted-foreground">Request size</dt>
            <dd className="font-mono">{formatBytes(reqSize)}</dd>
            <dt className="text-muted-foreground">Response size</dt>
            <dd className="font-mono">{formatBytes(resSize)}</dd>
            <dt className="text-muted-foreground">Duration</dt>
            <dd className="font-mono">{formatDuration(dur)}</dd>
          </dl>
        )}

        {tab === "request" && (
          <div>
            <SectionTitle>Headers</SectionTitle>
            <div className="px-3">
              <HeadersTable headers={flow.request.headers} />
            </div>
            <SectionTitle>Body</SectionTitle>
            <BodyViewer msg={flow.request} />
          </div>
        )}

        {tab === "response" && (
          <div>
            <SectionTitle>Headers</SectionTitle>
            <div className="px-3">
              <HeadersTable headers={flow.response?.headers ?? []} />
            </div>
            <SectionTitle>Body</SectionTitle>
            <BodyViewer msg={flow.response} />
          </div>
        )}

        {tab === "timing" && (
          <dl className="grid grid-cols-[120px_1fr] gap-x-3 gap-y-1.5 p-3 font-mono text-xs">
            <dt className="text-muted-foreground">sent</dt>
            <dd>{flow.timings.sent ?? "—"}</dd>
            <dt className="text-muted-foreground">ttfb</dt>
            <dd>{flow.timings.ttfb ?? "—"}</dd>
            <dt className="text-muted-foreground">done</dt>
            <dd>{flow.timings.done ?? "—"}</dd>
            <dt className="text-muted-foreground">duration</dt>
            <dd>{formatDuration(dur)}</dd>
          </dl>
        )}
      </div>
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <div className="px-3 py-1.5 text-[10px] font-semibold uppercase tracking-wide text-muted-foreground">
      {children}
    </div>
  );
}
