import { cn } from "@/lib/utils";

/** A small pulsing red dot — signals flows held on a breakpoint (needs attention). */
export function PulseDot({ className }: { className?: string }) {
  return (
    <span className={cn("relative flex size-2", className)} aria-hidden>
      <span className="absolute inline-flex size-full animate-ping rounded-full bg-http-red opacity-75" />
      <span className="relative inline-flex size-2 rounded-full bg-http-red" />
    </span>
  );
}
