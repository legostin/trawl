import { CheckCircle2 } from "lucide-react";
import { useToast } from "../toast";

export function Toast() {
  const message = useToast((s) => s.message);
  if (!message) return null;
  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-8 z-[60] flex justify-center">
      <div className="flex items-center gap-2 rounded-md border border-border bg-card px-3 py-2 text-sm shadow-lg">
        <CheckCircle2 className="size-4 text-http-green" />
        {message}
      </div>
    </div>
  );
}
