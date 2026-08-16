import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";
import { FolderOpen } from "lucide-react";
import { parseTrawlLink, followTrawlLink, revealArtifact } from "../agent/links";

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
            const link = (
              <a
                href={href}
                title={target ? undefined : href}
                className="text-primary underline underline-offset-2"
                onClick={(e) => {
                  e.preventDefault();
                  if (target) followTrawlLink(target);
                  else if (href) void openUrl(href);
                }}
              >
                {children}
              </a>
            );
            if (target?.kind !== "artifact") return link;
            // Opening a file is the common case, but a file you cannot find on
            // disk is only half delivered.
            return (
              <span className="inline-flex items-center gap-1">
                {link}
                <button
                  title="Show in Finder"
                  className="text-muted-foreground hover:text-foreground"
                  onClick={() => void revealArtifact(target.id)}
                >
                  <FolderOpen className="size-3" />
                </button>
              </span>
            );
          },
        }}
      >
        {text}
      </Markdown>
    </div>
  );
}
