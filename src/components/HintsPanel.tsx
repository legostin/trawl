import { useMemo } from "react";
import { Lightbulb } from "lucide-react";
import { useFlows } from "../store";
import { analyzeJson, accessor, matchGlob } from "@/lib/analyze";
import { bodyToText, tryParseJson } from "@/lib/body";

export function HintsPanel({
  pattern,
  onInsert,
}: {
  pattern: string;
  onInsert: (code: string) => void;
}) {
  const flows = useFlows((s) => s.flows);

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

  return (
    <div className="flex h-full w-60 shrink-0 flex-col border-l border-border">
      <div className="flex items-center gap-1.5 border-b border-border bg-card px-2 py-1.5 text-xs font-semibold text-muted-foreground">
        <Lightbulb className="size-3.5" /> Fields from past responses
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-1">
        {fields.length === 0 ? (
          <div className="p-2 text-[11px] leading-relaxed text-muted-foreground">
            No data yet. Route traffic matching this pattern to see JSON response fields here;
            clicking inserts field access into the script.
          </div>
        ) : (
          fields.map((f) => (
            <button
              key={f.path}
              onClick={() => onInsert(`${accessor(f.path)} = ;`)}
              className="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-[11px] hover:bg-accent"
              title={`Insert access to ${f.path}`}
            >
              <span className="truncate font-mono">{f.path}</span>
              <span className="ml-auto shrink-0 text-muted-foreground">{f.type}</span>
            </button>
          ))
        )}
      </div>
    </div>
  );
}
