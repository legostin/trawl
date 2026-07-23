import type { Flow } from "./types";

export interface TreeLeaf {
  kind: "leaf";
  key: string;
  flowId: number;
  method: string;
  status: number | undefined;
}

export interface TreeBranch {
  kind: "branch";
  key: string;
  label: string;
  count: number;
  children: TreeBranch[];
  leaves: TreeLeaf[];
}

interface BuildBranch {
  key: string;
  label: string;
  children: Map<string, BuildBranch>;
  leaves: TreeLeaf[];
}

function mkBuild(key: string, label: string): BuildBranch {
  return { key, label, children: new Map(), leaves: [] };
}

function convert(b: BuildBranch): TreeBranch {
  const children = [...b.children.values()]
    .sort((x, y) => x.label.localeCompare(y.label))
    .map(convert);
  const leaves = [...b.leaves].sort((x, y) => x.flowId - y.flowId);
  const count = leaves.length + children.reduce((n, c) => n + c.count, 0);
  return { kind: "branch", key: b.key, label: b.label, count, children, leaves };
}

/**
 * Builds a tree: host → path segments → leaf request.
 * Each flow becomes a leaf on the terminal segment of its path.
 */
export function buildDomainTree(flows: Flow[]): TreeBranch[] {
  const hosts = new Map<string, BuildBranch>();

  for (const f of flows) {
    const host = f.url.host || "(no host)";
    let hb = hosts.get(host);
    if (!hb) {
      hb = mkBuild(host, host);
      hosts.set(host, hb);
    }

    const path = f.url.path.split("?")[0];
    const segs = path.split("/").filter(Boolean);
    let cur = hb;
    for (const seg of segs) {
      const key = `${cur.key}/${seg}`;
      let ch = cur.children.get(key);
      if (!ch) {
        ch = mkBuild(key, seg);
        cur.children.set(key, ch);
      }
      cur = ch;
    }
    cur.leaves.push({
      kind: "leaf",
      key: `leaf-${f.id}`,
      flowId: f.id,
      method: f.method,
      status: f.response?.status,
    });
  }

  return [...hosts.values()].sort((x, y) => x.label.localeCompare(y.label)).map(convert);
}
