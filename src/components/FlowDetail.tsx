import { useState } from "react";
import {
  ChevronDown,
  CircleDot,
  Copy,
  FileCode2,
  FlaskConical,
  FolderPlus,
  MousePointerClick,
  Plus,
  TerminalSquare,
} from "lucide-react";
import { useFlows } from "../store";
import { useRules, type Rule } from "../rules";
import { useBreakpoints, breakpointFromFlow } from "../breakpoints";
import { useProjects, projectTracks, type Project } from "../projects";
import { usePlugins } from "../plugins";
import { useToast } from "../toast";
import { MethodBadge, StatusBadge } from "./badges";
import { HeadersTable } from "./HeadersTable";
import { InterceptEditor } from "./InterceptEditor";
import { BodyViewer } from "./BodyViewer";
import { EmptyState } from "./EmptyState";
import { TabBar } from "./ui/tabs";
import { Button } from "./ui/button";
import { buildCurl } from "@/lib/curl";
import { bodyToText } from "@/lib/body";
import { queryParams, formParams, isFormEncoded } from "@/lib/params";
import { parseRequestCookies, parseResponseCookies } from "@/lib/cookies";
import { CookiesTable } from "./CookiesTable";
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
  const upsertBreakpoint = useBreakpoints((s) => s.upsert);
  const activeId = useProjects((s) => s.activeId);
  const flowActions = usePlugins((s) => s.flowActions);
  const [tab, setTab] = useState<Tab>("overview");

  const createRule = async (rule: Rule) => {
    await upsertRule({ ...rule, projectId: activeId ?? null });
    setView("rules");
  };

  const createBreakpoint = async (f: Flow) => {
    await upsertBreakpoint(breakpointFromFlow(f, activeId ?? null));
    setView("breakpoints");
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

  if (flow.pausedPhase) {
    return <InterceptEditor flow={flow} />;
  }

  const { scheme, host, port, path } = flow.url;
  const showPort = port !== 80 && port !== 443;
  const url = `${scheme}://${host}${showPort ? `:${port}` : ""}${path}`;
  const reqSize = bodyLength(flow.request);
  const resSize = bodyLength(flow.response);
  const dur = durationMs(flow.timings.sent, flow.timings.done);

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-border bg-card px-3 py-2">
        <div className="flex items-center gap-2">
          <MethodBadge method={flow.method} className="text-xs" />
          <StatusBadge status={flow.response?.status} className="text-xs" />
          {flow.state === "error" && (
            <span className="truncate text-xs text-http-red">{flow.error ?? "error"}</span>
          )}
          <div className="ml-auto flex shrink-0 items-center gap-1">
            {flowActions.map((a) => (
              <Button
                key={a.id}
                variant="outline"
                size="sm"
                title={a.label}
                onClick={() => a.run(flow)}
              >
                {a.icon && <a.icon />}
                {a.label}
              </Button>
            ))}
            <ProjectAction host={flow.url.host} />
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
              title="Create a breakpoint for this endpoint"
              onClick={() => void createBreakpoint(flow)}
            >
              <CircleDot />
              Breakpoint
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
        <div className="mt-1.5 break-all font-mono text-xs text-muted-foreground">{url}</div>
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
            {flow.ruleTrace?.length > 0 && (
              <>
                <dt className="text-muted-foreground">Rule trace</dt>
                <dd className="font-mono">
                  {flow.ruleTrace.map((t, i) => (
                    <div key={i}>
                      {t.rule}: {t.op}
                      {t.path ? `('${t.path}')` : ""}
                      {t.nodes !== undefined ? ` → ${t.nodes} узлов` : ""}
                      {t.status !== undefined ? ` → ${t.status} (${t.ms} ms)` : ""}
                    </div>
                  ))}
                </dd>
              </>
            )}
          </dl>
        )}

        {tab === "request" && <RequestPanel flow={flow} />}

        {tab === "response" && <ResponsePanel flow={flow} />}

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

type ReqTab = "query" | "form" | "cookies" | "headers" | "body";

function RequestPanel({ flow }: { flow: Flow }) {
  const query = queryParams(flow.url.path);
  const form = formParams(flow.request);
  const hasForm = isFormEncoded(flow.request);
  const cookies = parseRequestCookies(flow.request.headers);

  const tabs: { value: ReqTab; label: string }[] = [];
  if (query.length > 0) tabs.push({ value: "query", label: `Query (${query.length})` });
  if (hasForm) tabs.push({ value: "form", label: `Form (${form.length})` });
  if (cookies.length > 0) tabs.push({ value: "cookies", label: `Cookies (${cookies.length})` });
  tabs.push({ value: "headers", label: `Headers (${flow.request.headers.length})` });
  tabs.push({ value: "body", label: "Body" });

  const [tab, setTab] = useState<ReqTab>(tabs[0].value);
  // Поток мог смениться — держим активную вкладку валидной.
  const active = tabs.some((t) => t.value === tab) ? tab : tabs[0].value;

  return (
    <div>
      <TabBar<ReqTab> value={active} onChange={setTab} tabs={tabs} />
      {active === "query" && (
        <div className="px-3 pt-2">
          <HeadersTable headers={query} emptyText="No query parameters" />
        </div>
      )}
      {active === "form" && (
        <div className="px-3 pt-2">
          <HeadersTable headers={form} emptyText="No form parameters" />
        </div>
      )}
      {active === "cookies" && <CookiesTable cookies={cookies} />}
      {active === "headers" && (
        <div className="px-3 pt-2">
          <HeadersTable headers={flow.request.headers} />
        </div>
      )}
      {active === "body" && <BodyViewer msg={flow.request} />}
    </div>
  );
}

type ResTab = "cookies" | "headers" | "body";

function ResponsePanel({ flow }: { flow: Flow }) {
  const headers = flow.response?.headers ?? [];
  const cookies = parseResponseCookies(headers);

  const tabs: { value: ResTab; label: string }[] = [];
  if (cookies.length > 0) tabs.push({ value: "cookies", label: `Cookies (${cookies.length})` });
  tabs.push({ value: "headers", label: `Headers (${headers.length})` });
  tabs.push({ value: "body", label: "Body" });

  const [tab, setTab] = useState<ResTab>(tabs[0].value);
  const active = tabs.some((t) => t.value === tab) ? tab : tabs[0].value;

  return (
    <div>
      <TabBar<ResTab> value={active} onChange={setTab} tabs={tabs} />
      {active === "cookies" && <CookiesTable cookies={cookies} emptyText="No Set-Cookie headers" />}
      {active === "headers" && (
        <div className="px-3 pt-2">
          <HeadersTable headers={headers} />
        </div>
      )}
      {active === "body" && <BodyViewer msg={flow.response} />}
    </div>
  );
}

function ProjectAction({ host }: { host: string }) {
  const projects = useProjects((s) => s.projects);
  const activeId = useProjects((s) => s.activeId);
  const addHost = useProjects((s) => s.addHost);
  const upsert = useProjects((s) => s.upsert);
  const show = useToast((s) => s.show);
  const [open, setOpen] = useState(false);

  const active = projects.find((p) => p.id === activeId) ?? null;

  // Уже в активном проекте — кнопку не показываем.
  if (active && projectTracks(active, host)) return null;

  // Есть активный проект — просто добавляем в него.
  if (active) {
    return (
      <Button
        variant="outline"
        size="sm"
        title={`Add ${host} to “${active.name}”`}
        onClick={() => {
          void addHost(active.id, host);
          show(`Added ${host} to ${active.name}`);
        }}
      >
        <FolderPlus />
        To project
      </Button>
    );
  }

  // Проекта нет — выбор проекта или создание нового прямо отсюда.
  const addTo = async (id: string, name: string) => {
    await addHost(id, host);
    show(`Added ${host} to ${name}`);
    setOpen(false);
  };
  const createNew = async () => {
    const p: Project = {
      id: crypto.randomUUID(),
      name: host,
      includeHosts: [host],
      excludeHosts: [],
      env: [],
    };
    await upsert(p);
    show(`Project “${host}” created`);
    setOpen(false);
  };

  return (
    <div className="relative">
      <Button variant="outline" size="sm" title="Add host to a project" onClick={() => setOpen((o) => !o)}>
        <FolderPlus />
        To project
        <ChevronDown className="opacity-60" />
      </Button>
      {open && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setOpen(false)} />
          <div className="absolute right-0 z-20 mt-1 w-56 rounded-md border border-border bg-popover p-1 text-xs shadow-lg">
            {projects.map((p) => (
              <button
                key={p.id}
                onClick={() => void addTo(p.id, p.name)}
                className="block w-full truncate rounded px-2 py-1.5 text-left hover:bg-accent"
              >
                {p.name}
              </button>
            ))}
            {projects.length > 0 && <div className="my-1 border-t border-border" />}
            <button
              onClick={() => void createNew()}
              className="flex w-full items-center gap-1.5 rounded px-2 py-1.5 text-left hover:bg-accent"
            >
              <Plus className="size-3" />
              New project “{host}”
            </button>
          </div>
        </>
      )}
    </div>
  );
}
