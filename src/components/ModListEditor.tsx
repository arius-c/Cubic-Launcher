import { For, Show, createMemo, createSignal, onCleanup, onMount, batch } from "solid-js";
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
  PencilIcon, ExternalLinkIcon, XIcon,
} from "./icons";
import { setInstancePresentationOpen, setExportModalOpen } from "../store";

interface Props {
  onAddMod: () => void;
  onDeleteSelected: () => void;
  onReorder: (orderedIds: string[]) => void;
}

// ID helpers
const tlId = (item: TopLevelItem) => item.kind === 'row' ? item.row.id : `group:${item.id}`;

// ── Group header ──────────────────────────────────────────────────────────────
function GroupHeader(props: { groupId: string; name: string; blockCount: number; collapsed: boolean; onStartDrag: (e: PointerEvent) => void }) {
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
      <div
        class="cursor-grab touch-none"
        onPointerDown={props.onStartDrag}
        title="Drag to reorder group"
      >
        <FolderOpenIcon class="h-4 w-4 text-primary" />
      </div>
      <Show
        when={editing()}
        fallback={
          <span class="flex-1 cursor-pointer text-sm font-medium text-foreground"
            onClick={() => startGroupRename(props.groupId, props.name)}>
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
export function ModListEditor(props: Props) {
  // draggingId: row ID OR `group:{groupId}`
  const [draggingId,       setDraggingId]       = createSignal<string | null>(null);
  const [hoveredDropId,    setHoveredDropId]    = createSignal<string | null>(null);
  const [dragPointer,      setDragPointer]      = createSignal<{ x: number; y: number } | null>(null);
  // Heights of each top-level item measured at drag start
  const [itemHeights,      setItemHeights]      = createSignal<Map<string, number>>(new Map());

  const activeModList  = () => modListCards().find(m => m.name === selectedModListName());
  const hasContent     = () => modRowsState().length > 0;
  const isDraggingGroup = () => draggingId()?.startsWith('group:') ?? false;

  const draggingRow = () => {
    const id = draggingId();
    if (!id || id.startsWith('group:')) return null;
    return modRowsState().find(r => r.id === id) ?? null;
  };
  const draggingGroupItem = () => {
    const id = draggingId();
    if (!id?.startsWith('group:')) return null;
    const gId = id.slice('group:'.length);
    const found = topLevelItems().find(i => i.kind === 'group' && i.id === gId);
    return found?.kind === 'group' ? found : null;
  };
  const draggingRowIcon = () => {
    const dr = draggingRow();
    return dr?.modrinth_id ? modIcons().get(dr.modrinth_id) : undefined;
  };

  const MOD_ROW_HEIGHT = 50;

  // ── Preview order for top-level items ──────────────────────────────────────
  const previewTLItems = createMemo(() => {
    const items   = topLevelItems();
    const dragging = draggingId();
    const drop    = hoveredDropId();
    if (!dragging || !drop) return items;
    // group-drop = dropping row INTO group, no top-level reorder
    if (drop.startsWith('group-drop:')) return items;

    const draggingItem = items.find(i => tlId(i) === dragging);
    if (!draggingItem) return items;

    const rest = items.filter(i => tlId(i) !== dragging);
    let insertIdx: number;

    if (drop.startsWith('tl-group-after:')) {
      const gId = drop.slice('tl-group-after:'.length);
      const idx = rest.findIndex(i => i.kind === 'group' && i.id === gId);
      insertIdx = idx < 0 ? rest.length : idx + 1;
    } else if (drop.startsWith('tl-group:')) {
      const gId = drop.slice('tl-group:'.length);
      const idx = rest.findIndex(i => i.kind === 'group' && i.id === gId);
      insertIdx = idx < 0 ? rest.length : idx;
    } else if (drop.startsWith('row-after:')) {
      const rid = drop.slice('row-after:'.length);
      const idx = rest.findIndex(i => i.kind === 'row' && i.row.id === rid);
      insertIdx = idx < 0 ? rest.length : idx + 1;
    } else {
      // before a row
      const idx = rest.findIndex(i => i.kind === 'row' && i.row.id === drop);
      insertIdx = idx < 0 ? rest.length : idx;
    }

    const result = [...rest];
    result.splice(insertIdx, 0, draggingItem);
    return result;
  });

  // ── Per-item translateY offsets (height-aware) ─────────────────────────────
  const previewTranslates = createMemo(() => {
    const items   = topLevelItems();
    const preview = previewTLItems();
    const heights = itemHeights();
    if (!draggingId() || !hoveredDropId() || hoveredDropId()?.startsWith('group-drop:')) {
      return new Map<string, number>();
    }

    const FALLBACK = MOD_ROW_HEIGHT;
    let nat = 0; const natPos = new Map<string, number>();
    for (const item of items) {
      const id = tlId(item);
      natPos.set(id, nat);
      nat += heights.get(id) ?? FALLBACK;
    }
    let pre = 0; const prePos = new Map<string, number>();
    for (const item of preview) {
      const id = tlId(item);
      prePos.set(id, pre);
      pre += heights.get(id) ?? FALLBACK;
    }

    const offsets = new Map<string, number>();
    for (const item of items) {
      const id = tlId(item);
      offsets.set(id, (prePos.get(id) ?? 0) - (natPos.get(id) ?? 0));
    }
    return offsets;
  });

  // ── Within-group row preview order ─────────────────────────────────────────
  const previewGroupRowOrder = (groupId: string): string[] => {
    const item = topLevelItems().find(i => i.kind === 'group' && i.id === groupId);
    if (!item || item.kind !== 'group') return [];
    const ids = item.blocks.map(r => r.id);
    const dragging = draggingId();
    const drop     = hoveredDropId();
    if (!dragging || !drop || !ids.includes(dragging)) return ids;

    let insertIdx: number;
    if (drop.startsWith('row-after:')) {
      const ai = ids.indexOf(drop.slice('row-after:'.length));
      if (ai < 0) return ids;
      insertIdx = ai + 1;
    } else if (ids.includes(drop)) {
      insertIdx = ids.indexOf(drop);
    } else {
      return ids;
    }

    const fromIdx = ids.indexOf(dragging);
    const next = [...ids];
    next.splice(fromIdx, 1);
    next.splice(insertIdx > fromIdx ? insertIdx - 1 : insertIdx, 0, dragging);
    return next;
  };

  // ── Helpers ────────────────────────────────────────────────────────────────
  const groupByRowId = () => {
    const map = new Map<string, string>();
    for (const g of aestheticGroups()) for (const id of g.blockIds) map.set(id, g.id);
    return map;
  };

  const removeFromArr = (arr: string[], id: string) => arr.filter(x => x !== id);

  // ── commitDrop ────────────────────────────────────────────────────────────
  const commitDrop = (fromId: string, dropId: string) => {
    if (fromId === dropId) return;
    appendDebugTrace("groups.drag.frontend", { phase: "drop", draggableId: fromId, droppableId: dropId });

    if (fromId.startsWith('group:')) {
      commitGroupMove(fromId.slice('group:'.length), dropId);
      return;
    }
    commitRowMove(fromId, dropId);
  };

  // Move an entire group (all its rows as a block) to a new position
  const commitGroupMove = (gId: string, dropId: string) => {
    const group = aestheticGroups().find(g => g.id === gId);
    if (!group) return;
    const groupRowIds = new Set(group.blockIds);
    const groupRows   = group.blockIds
      .map(id => modRowsState().find(r => r.id === id))
      .filter((r): r is ModRow => Boolean(r));
    const otherRows   = modRowsState().filter(r => !groupRowIds.has(r.id));

    let insertIdx: number;
    if (dropId.startsWith('tl-group:')) {
      const tgId  = dropId.slice('tl-group:'.length);
      const tg    = aestheticGroups().find(g => g.id === tgId);
      const first = tg?.blockIds[0];
      insertIdx   = first ? otherRows.findIndex(r => r.id === first) : otherRows.length;
      if (insertIdx < 0) insertIdx = otherRows.length;
    } else if (dropId.startsWith('tl-group-after:')) {
      const tgId = dropId.slice('tl-group-after:'.length);
      const tg   = aestheticGroups().find(g => g.id === tgId);
      let last   = -1;
      if (tg) for (let i = otherRows.length - 1; i >= 0; i--) {
        if (tg.blockIds.includes(otherRows[i].id)) { last = i; break; }
      }
      insertIdx = last < 0 ? otherRows.length : last + 1;
    } else if (dropId.startsWith('row-after:')) {
      const rid = dropId.slice('row-after:'.length);
      const idx = otherRows.findIndex(r => r.id === rid);
      insertIdx = idx < 0 ? otherRows.length : idx + 1;
    } else {
      // before ungrouped row
      const idx = otherRows.findIndex(r => r.id === dropId);
      insertIdx = idx < 0 ? otherRows.length : idx;
    }

    const newRows = [...otherRows];
    newRows.splice(insertIdx, 0, ...groupRows);
    setModRowsState(newRows);
    props.onReorder(newRows.map(r => r.id));
  };

  // Move a single row
  const commitRowMove = (fromId: string, dropId: string) => {
    const groups     = aestheticGroups().map(g => ({ ...g, blockIds: [...g.blockIds] }));
    const membership = groupByRowId();
    const srcGroupId = membership.get(fromId) ?? null;

    // Remove from source group if needed
    if (srcGroupId) {
      const sg = groups.find(g => g.id === srcGroupId);
      if (sg) sg.blockIds = removeFromArr(sg.blockIds, fromId);
    }

    if (dropId.startsWith('group-drop:')) {
      // Drop into a group (append)
      const tgId = dropId.slice('group-drop:'.length);
      const tg   = groups.find(g => g.id === tgId);
      if (tg && !tg.blockIds.includes(fromId)) tg.blockIds.push(fromId);
      setAestheticGroups(groups.filter(g => g.blockIds.length > 0));

      // Move the row adjacent to the group's rows in modRowsState
      const row = modRowsState().find(r => r.id === fromId)!;
      const others = modRowsState().filter(r => r.id !== fromId);
      let lastIdx = -1;
      if (tg) for (let i = others.length - 1; i >= 0; i--) {
        if (tg.blockIds.includes(others[i].id)) { lastIdx = i; break; }
      }
      const insertIdx = lastIdx < 0 ? others.length : lastIdx + 1;
      const newRows = [...others];
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
      if (dropId.startsWith('row-after:')) {
        targetRowId = dropId.slice('row-after:'.length);
      } else if (!dropId.startsWith('tl-group:') && !dropId.startsWith('tl-group-after:')) {
        targetRowId = dropId;
      }
      if (targetRowId) {
        const tgtGroupId = membership.get(targetRowId);
        if (tgtGroupId) {
          const tg = groups.find(g => g.id === tgtGroupId);
          if (tg && !tg.blockIds.includes(fromId)) {
            const tidx = tg.blockIds.indexOf(targetRowId);
            if (dropId.startsWith('row-after:')) {
              tg.blockIds.splice(tidx < 0 ? tg.blockIds.length : tidx + 1, 0, fromId);
            } else {
              tg.blockIds.splice(tidx < 0 ? 0 : tidx, 0, fromId);
            }
          }
        }
      }
    }

    // For all other drops: compute new row order, then batch-update both signals
    const row    = modRowsState().find(r => r.id === fromId)!;
    const others = modRowsState().filter(r => r.id !== fromId);
    const filteredGroups = groups.filter(g => g.blockIds.length > 0);

    let insertIdx: number;
    if (dropId.startsWith('tl-group:')) {
      const tgId  = dropId.slice('tl-group:'.length);
      const tg    = filteredGroups.find(g => g.id === tgId);
      const first = tg?.blockIds[0];
      insertIdx   = first ? others.findIndex(r => r.id === first) : others.length;
      if (insertIdx < 0) insertIdx = others.length;
    } else if (dropId.startsWith('tl-group-after:')) {
      const tgId = dropId.slice('tl-group-after:'.length);
      const tg   = filteredGroups.find(g => g.id === tgId);
      let last   = -1;
      if (tg) for (let i = others.length - 1; i >= 0; i--) {
        if (tg.blockIds.includes(others[i].id)) { last = i; break; }
      }
      insertIdx = last < 0 ? others.length : last + 1;
    } else if (dropId.startsWith('row-after:')) {
      const rid = dropId.slice('row-after:'.length);
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

  // ── Drag start ────────────────────────────────────────────────────────────
  const startDrag = (id: string, event: PointerEvent | MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();

    // Measure all top-level item heights
    const heights = new Map<string, number>();
    document.querySelectorAll<HTMLElement>('[data-tl-item-id]').forEach(el => {
      heights.set(el.dataset.tlItemId!, el.getBoundingClientRect().height);
    });
    setItemHeights(heights);

    setDraggingId(id);
    setHoveredDropId(null);
    setDragPointer({ x: event.clientX, y: event.clientY });
    document.body.style.userSelect = "none";
  };

  const finishDrag = () => {
    const from = draggingId();
    const drop = hoveredDropId();
    if (from && drop) commitDrop(from, drop);
    setDraggingId(null);
    setHoveredDropId(null);
    setDragPointer(null);
    setItemHeights(new Map());
    document.body.style.userSelect = "";
  };

  onMount(() => {
    const onMove = (event: PointerEvent) => {
      if (!draggingId()) return;
      setDragPointer({ x: event.clientX, y: event.clientY });

      const el = document.elementFromPoint(event.clientX, event.clientY);

      if (isDraggingGroup()) {
        // For group drag: detect at top-level item level
        const tlEl = el?.closest('[data-tl-item-id]') as HTMLElement | null;
        if (tlEl) {
          const itemId = tlEl.dataset.tlItemId!;
          if (itemId === draggingId()) return;
          const rect    = tlEl.getBoundingClientRect();
          const isBottom = event.clientY >= rect.top + rect.height / 2;
          if (itemId.startsWith('group:')) {
            const gId = itemId.slice('group:'.length);
            setHoveredDropId(isBottom ? `tl-group-after:${gId}` : `tl-group:${gId}`);
          } else {
            setHoveredDropId(isBottom ? `row-after:${itemId}` : itemId);
          }
        }
        return;
      }

      // Row drag: check individual rows first (inside or outside groups)
      const rowEl = el?.closest('[data-mod-row-id]') as HTMLElement | null;
      if (rowEl) {
        const hId = rowEl.dataset.modRowId!;
        if (hId === draggingId()) return;
        const rowIdx = rowEl.dataset.modRowIndex;
        let isBottom: boolean;
        if (rowIdx !== undefined) {
          const containerEl = rowEl.closest('[data-mod-row-container]') as HTMLElement | null;
          const containerTop = containerEl ? containerEl.getBoundingClientRect().top : 0;
          const naturalMidY  = containerTop + (parseInt(rowIdx) + 0.5) * MOD_ROW_HEIGHT;
          isBottom = event.clientY >= naturalMidY;
        } else {
          const rect = rowEl.getBoundingClientRect();
          isBottom = event.clientY >= rect.top + rect.height / 2;
        }
        setHoveredDropId(isBottom ? `row-after:${hId}` : hId);
        return;
      }

      // Row drag: check top-level group container (hover over group header/empty area)
      const tlEl = el?.closest('[data-tl-item-id]') as HTMLElement | null;
      if (tlEl) {
        const itemId = tlEl.dataset.tlItemId!;
        if (!itemId.startsWith('group:')) return;
        const gId  = itemId.slice('group:'.length);
        const rect = tlEl.getBoundingClientRect();
        const relY = event.clientY - rect.top;
        // Top 25% → before group, bottom 25% → after group, middle → into group
        if (relY < rect.height * 0.25) {
          setHoveredDropId(`tl-group:${gId}`);
        } else if (relY > rect.height * 0.75) {
          setHoveredDropId(`tl-group-after:${gId}`);
        } else {
          setHoveredDropId(`group-drop:${gId}`);
        }
      }
    };

    const onUp = () => { if (draggingId()) finishDrag(); };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup",   onUp);
    onCleanup(() => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup",   onUp);
      document.body.style.userSelect = "";
    });
  });

  return (
    <div class="flex flex-1 flex-col overflow-hidden">
      <ActionBar onAddMod={props.onAddMod} onDeleteSelected={props.onDeleteSelected} />

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
        {/* Mod list header */}
        <div class="shrink-0 border-b border-border bg-card/50 px-4 py-3">
          <div class="flex items-start justify-between gap-4">
            <div class="min-w-0 flex-1">
              <h2 class="text-lg font-semibold text-foreground">{activeModList()!.name}</h2>
              <p class="text-sm text-muted-foreground">{activeModList()!.description}</p>
              <div class="mt-1.5 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                <span>{modRowsState().length} rule{modRowsState().length !== 1 ? "s" : ""}</span>
                <span>·</span>
                <span>by {activeAccount()?.gamertag ?? "—"}</span>
              </div>
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <button
                onClick={() => setInstancePresentationOpen(true)}
                class="flex h-8 items-center gap-1.5 rounded-md px-2.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                title="Edit notes and icon for this mod list"
              >
                <PencilIcon class="h-3.5 w-3.5" />
                Notes
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
        <div class="flex-1 p-4" style={{ overflow: draggingId() ? "visible" : "auto" }}>
          <Show when={hasContent()} fallback={<EmptyState onAddMod={props.onAddMod} />}>
            <>
              {/* Drag ghost */}
              <Show when={draggingId() && dragPointer()}>
                <div
                  class="pointer-events-none fixed z-50 w-80 rounded-md border border-primary/40 bg-card shadow-2xl ring-1 ring-primary/20"
                  style={{ left: `${dragPointer()!.x + 12}px`, top: `${dragPointer()!.y - 24}px`, opacity: "0.95" }}
                >
                  <Show when={isDraggingGroup() && draggingGroupItem()} fallback={
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
                  }>
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
              <div class="space-y-0.5">
                <For each={topLevelItems()}>
                  {(item, index) => {
                    const id        = tlId(item);
                    const isDragging = () => draggingId() === id;
                    const offset     = () => isDragging() ? 0 : (previewTranslates().get(id) ?? 0);

                    if (item.kind === 'row') {
                      return (
                        <div
                          data-tl-item-id={id}
                          data-tl-item-index={String(index())}
                          style={{
                            transform:  draggingId() ? `translateY(${offset()}px)` : "none",
                            transition: draggingId() ? "transform 150ms ease" : "none",
                            position: "relative",
                            "z-index": isDragging() ? "0" : "1",
                          }}
                          class={isDragging() ? "opacity-0" : ""}
                        >
                          <div
                            data-mod-row-id={item.row.id}
                            onPointerEnter={() => {
                              if (draggingId() && !isDraggingGroup() && draggingId() !== item.row.id)
                                setHoveredDropId(item.row.id);
                            }}
                            class={hoveredDropId() === item.row.id ? "rounded-md ring-1 ring-primary/40" : ""}
                          >
                            <ModRuleItem row={item.row} onStartDrag={(rowId, e) => startDrag(rowId, e)} />
                          </div>
                        </div>
                      );
                    }

                    // Group item
                    const group = item;
                    const isGroupDropTarget = () => hoveredDropId() === `group-drop:${group.id}`;
                    const isBeforeTarget    = () => hoveredDropId() === `tl-group:${group.id}`;
                    const isAfterTarget     = () => hoveredDropId() === `tl-group-after:${group.id}`;

                    return (
                      <div
                        data-tl-item-id={id}
                        data-tl-item-index={String(index())}
                        style={{
                          transform:  draggingId() ? `translateY(${offset()}px)` : "none",
                          transition: draggingId() ? "transform 150ms ease" : "none",
                          position: "relative",
                          "z-index": isDragging() ? "0" : "1",
                        }}
                        class={`mb-2 ${isDragging() ? "opacity-0" : ""}`}
                      >
                        {/* Before-group drop indicator */}
                        <Show when={isBeforeTarget()}>
                          <div class="mb-1 h-0.5 rounded bg-primary" />
                        </Show>

                        <div
                          data-tl-item-id={id}
                          class={`rounded-xl border p-2 shadow-sm transition-colors ${
                            isGroupDropTarget()
                              ? "border-primary/40 bg-primary/5"
                              : "border-border/70 bg-muted/10"
                          }`}
                        >
                          {/* Header row */}
                          <div class="flex items-center gap-1 px-1 py-1">
                            <GroupHeader
                              groupId={group.id}
                              name={group.name}
                              blockCount={group.blocks.length}
                              collapsed={group.collapsed}
                              onStartDrag={(e) => startDrag(`group:${group.id}`, e)}
                            />
                          </div>

                          {/* Rows inside group */}
                          <Show when={!group.collapsed}>
                            <div class="mt-1 space-y-1" data-mod-row-container>
                              <For each={group.blocks}>
                                {(row, rowIndex) => {
                                  const previewIdx = () => {
                                    if (!draggingId()) return rowIndex();
                                    const order = previewGroupRowOrder(group.id);
                                    const i = order.indexOf(row.id);
                                    return i >= 0 ? i : rowIndex();
                                  };
                                  const isDraggingRow = () => draggingId() === row.id;
                                  const rowOffset     = () => isDraggingRow() ? 0 : (previewIdx() - rowIndex()) * MOD_ROW_HEIGHT;
                                  const isTarget      = () => !isDraggingRow() && (hoveredDropId() === row.id || hoveredDropId() === `row-after:${row.id}`);

                                  return (
                                    <div
                                      data-mod-row-id={row.id}
                                      data-mod-row-index={String(rowIndex())}
                                      style={{
                                        transform:  draggingId() ? `translateY(${rowOffset()}px)` : "none",
                                        transition: draggingId() ? "transform 150ms ease" : "none",
                                        position: "relative",
                                        "z-index": isDraggingRow() ? "0" : "1",
                                      }}
                                      class={isDraggingRow() ? "opacity-0" : isTarget() ? "rounded-md ring-1 ring-primary/40" : ""}
                                    >
                                      <ModRuleItem row={row} onStartDrag={(rowId, e) => startDrag(rowId, e)} />
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

                        {/* After-group drop indicator */}
                        <Show when={isAfterTarget()}>
                          <div class="mt-1 h-0.5 rounded bg-primary" />
                        </Show>
                      </div>
                    );
                  }}
                </For>
              </div>

              {/* No search results */}
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
