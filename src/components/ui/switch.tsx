import { cn } from "@/lib/utils";

/** Small on/off toggle (accessible switch role). Click never bubbles, so it's
 *  safe inside clickable list rows. */
export function Switch({
  checked,
  onCheckedChange,
  disabled,
  className,
  title,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
  title?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      title={title}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onCheckedChange(!checked);
      }}
      className={cn(
        "relative inline-flex h-4 w-7 shrink-0 cursor-pointer items-center rounded-full transition-colors",
        checked ? "bg-http-green" : "border border-border bg-secondary",
        disabled && "cursor-not-allowed opacity-50",
        className,
      )}
    >
      <span
        className={cn(
          "pointer-events-none block size-3 rounded-full bg-background shadow transition-transform",
          checked ? "translate-x-3.5" : "translate-x-0.5",
        )}
      />
    </button>
  );
}

/** Switch with a small caption on the right. */
export function LabeledSwitch({
  label,
  checked,
  onCheckedChange,
  title,
}: {
  label: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  title?: string;
}) {
  return (
    <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
      <Switch checked={checked} onCheckedChange={onCheckedChange} title={title ?? label} />
      {label}
    </span>
  );
}
