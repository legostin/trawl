type Handler = (payload: unknown) => void;

/** A single documented field of an event's payload (for the subscription UI). */
export interface EventParam {
  name: string;
  type: string;
  doc?: string;
}

export interface EventMeta {
  description?: string;
  /** TS type expression for the payload, e.g. "{ a: number }" — fed to Monaco. */
  payloadType?: string;
  /** Self-reported origin ("core" or a plugin id). */
  source?: string;
  /** Documented payload fields, for richer subscription-UI hints than payloadType alone. */
  params?: EventParam[];
}

export interface EventInfo extends EventMeta {
  type: string;
  /** Last payload observed for this event (undefined until first emit). */
  lastPayload?: unknown;
}

/** Minimal typed pub/sub for host↔plugin (and plugin↔plugin) communication. */
export class EventBus {
  private map = new Map<string, Set<Handler>>();
  private meta = new Map<string, EventMeta>();
  private last = new Map<string, unknown>();

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
    this.last.set(type, payload);
    this.map.get(type)?.forEach((h) => {
      try {
        h(payload);
      } catch (e) {
        console.error(`[trawl] plugin handler for "${type}" threw`, e);
      }
    });
  }

  /** Declare an event and its payload type (for the subscription UI + hints). */
  describe(type: string, meta: EventMeta): void {
    this.meta.set(type, meta);
  }

  /** Declared and observed events, sorted by type. */
  known(): EventInfo[] {
    const types = new Set([...this.meta.keys(), ...this.last.keys()]);
    return [...types]
      .sort()
      .map((type) => ({ ...this.meta.get(type), type, lastPayload: this.last.get(type) }));
  }
}

/** Shared bus instance used by the host and bridged from app state. */
export const bus = new EventBus();
