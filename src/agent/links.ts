import { invoke } from "@tauri-apps/api/core";
import { defaultUrlTransform } from "react-markdown";
import { useFlows } from "../store";
import { useRules } from "../rules";
import { useLayout } from "../layout";

export type TrawlLink =
  | { kind: "flow"; id: number }
  | { kind: "rule"; id: string }
  | { kind: "artifact"; id: string };

/**
 * Reads a `trawl:` link out of an agent's answer.
 *
 * Ordinary links keep going to the browser; only this scheme navigates inside
 * the app. Returns null for anything it does not recognise rather than
 * guessing, because the text it parses is partly written from captured traffic.
 */
export function parseTrawlLink(href: string | undefined): TrawlLink | null {
  if (!href) return null;
  const rest = href.startsWith("trawl://")
    ? href.slice("trawl://".length)
    : href.startsWith("trawl:")
      ? href.slice("trawl:".length)
      : null;
  if (rest === null) return null;

  const slash = rest.indexOf("/");
  if (slash < 0) return null;
  const kind = rest.slice(0, slash);
  const id = decodeURIComponent(rest.slice(slash + 1));
  if (!id) return null;

  switch (kind) {
    case "flow": {
      const n = Number(id);
      return Number.isInteger(n) && n >= 0 ? { kind: "flow", id: n } : null;
    }
    case "rule":
      return { kind: "rule", id };
    case "artifact":
      // The name is used as a path component on the other side; anything that
      // could climb out of the artifacts directory is not a link.
      return /^[^/\\]+$/.test(id) && id !== "." && id !== ".." ? { kind: "artifact", id } : null;
    default:
      return null;
  }
}

/**
 * Keeps `trawl:` links intact through the markdown renderer.
 *
 * react-markdown blanks the href of any scheme outside http/https/irc/mailto/
 * xmpp, so our own links arrived with nothing to act on. Only that one scheme
 * is added: the rest of the sanitising still applies, and it has to, because
 * the text being rendered is written partly from other people's servers.
 */
export function allowTrawlLinks(url: string): string {
  return /^trawl:/i.test(url) ? url : defaultUrlTransform(url);
}

/** Shows an artifact in the file manager, for when the file is the deliverable. */
export const revealArtifact = (name: string) => invoke("reveal_artifact", { name });

/** Follows a link the user clicked in the transcript. */
export function followTrawlLink(link: TrawlLink): void {
  switch (link.kind) {
    case "flow":
      useLayout.getState().setMode("traffic");
      useFlows.getState().setView("traffic");
      useFlows.getState().select(link.id);
      return;
    case "rule":
      useLayout.getState().setMode("traffic");
      useFlows.getState().setView("rules");
      useRules.getState().select(link.id);
      return;
    case "artifact":
      void invoke("open_artifact", { name: link.id });
      return;
  }
}
