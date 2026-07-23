import { Plus, X } from "lucide-react";
import type { EnvVar } from "../projects";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

export function EnvList({
  env,
  onChange,
  hint,
  overrideKeys,
}: {
  env: EnvVar[];
  onChange: (env: EnvVar[]) => void;
  hint: string;
  overrideKeys?: Set<string>;
}) {
  const setAt = (i: number, patch: Partial<EnvVar>) =>
    onChange(env.map((e, idx) => (idx === i ? { ...e, ...patch } : e)));
  return (
    <div>
      <div className="text-xs font-semibold">Environment variables</div>
      <div className="mb-1.5 text-[11px] text-muted-foreground">{hint}</div>
      <div className="space-y-1">
        {env.map((e, i) => (
          <div key={i} className="flex items-center gap-1">
            <Input
              value={e.key}
              onChange={(ev) => setAt(i, { key: ev.target.value })}
              placeholder="KEY"
              className="h-7 w-40 font-mono"
            />
            <Input
              value={e.value}
              onChange={(ev) => setAt(i, { value: ev.target.value })}
              placeholder="value"
              className="h-7 flex-1 font-mono"
            />
            {overrideKeys?.has(e.key) && (
              <span className="shrink-0 rounded bg-secondary px-1.5 py-0.5 text-[10px] text-muted-foreground">
                overrides Global
              </span>
            )}
            <Button
              size="iconSm"
              variant="ghost"
              title="Remove"
              onClick={() => onChange(env.filter((_, idx) => idx !== i))}
            >
              <X className="size-3" />
            </Button>
          </div>
        ))}
      </div>
      <Button
        size="sm"
        variant="outline"
        className="mt-1.5"
        onClick={() => onChange([...env, { key: "", value: "" }])}
      >
        <Plus />
        Add variable
      </Button>
    </div>
  );
}
