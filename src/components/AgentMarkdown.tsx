import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ReactNode } from "react";
import { ArrowLeftRight, FileDown, FolderOpen, Filter } from "lucide-react";
import {
  parseTrawlLink,
  followTrawlLink,
  revealArtifact,
  allowTrawlLinks,
  type TrawlLink,
} from "../agent/links";

const CHIP_ICON = {
  flow: ArrowLeftRight,
  rule: Filter,
  artifact: FileDown,
} as const;

/**
 * A reference to something in this app, rendered as a chip rather than a link.
 *
 * It behaves differently from a link — it moves the app rather than leaving it
 * — so it should not look like one. The icon says which kind of thing it points
 * at without the agent having to spell it out in the text.
 */
function TrawlChip({ target, children }: { target: TrawlLink; children: ReactNode }) {
  const Icon = CHIP_ICON[target.kind];
  return (
    <span className="inline-flex max-w-full items-center gap-1 align-baseline">
      <button
        onClick={() => followTrawlLink(target)}
        title={target.kind === "artifact" ? "Open" : "Show in the app"}
        className="inline-flex max-w-full items-center gap-1 rounded border border-border bg-muted/40 px-1.5 py-px font-mono text-[11px] leading-relaxed text-foreground transition-colors hover:border-primary hover:text-primary"
      >
        <Icon className="size-3 shrink-0 opacity-60" />
        <span className="truncate">{children}</span>
      </button>
      {target.kind === "artifact" && (
        // Opening the file is the common case, but a file you cannot find on
        // disk is only half delivered.
        <button
          title="Show in Finder"
          className="text-muted-foreground hover:text-foreground"
          onClick={() => void revealArtifact(target.id)}
        >
          <FolderOpen className="size-3" />
        </button>
      )}
    </span>
  );
}

/**
 * Renders an agent answer as Markdown.
 *
 * Raw HTML stays disabled (no rehype-raw): the answer can quote response
 * bodies captured from someone else's server, and that is not content this
 * app should be executing markup from.
 */
export function AgentMarkdown({ text }: { text: string }) {
  return (
    <div className="space-y-2 break-words">
      <Markdown
        remarkPlugins={[remarkGfm]}
        // Without this every trawl: link arrives with an empty href: the
        // renderer blanks schemes outside its own safe list.
        urlTransform={allowTrawlLinks}
        components={{
          // Soft line breaks inside a paragraph are meaningful in chat, so they
          // are preserved rather than collapsed into spaces.
          p: ({ children }) => <p className="whitespace-pre-wrap">{children}</p>,

          // The column is narrow. A wide table scrolls inside its own box
          // instead of stretching the panel and squeezing the app beside it.
          table: ({ children }) => (
            <div className="overflow-x-auto">
              <table className="w-full border-collapse text-xs">{children}</table>
            </div>
          ),
          th: ({ children }) => (
            <th className="border border-border px-2 py-1 text-left font-medium">{children}</th>
          ),
          td: ({ children }) => (
            <td className="border border-border px-2 py-1 align-top">{children}</td>
          ),

          pre: ({ children }) => <pre className="overflow-x-auto">{children}</pre>,
          code: ({ className, children }) => {
            const isBlock = /language-/.test(className ?? "");
            return isBlock ? (
              <code className="block rounded bg-muted/40 p-2 font-mono text-xs leading-snug">
                {children}
              </code>
            ) : (
              <code className="rounded bg-muted/40 px-1 font-mono text-[0.9em]">{children}</code>
            );
          },

          ul: ({ children }) => <ul className="list-disc space-y-0.5 pl-5">{children}</ul>,
          ol: ({ children }) => <ol className="list-decimal space-y-0.5 pl-5">{children}</ol>,
          h1: ({ children }) => <h1 className="text-sm font-semibold">{children}</h1>,
          h2: ({ children }) => <h2 className="text-sm font-semibold">{children}</h2>,
          h3: ({ children }) => <h3 className="text-sm font-semibold">{children}</h3>,
          blockquote: ({ children }) => (
            <blockquote className="border-l-2 border-border pl-3 text-muted-foreground">
              {children}
            </blockquote>
          ),
          hr: () => <hr className="border-border" />,

          // A plain <a> would navigate the whole webview away from the app.
          // Hand the URL to the OS instead, as the plugins panel already does.
          a: ({ href, children }) => {
            // A trawl: link points at something in this app — a flow, a rule,
            // a file the agent wrote — and navigates here. Everything else
            // still leaves for the browser.
            const target = parseTrawlLink(href);
            if (!target) {
              return (
                <a
                  href={href}
                  title={href}
                  className="text-primary underline underline-offset-2"
                  onClick={(e) => {
                    e.preventDefault();
                    if (href) void openUrl(href);
                  }}
                >
                  {children}
                </a>
              );
            }
            return <TrawlChip target={target}>{children}</TrawlChip>;
          },
        }}
      >
        {text}
      </Markdown>
    </div>
  );
}
