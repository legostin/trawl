import { useState } from "react";
import { Copy } from "lucide-react";
import { bodyToText, tryParseJson } from "@/lib/body";
import { JsonTree } from "./JsonTree";
import { Button } from "./ui/button";
import type { HttpMessage, ResponseMessage } from "@/types";

const LIMIT = 200_000;

export function BodyViewer({ msg }: { msg: HttpMessage | ResponseMessage | null }) {
  const [expanded, setExpanded] = useState(false);
  const text = bodyToText(msg);

  if (!text) return <div className="p-3 text-xs text-muted-foreground">No body</div>;
  if (text.startsWith("<binary"))
    return <div className="p-3 font-mono text-xs text-muted-foreground">{text}</div>;

  const tooBig = text.length > LIMIT && !expanded;
  const json = tooBig ? null : tryParseJson(text);

  return (
    <div className="relative">
      <Button
        variant="ghost"
        size="iconSm"
        className="absolute right-1 top-1 z-10"
        title="Copy body"
        onClick={() => void navigator.clipboard.writeText(text)}
      >
        <Copy />
      </Button>
      {tooBig ? (
        <div className="p-3 text-xs text-muted-foreground">
          Body is large ({(text.length / 1024).toFixed(0)} KB).{" "}
          <button className="text-primary underline" onClick={() => setExpanded(true)}>
            Show
          </button>
        </div>
      ) : json !== null ? (
        <div className="p-3">
          <JsonTree data={json} />
        </div>
      ) : (
        <pre className="whitespace-pre-wrap break-words p-3 font-mono text-xs text-foreground">
          {text}
        </pre>
      )}
    </div>
  );
}
