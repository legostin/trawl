import { Copy } from "lucide-react";
import type { Cookie } from "@/lib/cookies";

export function CookiesTable({ cookies, emptyText = "No cookies" }: { cookies: Cookie[]; emptyText?: string }) {
  if (cookies.length === 0) {
    return <div className="p-3 text-xs text-muted-foreground">{emptyText}</div>;
  }
  return (
    <div className="flex flex-col gap-1.5 p-3">
      {cookies.map((c, i) => (
        <div key={i} className="group rounded-md border border-border/60 bg-card px-2.5 py-1.5">
          <div className="flex items-center gap-2">
            <span className="shrink-0 font-mono text-xs font-semibold text-primary">{c.name}</span>
            <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground" title={c.value}>
              {c.value}
            </span>
            <button
              title="Copy value"
              onClick={() => void navigator.clipboard.writeText(c.value)}
              className="shrink-0 rounded p-1 text-muted-foreground opacity-0 transition hover:bg-secondary hover:text-foreground group-hover:opacity-100"
            >
              <Copy className="size-3" />
            </button>
            <button
              title="Copy name=value"
              onClick={() => void navigator.clipboard.writeText(`${c.name}=${c.value}`)}
              className="shrink-0 rounded px-1 py-0.5 text-[10px] text-muted-foreground opacity-0 transition hover:bg-secondary hover:text-foreground group-hover:opacity-100"
            >
              n=v
            </button>
          </div>
          {c.attrs.length > 0 && (
            <div className="mt-1 flex flex-wrap gap-1">
              {c.attrs.map(([k, v], j) => (
                <span key={j} className="rounded bg-secondary px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground">
                  {v ? `${k}=${v}` : k}
                </span>
              ))}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
