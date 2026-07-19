import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

interface TabsProps<T extends string> {
  tabs: { value: T; label: ReactNode }[];
  value: T;
  onChange: (v: T) => void;
  className?: string;
}

export function TabBar<T extends string>({ tabs, value, onChange, className }: TabsProps<T>) {
  return (
    <div className={cn("flex items-center gap-0.5 border-b border-border px-2", className)}>
      {tabs.map((t) => (
        <button
          key={t.value}
          onClick={() => onChange(t.value)}
          className={cn(
            "relative px-3 py-2 text-xs font-medium transition-colors cursor-pointer",
            value === t.value ? "text-foreground" : "text-muted-foreground hover:text-foreground",
          )}
        >
          {t.label}
          {value === t.value && (
            <span className="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-primary" />
          )}
        </button>
      ))}
    </div>
  );
}
