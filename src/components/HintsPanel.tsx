import { useMemo } from "react";
import { Lightbulb, Plus, Replace, Variable } from "lucide-react";
import { useFlows } from "../store";
import { useRules } from "../rules";
import { useProjects } from "../projects";
import { useToast } from "../toast";
import { analyzeJson, accessor, matchGlob, type FieldInfo } from "@/lib/analyze";
import { bodyToText, tryParseJson } from "@/lib/body";
import { saveToEnvRule, overrideRule } from "../scripting/genRules";
import { cn } from "@/lib/utils";

export function HintsPanel({
  pattern,
  onInsert,
}: {
  pattern: string;
  onInsert: (code: string) => void;
}) {
  const flows = useFlows((s) => s.flows);
  const setView = useFlows((s) => s.setView);
  const upsertRule = useRules((s) => s.upsert);
  const activeId = useProjects((s) => s.activeId);
  const showToast = useToast((s) => s.show);

  const fields = useMemo(() => {
    const matched = flows.filter((f) =>
      [`${f.url.host}${f.url.path}`, `${f.url.host}:${f.url.port}${f.url.path}`].some((t) =>
        matchGlob(pattern, t),
      ),
    );
    const values: unknown[] = [];
    for (const f of matched.slice(-20)) {
      const parsed = tryParseJson(bodyToText(f.response));
      if (parsed !== null) values.push(parsed);
    }
    return analyzeJson(values);
  }, [flows, pattern]);

  const createRule = async (rule: Parameters<typeof upsertRule>[0]) => {
    await upsertRule({ ...rule, projectId: activeId ?? null });
    showToast("Rule created");
    setView("rules");
  };

  return (
    <div className="flex h-full w-64 shrink-0 flex-col border-l border-border">
      <div className="flex items-center gap-1.5 border-b border-border bg-card px-2 py-1.5 text-xs font-semibold text-muted-foreground">
        <Lightbulb className="size-3.5" /> Fields from past responses
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-1">
        {fields.length === 0 ? (
          <div className="p-2 text-[11px] leading-relaxed text-muted-foreground">
            No data yet. Route traffic matching this pattern to see JSON response fields here, with
            example values and one-click actions.
          </div>
        ) : (
          fields.map((f) => (
            <FieldRow
              key={f.path}
              field={f}
              onInsert={() => onInsert(`${accessor(f.path)} = ;`)}
              onEnv={() => void createRule(saveToEnvRule(pattern, f.path, activeId ?? null))}
              onOverride={() =>
                void createRule(overrideRule(pattern, f.path, f.example, activeId ?? null))
              }
            />
          ))
        )}
      </div>
    </div>
  );
}

function FieldRow({
  field,
  onInsert,
  onEnv,
  onOverride,
}: {
  field: FieldInfo;
  onInsert: () => void;
  onEnv: () => void;
  onOverride: () => void;
}) {
  return (
    <div className="group rounded px-2 py-1 hover:bg-accent">
      <div className="flex items-center gap-1.5 text-[11px]">
        <span className="truncate font-mono">{field.path}</span>
        {field.varying && (
          <span
            className="size-1.5 shrink-0 rounded-full bg-http-amber"
            title="Value varies between responses (dynamic)"
          />
        )}
        <span className="ml-auto shrink-0 text-muted-foreground">{field.type}</span>
      </div>
      <div className="mt-0.5 flex items-center gap-1">
        <span
          className={cn(
            "min-w-0 flex-1 truncate font-mono text-[10px]",
            field.varying ? "text-http-amber" : "text-muted-foreground",
          )}
          title={field.example}
        >
          {field.type === "array" ? "[…]" : (field.example ?? "")}
        </span>
        <div className="flex shrink-0 items-center gap-0.5 opacity-0 group-hover:opacity-100">
          <IconBtn title="Insert accessor" onClick={onInsert}>
            <Plus className="size-3" />
          </IconBtn>
          <IconBtn title="Save to project env" onClick={onEnv}>
            <Variable className="size-3" />
          </IconBtn>
          <IconBtn title="Override value (mock)" onClick={onOverride}>
            <Replace className="size-3" />
          </IconBtn>
        </div>
      </div>
    </div>
  );
}

function IconBtn({
  title,
  onClick,
  children,
}: {
  title: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      title={title}
      onClick={onClick}
      className="flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
    >
      {children}
    </button>
  );
}
