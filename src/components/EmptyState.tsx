import type { ReactNode } from "react";

export function EmptyState({
  icon,
  title,
  hint,
}: {
  icon?: ReactNode;
  title: string;
  hint?: string;
}) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center text-muted-foreground">
      {icon && <div className="opacity-40">{icon}</div>}
      <div className="text-sm font-medium">{title}</div>
      {hint && <div className="max-w-xs text-xs opacity-70">{hint}</div>}
    </div>
  );
}
