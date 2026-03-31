/**
 * ModListEditor — scrollable mod rule list with top-level drag-and-drop.
 *
 * Uses the unified useDragEngine for all drag operations.
 * Drop-target detection is fully position-based (no elementFromPoint).
 * Alt drag-and-drop is handled per-row inside AltSection.
 */
import { For, Show, createMemo, batch } from "solid-js";
import type { ModRow } from "../lib/types";
import type { TopLevelItem } from "../store";
import { appendDebugTrace } from "../lib/debugTrace";
import {
  topLevelItems, modListCards, selectedModListName, search, modRowsState, setModRowsState,
  aestheticGroups, setAestheticGroups, activeAccount, modIcons,
  editingGroupId, groupNameDraft, setGroupNameDraft,
  toggleGroupCollapsed, startGroupRename, commitGroupRename, removeAestheticGroup,
} from "../store";
import { ActionBar } from "./ActionBar";
import { ModRuleItem } from "./ModRuleItem";
import {
  PackageIcon, ChevronDownIcon, ChevronRightIcon, FolderOpenIcon,
  PencilIcon, ExternalLinkIcon, XIcon, MaterialIcon,
} from "./icons";
import { setInstancePresentationOpen, setExportModalOpen, instancePresentation } from "../store";
import { useDragEngine, type DragItem } from "../lib/dragEngine";
import { computePreviewTranslates } from "../lib/dragUtils";

// ── ID helper ─────────────────────────────────────────────────────────────────
const tlId = (item: TopLevelItem) => item.kind === "row" ? item.row.id : `group:${item.id}`;

// ── Group header ──────────────────────────────────────────────────────────────
function GroupHeader(props: {
  groupId: string;
  name: string;
  blockCount: number;
  collapsed: boolean;
  onStartDrag: (e: PointerEvent) => void;
}) {
  const editing = () => editingGroupId() === props.groupId;
  return (
    <div class="flex flex-1 items-center gap-2 min-w-0">
      <button
        onClick={() => toggleGroupCollapsed(props.groupId)}
        class="flex items-center text-sm font-medium text-muted-foreground transition-colors hover:text-foreground"
        title={props.collapsed ? "Expand group" : "Collapse group"}
      >
        <Show when={props.collapsed} fallback={<ChevronDownIcon class="h-4 w-4" />}>
          <ChevronRightIcon class="h-4 w-4" />
        </Show>
      </button>
      <div class="cursor-grab touch-none" onPointerDown={props.onStartDrag} title="Drag to reorder group">
        <FolderOpenIcon class="h-4 w-4 text-primary" />
      </div>
      <Show
        when={editing()}
        fallback={
          <span
            class="flex-1 cursor-pointer text-sm font-medium text-foreground"
            onClick={() => startGroupRename(props.groupId, props.name)}
          >
            {props.name}
          </span>
        }
      >
        <input
          type="text"
          value={groupNameDraft()}
          onInput={e => setGroupNameDraft(e.currentTarget.value)}
          onBlur={() => commitGroupRename(props.groupId)}
          onKeyDown={e => {
            if (e.key === "Enter" || e.key === "Escape") commitGroupRename(props.groupId);
          }}
          class="flex-1 rounded bg-transparent text-sm font-medium text-foreground outline-none"
          autofocus
        />
      </Show>
      <span class="shrink-0 text-xs text-muted-foreground">{props.blockCount} mods</span>
      <button
        onClick={() => removeAestheticGroup(props.groupId)}
        class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
        title="Remove group"
      >
        <XIcon class="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

// ── Empty state ───────────────────────────────────────────────────────────────
function EmptyState(props: { onAddMod: () => void }) {
  return (
    <div class="flex flex-col items-center justify-center py-16 text-center">
      <div class="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-muted">
        <PackageIcon class="h-8 w-8 text-muted-foreground" />
      </div>
      <h3 class="mb-2 text-lg font-semibold text-foreground">Empty Mod List</h3>
      <p class="mb-6 max-w-xs text-sm text-muted-foreground">
        Add mods from Modrinth or upload local JAR files.
      </p>
      <button
        onClick={props.onAddMod}
        class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90"
      >
        Add Your First Mod
      </button>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────
interface Props {
  onAddMod: () => void;
  onDeleteSelected: () => void;
  onReorder: (orderedIds: string[]) => void;
  onReorderAlts?: (parentId: string, orderedIds: string[]) => void;
}

export function ModListEditor(props: Props) {
  let listContainerRef: HTMLDivElement | undefined;

  const activeModList   = () => modListCards().find(m => m.name === selectedModListName());
  const hasContent      = () => modRowsState().length > 0;

  // ── Drag engine ──────────────────────────────────────────────────────────
  const engine = useDragEngine({
    containerRef: () => listContainerRef,
    getItems: () => topLevelItems().map((item): DragItem =>
      item.kind === "row"
        ? { kind: "row", id: item.row.id }
        : { kind: "group", id: item.id }
    ),
    onCommit: (fromId, dropId, fromKind) => commitDrop(fromId, dropId, fromKind),
  });

  const isDraggingGroup = () => engine.draggingKind() === "group";

  const draggingRow = () => {
    const id = engine.draggingId();
    if (!id || engine.draggingKind() === "group") return null;
    return modRowsState().find(r => r.id === id) ?? null;
  };
  const draggingGroupItem = () => {
    const id = engine.draggingId();
    if (!id || engine.draggingKind() !== "group") return null;
    const found = topLevelItems().find(i => i.kind === "group" && i.id === id);
    return found?.kind === "group" ? found : null;
  };
  const draggingRowIcon = () => {
    const dr = draggingRow();
    return dr?.modrinth_id ? modIcons().get(dr.modrinth_id) : undefined;
  };

  // ── Extended drop target detection ──────────────────────────────────────
  // The base engine uses simple before:/after: IDs. For ModListEditor we need
  // richer semantics: group-drop (row dropped INTO a group), and within-group
  // row targeting. We override the engine's setHoveredDropId during pointermove
  // via a custom detection that runs on top of cached measurements.

  // Override: the engine calls its own detectDropTarget which sets hoveredDropId.
  // We intercept by providing our own richer detection. We do this by replacing
  // the engine's internal onMove — but the engine doesn't expose that.
  // Instead, we use the engine's setHoveredDropId + cachedContainerRect/cachedHeights.

  // Actually, the cleanest approach: we let the engine handle pointer tracking
  // but override the drop target on each move via an effect-like approach.
  // The engine already exposes setHoveredDropId. We'll wrap startDrag.

  // For simplicity: we use the engine for state management and preview math,
  // but override the detection. We achieve this by providing a custom getItems
  // that includes inner-group rows as part of the item list for measurement,
  // and then custom detection.

  // The detection logic needs to handle:
  // 1. Row dragged before/after another row (simple)
  // 2. Row dragged before/after a group (tl-group:/tl-group-after:)
  // 3. Row dragged INTO a group (group-drop:)
  // 4. Row dragged before/after an inner-group row
  // 5. Group dragged before/after a row or group

  // We replicate the original detection using the engine's cached data:

  const detectEnhancedDropTarget = (cursorY: number): string | null => {
    const cr = engine.cachedContainerRect();
    if (!cr) return null;

    const items   = topLevelItems();
    const heights = engine.cachedHeights();
    const dragId  = engine.draggingId()!;
    const isGroupDrag = isDraggingGroup();
    // For group drag, the draggingId is the raw group id (no "group:" prefix from engine)
    // but in topLevelItems, groups have tlId = "group:{id}"
    const dragTlId = isGroupDrag ? `group:${dragId}` : dragId;
    let y = cr.top;

    for (const item of items) {
      const id = tlId(item);
      const h  = heights.get(id) ?? 40;

      if (id === dragTlId) { y += h; continue; }

      if (cursorY < y + h) {
        if (item.kind === "group") {
          const gId  = item.id;
          const relY = cursorY - y;

          if (isGroupDrag) {
            return cursorY >= y + h / 2 ? `tl-group-after:${gId}` : `tl-group:${gId}`;
          }
          const draggedIsGroupMember = groupByRowId().get(dragId) === gId;
          if (!draggedIsGroupMember && relY < h * 0.25) return `tl-group:${gId}`;
          if (!draggedIsGroupMember && relY > h * 0.75) return `tl-group-after:${gId}`;

          // Middle zone: find exact target row inside the group using cached midYs
          const midYs = engine.cachedMidYs();
          const candidates = item.blocks.filter(b => b.id !== dragTlId);
          if (candidates.length === 0) return `group-drop:${gId}`;
          let nearest = candidates[0];
          for (const c of candidates.slice(1)) {
            const cMid = midYs.get(c.id) ?? Infinity;
            const nMid = midYs.get(nearest.id) ?? Infinity;
            if (Math.abs(cursorY - cMid) < Math.abs(cursorY - nMid)) nearest = c;
          }
          const nearestMid = midYs.get(nearest.id) ?? (y + h / 2);
          return cursorY >= nearestMid ? `row-after:${nearest.id}` : nearest.id;
        }

        if (isGroupDrag) {
          const rowId = item.row.id;
          return cursorY >= y + h / 2 ? `row-after:${rowId}` : rowId;
        }

        // Row item — simple before/after
        const rowId = item.row.id;
        return cursorY >= y + h / 2 ? `row-after:${rowId}` : rowId;
      }

      y += h;
    }
    return null;
  };

  // Wrap the engine's startDrag to also install our custom detection
  const handleStartDrag = (id: string, kind: "row" | "group", event: PointerEvent | MouseEvent) => {
    engine.startDrag(id, kind, event);

    // Install a custom pointermove handler that overrides the engine's detection
    const onMoveOverride = (ev: PointerEvent) => {
      if (!engine.draggingId()) return;
      const target = detectEnhancedDropTarget(ev.clientY);
      if (target !== null) {
        engine.setHoveredDropId(target);
      }
    };
    const onUpCleanup = () => {
      window.removeEventListener("pointermove", onMoveOverride);
      window.removeEventListener("pointerup", onUpCleanup);
    };
    window.addEventListener("pointermove", onMoveOverride);
    window.addEventListener("pointerup", onUpCleanup);
  };

  // ── Preview order (TL items) ───────────────────────────────────────────────
  const previewTLItems = createMemo(() => {
    const items    = topLevelItems();
    const dragging = engine.draggingId();
    const drop     = engine.hoveredDropId();
    if (!dragging || !drop) return items;
    if (drop.startsWith("group-drop:")) return items;

    const dragTlId = isDraggingGroup() ? `group:${dragging}` : dragging;
    const draggingItem = items.find(i => tlId(i) === dragTlId);
    if (!draggingItem) return items;

    const rest = items.filter(i => tlId(i) !== dragTlId);
    let insertIdx: number;

    if (drop.startsWith("tl-group-after:")) {
      const gId = drop.slice("tl-group-after:".length);
      const idx = rest.findIndex(i => i.kind === "group" && i.id === gId);
      insertIdx = idx < 0 ? rest.length : idx + 1;
    } else if (drop.startsWith("tl-group:")) {
      const gId = drop.slice("tl-group:".length);
      const idx = rest.findIndex(i => i.kind === "group" && i.id === gId);
      insertIdx = idx < 0 ? rest.length : idx;
    } else if (drop.startsWith("row-after:")) {
      const rid = drop.slice("row-after:".length);
      const idx = rest.findIndex(i => i.kind === "row" && i.row.id === rid);
      insertIdx = idx < 0 ? rest.length : idx + 1;
    } else {
      const idx = rest.findIndex(i => i.kind === "row" && i.row.id === drop);
      insertIdx = idx < 0 ? rest.length : idx;
    }

    const result = [...rest];
    result.splice(insertIdx, 0, draggingItem);
    return result;
  });

  // ── Per-item translateY offsets ────────────────────────────────────────────
  const previewTranslates = createMemo(() => {
    if (!engine.draggingId() || !engine.hoveredDropId() || engine.hoveredDropId()?.startsWith("group-drop:")) {
      return new Map<string, number>();
    }
    const items   = topLevelItems();
    const preview = previewTLItems();
    return computePreviewTranslates(
      items.map(tlId),
      preview.map(tlId),
      engine.cachedHeights(),
      0
    );
  });

  // ── Within-group row preview order ────────────────────────────────────────
  const previewGroupRowTranslates = createMemo(() => {
    const map = new Map<string, number>();
    const dragging = engine.draggingId();
    const drop     = engine.hoveredDropId();
    if (!dragging || !drop) return map;

    for (const item of topLevelItems()) {
      if (item.kind !== "group") continue;
      const ids = item.blocks.map(r => r.id);
      if (!ids.includes(dragging)) continue;

      let insertIdx: number;
      if (drop.startsWith("row-after:")) {
        const ai = ids.indexOf(drop.slice("row-after:".length));
        if (ai < 0) continue;
        insertIdx = ai + 1;
      } else if (ids.includes(drop)) {
        insertIdx = ids.indexOf(drop);
      } else {
        continue;
      }

      const fromIdx = ids.indexOf(dragging);
      const next    = [...ids];
      next.splice(fromIdx, 1);
      next.splice(insertIdx > fromIdx ? insertIdx - 1 : insertIdx, 0, dragging);

      // Compute translateY for each row using measured heights
      const heights = engine.cachedHeights();
      const translates = computePreviewTranslates(ids, next, heights, 0);
      for (const [id, offset] of translates) {
        map.set(id, offset);
      }
    }
    return map;
  });

  // ── Helpers ────────────────────────────────────────────────────────────────
  const groupByRowId = () => {
    const map = new Map<string, string>();
    for (const g of aestheticGroups()) for (const id of g.blockIds) map.set(id, g.id);
    return map;
  };
  const removeFromArr = (arr: string[], id: string) => arr.filter(x => x !== id);

  // ── Commit group move ──────────────────────────────────────────────────────
  const commitGroupMove = (gId: string, dropId: string) => {
    const group = aestheticGroups().find(g => g.id === gId);
    if (!group) return;
    const groupRowIds = new Set(group.blockIds);
    const groupRows   = group.blockIds
      .map(id => modRowsState().find(r => r.id === id))
      .filter((r): r is ModRow => Boolean(r));
    const otherRows   = modRowsState().filter(r => !groupRowIds.has(r.id));

    let insertIdx: number;
    if (dropId.startsWith("tl-group:")) {
      const tgId  = dropId.slice("tl-group:".length);
      const tg    = aestheticGroups().find(g => g.id === tgId);
      const first = tg?.blockIds[0];
      insertIdx   = first ? otherRows.findIndex(r => r.id === first) : otherRows.length;
      if (insertIdx < 0) insertIdx = otherRows.length;
    } else if (dropId.startsWith("tl-group-after:")) {
      const tgId = dropId.slice("tl-group-after:".length);
      const tg   = aestheticGroups().find(g => g.id === tgId);
      let last = -1;
      if (tg) for (let i = otherRows.length - 1; i >= 0; i--) {
        if (tg.blockIds.includes(otherRows[i].id)) { last = i; break; }
      }
      insertIdx = last < 0 ? otherRows.length : last + 1;
    } else if (dropId.startsWith("row-after:")) {
      const rid = dropId.slice("row-after:".length);
      const idx = otherRows.findIndex(r => r.id === rid);
      insertIdx = idx < 0 ? otherRows.length : idx + 1;
    } else {
      const idx = otherRows.findIndex(r => r.id === dropId);
      insertIdx = idx < 0 ? otherRows.length : idx;
    }

    const newRows = [...otherRows];
    newRows.splice(insertIdx, 0, ...groupRows);
    batch(() => { setModRowsState(newRows); });
    props.onReorder(newRows.map(r => r.id));
  };

  // ── Commit row move ────────────────────────────────────────────────────────
  const commitRowMove = (fromId: string, dropId: string) => {
    const groups     = aestheticGroups().map(g => ({ ...g, blockIds: [...g.blockIds] }));
    const membership = groupByRowId();
    const srcGroupId = membership.get(fromId) ?? null;

    if (srcGroupId) {
      const sg = groups.find(g => g.id === srcGroupId);
      if (sg) sg.blockIds = removeFromArr(sg.blockIds, fromId);
    }

    if (dropId.startsWith("group-drop:")) {
      const tgId = dropId.slice("group-drop:".length);
      const tg   = groups.find(g => g.id === tgId);
      if (tg && !tg.blockIds.includes(fromId)) tg.blockIds.push(fromId);

      const row    = modRowsState().find(r => r.id === fromId)!;
      const others = modRowsState().filter(r => r.id !== fromId);
      let lastIdx  = -1;
      if (tg) for (let i = others.length - 1; i >= 0; i--) {
        if (tg.blockIds.includes(others[i].id)) { lastIdx = i; break; }
      }
      const insertIdx = lastIdx < 0 ? others.length : lastIdx + 1;
      const newRows   = [...others];
      newRows.splice(insertIdx, 0, row);
      batch(() => {
        setAestheticGroups(groups.filter(g => g.blockIds.length > 0));
        setModRowsState(newRows);
      });
      props.onReorder(newRows.map(r => r.id));
      return;
    }

    // If dropping on/after a row that belongs to a group, add fromId to that group
    {
      let targetRowId: string | null = null;
      if (dropId.startsWith("row-after:")) {
        targetRowId = dropId.slice("row-after:".length);
      } else if (!dropId.startsWith("tl-group:") && !dropId.startsWith("tl-group-after:")) {
        targetRowId = dropId;
      }
      if (targetRowId) {
        const tgtGroupId = membership.get(targetRowId);
        if (tgtGroupId) {
          const tg = groups.find(g => g.id === tgtGroupId);
          if (tg && !tg.blockIds.includes(fromId)) {
            const tidx = tg.blockIds.indexOf(targetRowId);
            if (dropId.startsWith("row-after:")) {
              tg.blockIds.splice(tidx < 0 ? tg.blockIds.length : tidx + 1, 0, fromId);
            } else {
              tg.blockIds.splice(tidx < 0 ? 0 : tidx, 0, fromId);
            }
          }
        }
      }
    }

    const row    = modRowsState().find(r => r.id === fromId)!;
    const others = modRowsState().filter(r => r.id !== fromId);
    const filteredGroups = groups.filter(g => g.blockIds.length > 0);

    let insertIdx: number;
    if (dropId.startsWith("tl-group:")) {
      const tgId  = dropId.slice("tl-group:".length);
      const tg    = filteredGroups.find(g => g.id === tgId);
      const first = tg?.blockIds[0];
      insertIdx   = first ? others.findIndex(r => r.id === first) : others.length;
      if (insertIdx < 0) insertIdx = others.length;
    } else if (dropId.startsWith("tl-group-after:")) {
      const tgId = dropId.slice("tl-group-after:".length);
      const tg   = filteredGroups.find(g => g.id === tgId);
      let last = -1;
      if (tg) for (let i = others.length - 1; i >= 0; i--) {
        if (tg.blockIds.includes(others[i].id)) { last = i; break; }
      }
      insertIdx = last < 0 ? others.length : last + 1;
    } else if (dropId.startsWith("row-after:")) {
      const rid = dropId.slice("row-after:".length);
      const idx = others.findIndex(r => r.id === rid);
      insertIdx = idx < 0 ? others.length : idx + 1;
    } else {
      const idx = others.findIndex(r => r.id === dropId);
      insertIdx = idx < 0 ? others.length : idx;
    }

    const newRows = [...others];
    newRows.splice(insertIdx, 0, row);
    batch(() => {
      setAestheticGroups(filteredGroups);
      setModRowsState(newRows);
    });
    props.onReorder(newRows.map(r => r.id));
  };

  // ── Commit drop ────────────────────────────────────────────────────────────
  const commitDrop = (fromId: string, dropId: string, fromKind: "row" | "group") => {
    if (fromId === dropId) return;
    appendDebugTrace("groups.drag.frontend", { phase: "drop", draggableId: fromId, droppableId: dropId });
    if (fromKind === "group") {
      commitGroupMove(fromId, dropId);
    } else {
      commitRowMove(fromId, dropId);
    }
  };

  // ── Render ─────────────────────────────────────────────────────────────────
  return (
    <div class="flex flex-1 flex-col overflow-hidden">
      <Show
        when={activeModList()}
        fallback={
          <div class="flex flex-1 items-center justify-center">
            <div class="flex flex-col items-center text-center">
              <div class="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-muted">
                <PackageIcon class="h-8 w-8 text-muted-foreground" />
              </div>
              <h3 class="text-lg font-semibold text-foreground">No Mod List Selected</h3>
              <p class="mt-1 text-sm text-muted-foreground">
                Select a mod list from the sidebar or create a new one.
              </p>
            </div>
          </div>
        }
      >
        <ActionBar onAddMod={props.onAddMod} onDeleteSelected={props.onDeleteSelected} />

        {/* Mod list header */}
        <div class="shrink-0 border-b border-border bg-card/50 px-4 py-3">
          <div class="flex items-start justify-between gap-4">
            <div class="min-w-0 flex-1">
              <h2 class="text-lg font-semibold text-foreground">
                {activeModList()!.name}
              </h2>
              <p class="text-sm text-muted-foreground">{activeModList()!.description}</p>
              <div class="mt-1.5 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                <span>{modRowsState().length} rule{modRowsState().length !== 1 ? "s" : ""}</span>
                <span>·</span>
                <span>by {instancePresentation().iconAccent || activeAccount()?.gamertag || "—"}</span>
              </div>
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <button
                onClick={() => setInstancePresentationOpen(true)}
                class="flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                title="Edit settings for this mod list"
              >
                <PencilIcon class="h-3.5 w-3.5" />
                Settings
              </button>
              <button
                onClick={() => {
                  // Import: open file picker for rules.json, then reload
                  void (async () => {
                    try {
                      const { open } = await import("@tauri-apps/plugin-dialog");
                      const selected = await open({ title: "Import Mod List (rules.json)", filters: [{ name: "JSON", extensions: ["json"] }], multiple: false });
                      if (!selected) return;
                      const { invoke } = await import("@tauri-apps/api/core");
                      // Read the file and copy it over the current modlist's rules.json
                      const modlistName = selectedModListName();
                      if (!modlistName) return;
                      await invoke("import_modlist_command", { modlistName, sourcePath: selected as string });
                      window.location.reload();
                    } catch { /* cancelled or error */ }
                  })();
                }}
                class="flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                title="Import rules from a JSON file"
              >
                <MaterialIcon name="download" size="sm" />
                Import
              </button>
              <button
                onClick={() => setExportModalOpen(true)}
                class="flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                title="Export this mod list"
              >
                <ExternalLinkIcon class="h-3.5 w-3.5" />
                Export
              </button>
            </div>
          </div>
        </div>

        {/* Scrollable mod list */}
        <div class="flex-1 p-4" style={{ overflow: engine.anyDragging() ? "visible" : "auto" }}>
          <Show when={hasContent()} fallback={<EmptyState onAddMod={props.onAddMod} />}>
            <>
              {/* Drag ghost */}
              <Show when={engine.draggingId() && engine.dragPointer()}>
                <div
                  class="pointer-events-none fixed z-50 w-80 rounded-md border border-primary/40 bg-card shadow-2xl ring-1 ring-primary/20"
                  style={{ left: `${engine.dragPointer()!.x + 12}px`, top: `${engine.dragPointer()!.y - 24}px`, opacity: "0.95" }}
                >
                  <Show
                    when={isDraggingGroup() && draggingGroupItem()}
                    fallback={
                      <div class="flex items-center gap-3 px-2 py-2">
                        <div class="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted">
                          <Show when={draggingRowIcon()} fallback={<PackageIcon class="h-4 w-4 text-muted-foreground" />}>
                            <img src={draggingRowIcon()!} alt={draggingRow()?.name} class="h-8 w-8 object-cover" />
                          </Show>
                        </div>
                        <div class="min-w-0 flex-1">
                          <span class="truncate font-medium text-foreground">{draggingRow()?.name}</span>
                        </div>
                      </div>
                    }
                  >
                    <div class="flex items-center gap-3 px-3 py-2">
                      <FolderOpenIcon class="h-5 w-5 shrink-0 text-primary" />
                      <div class="min-w-0 flex-1">
                        <div class="truncate font-medium text-foreground">{draggingGroupItem()!.name}</div>
                        <div class="text-xs text-muted-foreground">{draggingGroupItem()!.blocks.length} mods</div>
                      </div>
                    </div>
                  </Show>
                </div>
              </Show>

              {/* Unified list */}
              <div class="space-y-0.5" ref={listContainerRef}>
                <For each={topLevelItems()}>
                  {(item, index) => {
                    const id         = tlId(item);
                    const isDragging = () => {
                      const dId = engine.draggingId();
                      if (!dId) return false;
                      return isDraggingGroup() ? `group:${dId}` === id : dId === id;
                    };
                    const offset     = () => isDragging() ? 0 : (previewTranslates().get(id) ?? 0);

                    if (item.kind === "row") {
                      return (
                        <div
                          data-draggable-id={id}
                          data-draggable-mid-id={id}
                          style={{
                            transform:  engine.anyDragging() ? `translateY(${offset()}px)` : "none",
                            transition: engine.anyDragging() ? "transform 150ms ease" : "none",
                            position:   "relative",
                            "z-index":  isDragging() ? "0" : "1",
                          }}
                          class={isDragging() ? "opacity-0 pointer-events-none" : ""}
                        >
                          <div
                            class={engine.hoveredDropId() === item.row.id ? "rounded-md ring-1 ring-primary/40" : ""}
                          >
                            <ModRuleItem
                              row={item.row}
                              onStartDrag={(rowId, e) => handleStartDrag(rowId, "row", e)}
                              onReorderAlts={(parentId, orderedIds) => props.onReorderAlts?.(parentId, orderedIds)}
                            />
                          </div>
                        </div>
                      );
                    }

                    // Group item
                    const group          = item;
                    const isGroupDrop    = () => engine.hoveredDropId() === `group-drop:${group.id}`;
                    const isBeforeTarget = () => engine.hoveredDropId() === `tl-group:${group.id}`;
                    const isAfterTarget  = () => engine.hoveredDropId() === `tl-group-after:${group.id}`;

                    return (
                      <div
                        data-draggable-id={id}
                        style={{
                          transform:  engine.anyDragging() ? `translateY(${offset()}px)` : "none",
                          transition: engine.anyDragging() ? "transform 150ms ease" : "none",
                          position:   "relative",
                          "z-index":  isDragging() ? "0" : "1",
                        }}
                        class={`mb-2 ${isDragging() ? "opacity-0 pointer-events-none" : ""}`}
                      >
                        <Show when={isBeforeTarget()}>
                          <div class="mb-1 h-0.5 rounded bg-primary" />
                        </Show>

                        <div class={`rounded-xl border p-2 shadow-sm transition-colors ${
                          isGroupDrop() ? "border-primary/40 bg-primary/5" : "border-border/70 bg-muted/10"
                        }`}>
                          <div class="flex items-center gap-1 px-1 py-1">
                            <GroupHeader
                              groupId={group.id}
                              name={group.name}
                              blockCount={group.blocks.length}
                              collapsed={group.collapsed}
                              onStartDrag={(e) => handleStartDrag(group.id, "group", e)}
                            />
                          </div>

                          <Show when={!group.collapsed}>
                            <div class="mt-1 space-y-1">
                              <For each={group.blocks}>
                                {(row) => {
                                  const isDraggingRow = () => engine.draggingId() === row.id && engine.draggingKind() === "row";
                                  const rowOffset     = () => isDraggingRow() ? 0 : (previewGroupRowTranslates().get(row.id) ?? 0);
                                  const isTarget      = () =>
                                    !isDraggingRow() && (
                                      engine.hoveredDropId() === row.id ||
                                      engine.hoveredDropId() === `row-after:${row.id}`
                                    );

                                  return (
                                    <div
                                      data-draggable-id={row.id}
                                      data-draggable-mid-id={row.id}
                                      style={{
                                        transform:  engine.anyDragging() ? `translateY(${rowOffset()}px)` : "none",
                                        transition: engine.anyDragging() ? "transform 150ms ease" : "none",
                                        position:   "relative",
                                        "z-index":  isDraggingRow() ? "0" : "1",
                                      }}
                                      class={isDraggingRow() ? "opacity-0 pointer-events-none" : isTarget() ? "rounded-md ring-1 ring-primary/40" : ""}
                                    >
                                      <ModRuleItem
                                        row={row}
                                        onStartDrag={(rowId, e) => handleStartDrag(rowId, "row", e)}
                                        onReorderAlts={(parentId, orderedIds) => props.onReorderAlts?.(parentId, orderedIds)}
                                      />
                                    </div>
                                  );
                                }}
                              </For>
                              <Show when={group.blocks.length === 0}>
                                <div class="rounded-md border border-dashed border-border bg-background/40 px-4 py-4 text-center text-sm text-muted-foreground">
                                  Drag mods here to place them in this group.
                                </div>
                              </Show>
                            </div>
                          </Show>
                        </div>

                        <Show when={isAfterTarget()}>
                          <div class="mt-1 h-0.5 rounded bg-primary" />
                        </Show>
                      </div>
                    );
                  }}
                </For>
              </div>

              <Show when={search() && topLevelItems().length === 0}>
                <div class="py-12 text-center">
                  <p class="text-muted-foreground">No mods found matching "{search()}"</p>
                </div>
              </Show>
            </>
          </Show>
        </div>
      </Show>
    </div>
  );
}
