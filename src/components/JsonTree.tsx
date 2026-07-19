import { useState } from "react";
import { ChevronRight } from "lucide-react";
import { cn } from "@/lib/utils";

function Primitive({ value }: { value: unknown }) {
  if (value === null) return <span className="text-http-gray">null</span>;
  switch (typeof value) {
    case "string":
      return <span className="text-http-green">"{value}"</span>;
    case "number":
      return <span className="text-http-blue">{String(value)}</span>;
    case "boolean":
      return <span className="text-http-amber">{String(value)}</span>;
    default:
      return <span>{String(value)}</span>;
  }
}

function Node({
  keyName,
  value,
  depth,
}: {
  keyName?: string;
  value: unknown;
  depth: number;
}) {
  const isObj = value !== null && typeof value === "object";
  const [open, setOpen] = useState(depth < 1);

  const label =
    keyName !== undefined ? <span className="text-muted-foreground">{keyName}: </span> : null;

  if (!isObj) {
    return (
      <div className="leading-5">
        {label}
        <Primitive value={value} />
      </div>
    );
  }

  const isArray = Array.isArray(value);
  const entries = isArray
    ? (value as unknown[]).map((v, i) => [String(i), v] as const)
    : Object.entries(value as Record<string, unknown>);
  const [ob, cb] = isArray ? ["[", "]"] : ["{", "}"];

  return (
    <div className="leading-5">
      <div className="cursor-pointer select-none" onClick={() => setOpen(!open)}>
        <ChevronRight
          className={cn(
            "-mt-0.5 mr-0.5 inline size-3 text-muted-foreground transition-transform",
            open && "rotate-90",
          )}
        />
        {label}
        <span className="text-muted-foreground">
          {ob}
          {!open && ` … ${entries.length} ${cb}`}
        </span>
      </div>
      {open && (
        <>
          <div className="border-l border-border/60 pl-3">
            {entries.map(([k, v]) => (
              <Node key={k} keyName={isArray ? undefined : k} value={v} depth={depth + 1} />
            ))}
          </div>
          <span className="text-muted-foreground">{cb}</span>
        </>
      )}
    </div>
  );
}

export function JsonTree({ data }: { data: unknown }) {
  return (
    <div className="font-mono text-xs">
      <Node value={data} depth={0} />
    </div>
  );
}
