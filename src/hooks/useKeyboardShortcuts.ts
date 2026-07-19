import { useEffect } from "react";
import { useFlows } from "../store";
import { visibleFlows } from "../filter";

/** Глобальные горячие клавиши: навигация по списку, фокус поиска, деселект, сворачивание деталей. */
export function useKeyboardShortcuts() {
  const flows = useFlows((s) => s.flows);
  const filter = useFlows((s) => s.filter);
  const selectedId = useFlows((s) => s.selectedId);
  const select = useFlows((s) => s.select);
  const setFilter = useFlows((s) => s.setFilter);
  const toggleDetail = useFlows((s) => s.toggleDetail);
  const view = useFlows((s) => s.view);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const typing =
        !!target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);

      // Фокус поиска: "/" или ⌘K / Ctrl+K
      if ((e.key === "/" && !typing) || (e.key.toLowerCase() === "k" && (e.metaKey || e.ctrlKey))) {
        e.preventDefault();
        document.querySelector<HTMLInputElement>("[data-search-input]")?.focus();
        return;
      }

      // Свернуть/показать детали: ⌘\ / Ctrl+\
      if ((e.metaKey || e.ctrlKey) && e.key === "\\") {
        e.preventDefault();
        toggleDetail();
        return;
      }

      if (e.key === "Escape") {
        if (typing) {
          if (filter.query) setFilter({ query: "" });
          (target as HTMLElement).blur();
        } else {
          select(null);
        }
        return;
      }

      if (typing || view !== "traffic") return;

      const isDown = e.key === "ArrowDown" || e.key === "j";
      const isUp = e.key === "ArrowUp" || e.key === "k";
      if (!isDown && !isUp) return;

      const list = visibleFlows(flows, filter);
      if (list.length === 0) return;
      e.preventDefault();
      const idx = list.findIndex((f) => f.id === selectedId);
      const dir = isDown ? 1 : -1;
      let next = idx === -1 ? (isDown ? 0 : list.length - 1) : idx + dir;
      next = Math.max(0, Math.min(list.length - 1, next));
      select(list[next].id);
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [flows, filter, selectedId, view, select, setFilter, toggleDetail]);
}
