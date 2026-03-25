/**
 * Unified drag-and-drop engine for ModListEditor, AltSection, and AlternativesPanel.
 *
 * All layout measurements are taken ONCE at dragStart and cached in signals.
 * No getBoundingClientRect calls during pointermove.
 * hoveredDropId is "sticky" — never reset to null mid-drag.
 */
import { createSignal, createMemo, onMount, onCleanup, batch } from "solid-js";
import { computePreviewTranslates } from "./dragUtils";

// ── Types ────────────────────────────────────────────────────────────────────

export type DragItem =
  | { kind: "row"; id: string }
  | { kind: "group"; id: string };

export type DragEngineConfig = {
  containerRef: () => HTMLElement | undefined;
  getItems: () => DragItem[];
  onCommit: (fromId: string, dropId: string, fromKind: "row" | "group") => void;
};

const dragItemId = (item: DragItem): string =>
  item.kind === "row" ? item.id : `group:${item.id}`;

// ── Engine ───────────────────────────────────────────────────────────────────

export function useDragEngine(config: DragEngineConfig) {
  // ── Signals ──────────────────────────────────────────────────────────────
  const [draggingId,   setDraggingId]   = createSignal<string | null>(null);
  const [draggingKind, setDraggingKind] = createSignal<"row" | "group" | null>(null);
  const [hoveredDropId, setHoveredDropId] = createSignal<string | null>(null);
  const [dragPointer,  setDragPointer]  = createSignal<{ x: number; y: number } | null>(null);

  // Cached layout snapshots — taken once at dragStart
  const [cachedContainerRect, setCachedContainerRect] = createSignal<DOMRect | null>(null);
  const [cachedHeights,       setCachedHeights]       = createSignal<Map<string, number>>(new Map());
  const [cachedMidYs,         setCachedMidYs]         = createSignal<Map<string, number>>(new Map());

  const anyDragging = () => draggingId() != null;

  // ── Measurement (once at dragStart) ──────────────────────────────────────

  function measureAll() {
    const container = config.containerRef();
    if (!container) return;

    const rect = container.getBoundingClientRect();
    setCachedContainerRect(rect);

    const heights = new Map<string, number>();
    container.querySelectorAll<HTMLElement>("[data-draggable-id]").forEach(el => {
      const id = el.getAttribute("data-draggable-id")!;
      heights.set(id, el.getBoundingClientRect().height);
    });
    setCachedHeights(heights);

    const midYs = new Map<string, number>();
    container.querySelectorAll<HTMLElement>("[data-draggable-mid-id]").forEach(el => {
      const id = el.getAttribute("data-draggable-mid-id")!;
      const r = el.getBoundingClientRect();
      midYs.set(id, r.top + r.height / 2);
    });
    setCachedMidYs(midYs);
  }

  // ── Drop target detection (uses only cached measurements) ────────────────

  function detectDropTarget(cursorY: number): string | null {
    const cr = cachedContainerRect();
    if (!cr) return null;

    const items   = config.getItems();
    const heights = cachedHeights();
    const dragId  = draggingId()!;
    let y = cr.top;

    for (const item of items) {
      const id = dragItemId(item);
      const h  = heights.get(id) ?? 40; // minimal fallback, should never be needed

      // Skip the item being dragged
      if (id === dragId) { y += h; continue; }

      if (cursorY < y + h) {
        const isBottom = cursorY >= y + h / 2;
        if (item.kind === "group") {
          return isBottom ? `after:${id}` : `before:${id}`;
        }
        return isBottom ? `after:${id}` : `before:${id}`;
      }

      y += h;
    }

    // Past the end — treat as after last item
    if (items.length > 0) {
      const lastId = dragItemId(items[items.length - 1]);
      if (lastId !== dragId) return `after:${lastId}`;
      // If last item IS the dragged item, use the one before it
      if (items.length >= 2) {
        return `after:${dragItemId(items[items.length - 2])}`;
      }
    }
    return null;
  }

  // ── Preview computation (all createMemo) ─────────────────────────────────

  const previewItems = createMemo((): DragItem[] => {
    const items = config.getItems();
    const dId   = draggingId();
    const drop  = hoveredDropId();
    if (!dId || !drop) return items;

    const draggingItem = items.find(i => dragItemId(i) === dId);
    if (!draggingItem) return items;

    const rest = items.filter(i => dragItemId(i) !== dId);

    let insertIdx: number;
    if (drop.startsWith("before:")) {
      const targetId = drop.slice("before:".length);
      const idx = rest.findIndex(i => dragItemId(i) === targetId);
      insertIdx = idx < 0 ? rest.length : idx;
    } else if (drop.startsWith("after:")) {
      const targetId = drop.slice("after:".length);
      const idx = rest.findIndex(i => dragItemId(i) === targetId);
      insertIdx = idx < 0 ? rest.length : idx + 1;
    } else {
      // Custom drop IDs (group-drop, etc.) — handled by extended engines
      return items;
    }

    const result = [...rest];
    result.splice(insertIdx, 0, draggingItem);
    return result;
  });

  const previewTranslates = createMemo((): Map<string, number> => {
    const dId  = draggingId();
    const drop = hoveredDropId();
    if (!dId || !drop) return new Map();

    const items   = config.getItems();
    const preview = previewItems();
    return computePreviewTranslates(
      items.map(dragItemId),
      preview.map(dragItemId),
      cachedHeights(),
      0  // no fallback — all heights must be measured
    );
  });

  // ── Lifecycle ────────────────────────────────────────────────────────────

  function startDrag(id: string, kind: "row" | "group", event: PointerEvent | MouseEvent) {
    event.preventDefault();
    event.stopPropagation();

    measureAll();

    batch(() => {
      setDraggingId(id);
      setDraggingKind(kind);
      setHoveredDropId(null);
      setDragPointer({ x: event.clientX, y: event.clientY });
    });
    document.body.style.userSelect = "none";
  }

  function finishDrag() {
    const fromId = draggingId();
    const dropId = hoveredDropId();
    const kind   = draggingKind();
    if (fromId && dropId && kind) {
      config.onCommit(fromId, dropId, kind);
    }
    batch(() => {
      setDraggingId(null);
      setDraggingKind(null);
      setHoveredDropId(null);
      setDragPointer(null);
      setCachedContainerRect(null);
      setCachedHeights(new Map());
      setCachedMidYs(new Map());
    });
    document.body.style.userSelect = "";
  }

  onMount(() => {
    const onMove = (event: PointerEvent) => {
      if (!draggingId()) return;
      setDragPointer({ x: event.clientX, y: event.clientY });
      const newTarget = detectDropTarget(event.clientY);
      // Sticky: only update if we got a valid new target
      if (newTarget !== null) {
        setHoveredDropId(newTarget);
      }
      // If null, keep the last non-null value (sticky behavior — Bug 2 fix)
    };
    const onUp = () => {
      if (draggingId()) finishDrag();
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup",   onUp);
    onCleanup(() => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup",   onUp);
      document.body.style.userSelect = "";
    });
  });

  return {
    startDrag,
    draggingId,
    draggingKind,
    hoveredDropId,
    dragPointer,
    anyDragging,
    previewTranslates,
    previewItems,
    // Expose cached measurements for advanced detection in components
    cachedHeights,
    cachedMidYs,
    cachedContainerRect,
    // Allow components to override detection (for groups with inner rows, alt groups, etc.)
    setHoveredDropId,
  };
}
