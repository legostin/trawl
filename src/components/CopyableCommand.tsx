import { Copy } from "lucide-react";
import { useToast } from "../toast";

export function CopyableCommand({ cmd }: { cmd: string }) {
  const show = useToast((s) => s.show);
  return (
    <div className="relative mt-1.5 whitespace-pre-wrap break-all rounded bg-secondary/60 p-2 pr-8 font-mono text-[11px]">
      {cmd}
      <button
        title="Copy"
        onClick={() => {
          void navigator.clipboard.writeText(cmd);
          show("Copied");
        }}
        className="absolute right-1 top-1 flex size-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
      >
        <Copy className="size-3" />
      </button>
    </div>
  );
}
