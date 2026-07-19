import { cn } from "@/lib/utils";

/** Цвет метода HTTP (Tailwind text-класс на семантическом токене). */
export function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case "GET":
      return "text-http-green";
    case "POST":
      return "text-http-blue";
    case "PUT":
    case "PATCH":
      return "text-http-amber";
    case "DELETE":
      return "text-http-red";
    case "HEAD":
    case "OPTIONS":
      return "text-http-purple";
    default:
      return "text-http-gray";
  }
}

/** Цвет класса статуса ответа. */
export function statusColor(status: number | undefined): string {
  if (status === undefined) return "text-http-gray";
  switch (Math.floor(status / 100)) {
    case 2:
      return "text-http-green";
    case 3:
      return "text-http-blue";
    case 4:
      return "text-http-amber";
    case 5:
      return "text-http-red";
    default:
      return "text-http-gray";
  }
}

export function MethodBadge({ method, className }: { method: string; className?: string }) {
  return (
    <span className={cn("font-mono text-[11px] font-bold", methodColor(method), className)}>
      {method.toUpperCase()}
    </span>
  );
}

export function StatusBadge({
  status,
  className,
}: {
  status: number | undefined;
  className?: string;
}) {
  return (
    <span className={cn("font-mono text-[11px] font-semibold", statusColor(status), className)}>
      {status ?? "···"}
    </span>
  );
}
