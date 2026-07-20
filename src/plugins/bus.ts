type Handler = (payload: unknown) => void;

/** Minimal typed pub/sub for host↔plugin (and plugin↔plugin) communication. */
export class EventBus {
  private map = new Map<string, Set<Handler>>();

  on(type: string, cb: Handler): () => void {
    let set = this.map.get(type);
    if (!set) {
      set = new Set();
      this.map.set(type, set);
    }
    set.add(cb);
    return () => this.off(type, cb);
  }

  off(type: string, cb: Handler): void {
    this.map.get(type)?.delete(cb);
  }

  emit(type: string, payload?: unknown): void {
    this.map.get(type)?.forEach((h) => {
      try {
        h(payload);
      } catch (e) {
        console.error(`[trawl] plugin handler for "${type}" threw`, e);
      }
    });
  }
}

/** Shared bus instance used by the host and bridged from app state. */
export const bus = new EventBus();
