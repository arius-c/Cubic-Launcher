/**
 * AltSection — renders and manages drag-and-drop for a rule's alternatives.
 *
 * Uses the unified useDragEngine. Single engine instance replaces both
 * the alt-row and alt-group drag signal sets.
 * Drop-target detection is fully position-based (no elementFromPoint).
 */
import { For, Show, batch, createMemo } from "solid-js";
import type { ModRow, AestheticGroup } from "../lib/types";
import {
  aestheticGroups, setAestheticGroups, modRowsState, setModRowsState,
  functionalGroups, sortOrder, tagFilter, modIcons,
  toggleGroupCollapsed, removeAestheticGroup,
  editingGroupId, groupNameDraft, setGroupNameDraft, startGroupRename, commitGroupRename,
} from "../store";
import { ChevronRightIcon, ChevronDownIcon, FolderOpenIcon, PackageIcon, XIcon } from "./icons";
import {
  computePreviewTranslates,
  findRowByIdDeep,
  updateAltsDeep,
} from "../lib/dragUtils";
import { ModRuleItem } from "./ModRuleItem";
import { useDragEngine, type DragItem } from "../lib/dragEngine";

// ── Types ─────────────────────────────────────────────────────────────────────

type AltTLItem =
  | { kind: "alt-group"; id: string; name: string; collapsed: boolean; blocks: ModRow[]; blockIds: string[] }
  | { kind: "alt-row"; row: ModRow };

const altTlId = (item: AltTLItem): string =>
  item.kind === "alt-row" ? item.row.id : `alt-group:${item.id}`;

// ── Helpers ───────────────────────────────────────────────────────────────────

const removeFrom = (ids: string[], id: string) => ids.filter(i => i !== id);
const insertBefore = (ids: string[], id: string, beforeId: string) => {
  const next = removeFrom(ids, id);
  const idx = next.findIndex(i => i === beforeId);
  if (idx < 0) return [...next, id];
  next.splice(idx, 0, id);
  return next;
};

// ── Component ─────────────────────────────────────────────────────────────────

interface AltSectionProps {
  parentRow: ModRow;
  depth: number;
  onReorderAlts?: (parentId: string, orderedIds: string[]) => void;
}

export function AltSection(props: AltSectionProps) {
  let containerRef: HTMLDivElement | undefined;

  // ── Derived state ──────────────────────────────────────────────────────────

  const altMatchesFilter = (alt: ModRow): boolean => {
    const tf = tagFilter();
    if (tf.size === 0) return true;
    const fg = functionalGroups();
    const check = (row: ModRow): boolean =>
      fg.some(g => tf.has(g.id) && g.modIds.includes(row.id)) ||
      (row.alternatives ?? []).some(check);
    return check(alt);
  };

  const scopedAltGroups = () => {
    const so = sortOrder();
    return aestheticGroups()
      .filter(g => g.scopeRowId === props.parentRow.id)
      .map(g => {
        let blocks = g.blockIds
          .map(id => (props.parentRow.alternatives ?? []).find(a => a.id === id))
          .filter((a): a is ModRow => Boolean(a))
          .filter(altMatchesFilter);
        if (so === "name-az") blocks = [...blocks].sort((a, b) => a.name.localeCompare(b.name));
        if (so === "name-za") blocks = [...blocks].sort((a, b) => b.name.localeCompare(a.name));
        return { ...g, blocks };
      })
      .filter(g => g.blocks.length > 0);
  };

  const altGroupMembership = () => {
    const map = new Map<string, string>();
    for (const g of aestheticGroups().filter(g => g.scopeRowId === props.parentRow.id)) {
      for (const id of g.blockIds) map.set(id, g.id);
    }
    return map;
  };

  const altTopLevelItems = (): AltTLItem[] => {
    const membership = altGroupMembership();
    const groups = scopedAltGroups();
    const groupMap = new Map(groups.map(g => [g.id, g]));
    const seenGroups = new Set<string>();
    const items: AltTLItem[] = [];

    for (const alt of (props.parentRow.alternatives ?? [])) {
      const gId = membership.get(alt.id);
      if (gId) {
        if (!seenGroups.has(gId)) {
          seenGroups.add(gId);
          const group = groupMap.get(gId);
          if (group) {
            items.push({
              kind: "alt-group",
              id: group.id,
              name: group.name,
              collapsed: group.collapsed,
              blocks: group.blocks,
              blockIds: group.blockIds,
            });
          }
        }
      } else if (altMatchesFilter(alt)) {
        items.push({ kind: "alt-row", row: alt });
      }
    }
    return items;
  };

  // ── Unified drag engine ──────────────────────────────────────────────────
  const engine = useDragEngine({
    containerRef: () => containerRef,
    getItems: () => altTopLevelItems().map((item): DragItem =>
      item.kind === "alt-row"
        ? { kind: "row", id: item.row.id }
        : { kind: "group", id: item.id }
    ),
    onCommit: (fromId, dropId, fromKind) => {
      if (fromId === dropId) return;
      if (fromKind === "group") {
        commitAltGroupDrop(fromId, dropId);
      } else {
        commitAltDrop(fromId, dropId);
      }
    },
  });

  const isGroupDrag = () => engine.draggingKind() === "group";
  const draggingAlt = () => (props.parentRow.alternatives ?? []).find(a => a.id === engine.draggingId()) ?? null;
  const draggingAltGroupData = () => scopedAltGroups().find(g => g.id === engine.draggingId()) ?? null;

  // ── Enhanced drop target detection ──────────────────────────────────────
  const detectAltDropTarget = (cursorY: number): string | null => {
    const cr = engine.cachedContainerRect();
    if (!cr) return null;

    const items      = altTopLevelItems();
    const heights    = engine.cachedHeights();
    const isGDrag    = isGroupDrag();
    const draggingItemId = engine.draggingId()!;
    const membership = altGroupMembership();
    const initMidYs  = engine.cachedMidYs();

    let y = cr.top;

    for (const item of items) {
      const id = altTlId(item);
      const h  = heights.get(id) ?? 40;

      // Skip the item being dragged
      const dragTlId = isGDrag ? `alt-group:${draggingItemId}` : draggingItemId;
      if (id === dragTlId) { y += h; continue; }

      if (cursorY < y + h) {
        if (item.kind === "alt-group") {
          const gId  = item.id;
          const relY = cursorY - y;

          if (isGDrag) {
            return cursorY >= y + h / 2 ? `alt-group-after:${gId}` : `alt-group:${gId}`;
          }

          const draggedIsGroupMember = membership.get(draggingItemId) === gId;

          if (!draggedIsGroupMember && relY < h * 0.2) return `alt-tl-group:${gId}`;
          if (!draggedIsGroupMember && relY > h * 0.8) return `alt-tl-group-after:${gId}`;

          const candidates = item.blocks.filter(b => b.id !== draggingItemId);
          if (candidates.length === 0) return `alt-group-drop:${gId}`;

          let nearest = candidates[0];
          let nearestMid = initMidYs.get(nearest.id) ?? (y + h / 2);
          for (const c of candidates.slice(1)) {
            const mid = initMidYs.get(c.id) ?? nearestMid;
            if (Math.abs(cursorY - mid) < Math.abs(cursorY - nearestMid)) {
              nearest = c;
              nearestMid = mid;
            }
          }
          return cursorY >= nearestMid ? `alt-after:${nearest.id}` : nearest.id;
        }

        if (item.kind === "alt-row") {
          const altId = item.row.id;
          if (isGDrag) {
            return cursorY >= y + h / 2 ? `alt-row-after:${altId}` : `alt-row:${altId}`;
          }
          const midY = initMidYs.get(altId) ?? (y + h / 2);
          return cursorY >= midY ? `alt-after:${altId}` : altId;
        }
      }

      y += h;
    }
    return null;
  };

  // Wrap startDrag to install custom detection
  const handleAltDragStart = (id: string, kind: "row" | "group", event: PointerEvent | MouseEvent) => {
    engine.startDrag(id, kind, event);

    const onMoveOverride = (ev: PointerEvent) => {
      if (!engine.draggingId()) return;
      const target = detectAltDropTarget(ev.clientY);
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

  // ── Preview orders (all createMemo) ─────────────────────────────────────

  const previewAltTLItems = createMemo((): AltTLItem[] => {
    const items = altTopLevelItems();
    const dragging = engine.draggingId();
    const drop     = engine.hoveredDropId();
    if (!dragging || !drop) return items;

    if (isGroupDrag()) {
      const draggingItem = items.find(i => i.kind === "alt-group" && i.id === dragging);
      if (!draggingItem) return items;

      const rest = items.filter(i => !(i.kind === "alt-group" && i.id === dragging));
      let insertIdx: number;

      if (drop.startsWith("alt-group-after:")) {
        const gId = drop.slice("alt-group-after:".length);
        const idx = rest.findIndex(i => i.kind === "alt-group" && i.id === gId);
        insertIdx = idx < 0 ? rest.length : idx + 1;
      } else if (drop.startsWith("alt-group:")) {
        const gId = drop.slice("alt-group:".length);
        const idx = rest.findIndex(i => i.kind === "alt-group" && i.id === gId);
        insertIdx = idx < 0 ? rest.length : idx;
      } else if (drop.startsWith("alt-row-after:")) {
        const rowId = drop.slice("alt-row-after:".length);
        const idx = rest.findIndex(i => i.kind === "alt-row" && i.row.id === rowId);
        insertIdx = idx < 0 ? rest.length : idx + 1;
      } else if (drop.startsWith("alt-row:")) {
        const rowId = drop.slice("alt-row:".length);
        const idx = rest.findIndex(i => i.kind === "alt-row" && i.row.id === rowId);
        insertIdx = idx < 0 ? rest.length : idx;
      } else {
        return items;
      }

      const result = [...rest];
      result.splice(insertIdx, 0, draggingItem);
      return result;
    }

    // Alt row drag
    if (drop.startsWith("alt-group-drop:")) return items;

    const draggingIdx = items.findIndex(i => i.kind === "alt-row" && i.row.id === dragging);
    if (draggingIdx < 0) return items;

    const draggingItem = items[draggingIdx];
    const rest = items.filter((_, idx) => idx !== draggingIdx);
    let insertIdx: number;

    if (drop.startsWith("alt-tl-group:")) {
      const gId = drop.slice("alt-tl-group:".length);
      const idx = rest.findIndex(i => i.kind === "alt-group" && i.id === gId);
      insertIdx = idx < 0 ? rest.length : idx;
    } else if (drop.startsWith("alt-tl-group-after:")) {
      const gId = drop.slice("alt-tl-group-after:".length);
      const idx = rest.findIndex(i => i.kind === "alt-group" && i.id === gId);
      insertIdx = idx < 0 ? rest.length : idx + 1;
    } else if (drop.startsWith("alt-after:")) {
      const afterId = drop.slice("alt-after:".length);
      const idx = rest.findIndex(i => i.kind === "alt-row" && i.row.id === afterId);
      if (idx < 0) return items;
      insertIdx = idx + 1;
    } else {
      const idx = rest.findIndex(i => i.kind === "alt-row" && i.row.id === drop);
      if (idx < 0) return items;
      insertIdx = idx;
    }

    const result = [...rest];
    result.splice(insertIdx, 0, draggingItem);
    return result;
  });

  const previewAltTLTranslates = createMemo((): Map<string, number> => {
    if (!engine.anyDragging()) return new Map();
    if (engine.hoveredDropId()?.startsWith("alt-group-drop:")) return new Map();
    if (!engine.draggingId() || !engine.hoveredDropId()) return new Map();
    const items   = altTopLevelItems();
    const preview = previewAltTLItems();
    return computePreviewTranslates(
      items.map(altTlId),
      preview.map(altTlId),
      engine.cachedHeights(),
      0
    );
  });

  const previewGroupedTranslates = createMemo((): Map<string, number> => {
    const map = new Map<string, number>();
    const dragging = engine.draggingId();
    const drop     = engine.hoveredDropId();
    if (!dragging || !drop || isGroupDrag()) return map;

    for (const group of scopedAltGroups()) {
      const ids = group.blocks.map(a => a.id);
      if (!ids.includes(dragging)) continue;

      let insertIdx: number;
      if (drop === `alt-group-drop:${group.id}`) {
        insertIdx = ids.length;
      } else if (drop.startsWith("alt-after:")) {
        const afterId = drop.slice("alt-after:".length);
        const ai = ids.indexOf(afterId);
        if (ai < 0) continue;
        insertIdx = ai + 1;
      } else if (ids.includes(drop)) {
        insertIdx = ids.indexOf(drop);
      } else {
        continue;
      }

      const fromIdx = ids.indexOf(dragging);
      const next = [...ids];
      next.splice(fromIdx, 1);
      next.splice(insertIdx > fromIdx ? insertIdx - 1 : insertIdx, 0, dragging);

      const translates = computePreviewTranslates(ids, next, engine.cachedHeights(), 0);
      for (const [id, offset] of translates) {
        map.set(id, offset);
      }
    }
    return map;
  });

  // ── Commit: individual alt drop ────────────────────────────────────────────

  const commitAltDrop = (fromId: string, dropId: string) => {
    const parentId   = props.parentRow.id;
    const rawScoped  = aestheticGroups().filter(g => g.scopeRowId === parentId);
    const membership = altGroupMembership();
    const groups     = rawScoped.map(g => ({ ...g, blockIds: [...g.blockIds] }));
    let ungroupedIds = (props.parentRow.alternatives ?? [])
      .map(a => a.id)
      .filter(id => !membership.has(id));

    // Remove from source
    const srcGroupId = membership.get(fromId) ?? null;
    if (srcGroupId) {
      const src = groups.find(g => g.id === srcGroupId);
      if (src) src.blockIds = removeFrom(src.blockIds, fromId);
    } else {
      ungroupedIds = removeFrom(ungroupedIds, fromId);
    }

    // Insert at target
    let useTLOrdering = false;
    let tlInsertTarget: { kind: "before-group"; gId: string } | { kind: "after-group"; gId: string } | null = null;

    if (dropId.startsWith("alt-group-drop:")) {
      const tgtId = dropId.replace("alt-group-drop:", "");
      const tgt = groups.find(g => g.id === tgtId);
      if (tgt) tgt.blockIds = [...removeFrom(tgt.blockIds, fromId), fromId];
    } else if (dropId.startsWith("alt-tl-group:")) {
      const tgId = dropId.slice("alt-tl-group:".length);
      ungroupedIds = [...ungroupedIds, fromId];
      useTLOrdering = true;
      tlInsertTarget = { kind: "before-group", gId: tgId };
    } else if (dropId.startsWith("alt-tl-group-after:")) {
      const tgId = dropId.slice("alt-tl-group-after:".length);
      ungroupedIds = [...ungroupedIds, fromId];
      useTLOrdering = true;
      tlInsertTarget = { kind: "after-group", gId: tgId };
    } else if (dropId === "alt-ungrouped-drop") {
      ungroupedIds = [...ungroupedIds, fromId];
    } else if (dropId.startsWith("alt-after:")) {
      const afterId      = dropId.replace("alt-after:", "");
      const afterGroupId = membership.get(afterId) ?? null;
      if (afterGroupId) {
        const tgt = groups.find(g => g.id === afterGroupId);
        if (tgt) {
          const ai = tgt.blockIds.indexOf(afterId);
          if (ai < 0 || ai === tgt.blockIds.length - 1) {
            tgt.blockIds = [...removeFrom(tgt.blockIds, fromId), fromId];
          } else {
            tgt.blockIds = insertBefore(tgt.blockIds, fromId, tgt.blockIds[ai + 1]);
          }
        }
      } else {
        const ai = ungroupedIds.indexOf(afterId);
        if (ai < 0 || ai === ungroupedIds.length - 1) {
          ungroupedIds = [...ungroupedIds, fromId];
        } else {
          ungroupedIds = insertBefore(ungroupedIds, fromId, ungroupedIds[ai + 1]);
        }
      }
    } else {
      const tgtGroupId = membership.get(dropId) ?? null;
      if (tgtGroupId) {
        const tgt = groups.find(g => g.id === tgtGroupId);
        if (tgt) tgt.blockIds = insertBefore(tgt.blockIds, fromId, dropId);
      } else {
        ungroupedIds = insertBefore(ungroupedIds, fromId, dropId);
      }
    }

    // Build interleaved ordering
    const interleaved: string[] = [];
    const seenGroups = new Set<string>();
    const ungroupedSet = new Set(ungroupedIds);
    let ungroupedPos = 0;
    for (const alt of (props.parentRow.alternatives ?? [])) {
      if (alt.id === fromId && !membership.has(fromId) && !ungroupedSet.has(fromId)) continue;
      const gId = membership.get(alt.id);
      if (gId) {
        if (!seenGroups.has(gId)) {
          seenGroups.add(gId);
          const g = groups.find(gg => gg.id === gId);
          if (g) interleaved.push(...g.blockIds);
        }
      } else if (ungroupedSet.has(alt.id)) {
        interleaved.push(ungroupedIds[ungroupedPos++]);
      }
    }
    for (const g of groups) {
      if (!seenGroups.has(g.id) && g.blockIds.length > 0) interleaved.push(...g.blockIds);
    }
    for (const id of ungroupedIds) {
      if (!interleaved.includes(id)) interleaved.push(id);
    }

    let newOrderedIds: string[];
    if (useTLOrdering && tlInsertTarget) {
      const withoutDragged = interleaved.filter(id => id !== fromId);
      let insertIdx: number;
      const tg = groups.find(g => g.id === tlInsertTarget!.gId);
      if (tlInsertTarget.kind === "before-group") {
        const firstMember = tg?.blockIds[0];
        insertIdx = firstMember ? withoutDragged.indexOf(firstMember) : withoutDragged.length;
        if (insertIdx < 0) insertIdx = withoutDragged.length;
      } else {
        const lastMember = tg?.blockIds[tg.blockIds.length - 1];
        insertIdx = lastMember ? withoutDragged.indexOf(lastMember) + 1 : withoutDragged.length;
        if (insertIdx < 0) insertIdx = withoutDragged.length;
      }
      withoutDragged.splice(insertIdx, 0, fromId);
      newOrderedIds = withoutDragged;
    } else {
      newOrderedIds = interleaved;
    }

    const curRows   = modRowsState();
    const parentRow = findRowByIdDeep(curRows, parentId);
    const altMap    = new Map((parentRow?.alternatives ?? []).map(a => [a.id, a]));
    const newAlts   = newOrderedIds.map(id => altMap.get(id)).filter((a): a is ModRow => Boolean(a));
    const newRows   = updateAltsDeep(curRows, parentId, newAlts);

    const filteredGroups: AestheticGroup[] = groups
      .filter(g => g.blockIds.length > 0)
      .map(g => ({ id: g.id, name: g.name, collapsed: g.collapsed, blockIds: g.blockIds, scopeRowId: g.scopeRowId }));
    const newAestheticGroups = [
      ...aestheticGroups().filter(g => g.scopeRowId !== parentId),
      ...filteredGroups,
    ];

    batch(() => {
      setAestheticGroups(newAestheticGroups);
      setModRowsState(newRows);
    });
    props.onReorderAlts?.(parentId, newOrderedIds);
  };

  // ── Commit: alt group drop ─────────────────────────────────────────────────

  const commitAltGroupDrop = (fromGroupId: string, dropId: string) => {
    const parentId = props.parentRow.id;
    const scoped   = aestheticGroups().filter(g => g.scopeRowId === parentId);
    const others   = aestheticGroups().filter(g => g.scopeRowId !== parentId);
    const dragging = scoped.find(g => g.id === fromGroupId);
    if (!dragging) return;

    const rest = scoped.filter(g => g.id !== fromGroupId);
    let insertIdx: number;

    if (dropId.startsWith("alt-group-after:")) {
      const tgId = dropId.slice("alt-group-after:".length);
      const idx  = rest.findIndex(g => g.id === tgId);
      insertIdx  = idx < 0 ? rest.length : idx + 1;
    } else if (dropId.startsWith("alt-group:")) {
      const tgId = dropId.slice("alt-group:".length);
      const idx  = rest.findIndex(g => g.id === tgId);
      insertIdx  = idx < 0 ? rest.length : idx;
    } else {
      insertIdx = rest.length;
    }

    const reordered = [...rest];
    reordered.splice(insertIdx, 0, dragging);

    const groupMemberIds = new Set(dragging.blockIds);
    const curRows  = modRowsState();
    const parentRow = findRowByIdDeep(curRows, parentId);
    if (!parentRow) return;

    const curAlts     = parentRow.alternatives ?? [];
    const groupMembers = dragging.blockIds
      .map(id => curAlts.find(a => a.id === id))
      .filter((a): a is ModRow => Boolean(a));
    const otherAlts   = curAlts.filter(a => !groupMemberIds.has(a.id));

    let altInsertIdx: number;
    if (dropId.startsWith("alt-group-after:")) {
      const tgId = dropId.slice("alt-group-after:".length);
      const tg   = scoped.find(g => g.id === tgId);
      let last = -1;
      if (tg) for (let i = otherAlts.length - 1; i >= 0; i--) {
        if (tg.blockIds.includes(otherAlts[i].id)) { last = i; break; }
      }
      altInsertIdx = last < 0 ? otherAlts.length : last + 1;
    } else if (dropId.startsWith("alt-group:")) {
      const tgId  = dropId.slice("alt-group:".length);
      const tg    = scoped.find(g => g.id === tgId);
      const first = tg?.blockIds[0];
      altInsertIdx = first ? otherAlts.findIndex(a => a.id === first) : otherAlts.length;
      if (altInsertIdx < 0) altInsertIdx = otherAlts.length;
    } else if (dropId.startsWith("alt-row-after:")) {
      const rowId = dropId.slice("alt-row-after:".length);
      const idx   = otherAlts.findIndex(a => a.id === rowId);
      altInsertIdx = idx < 0 ? otherAlts.length : idx + 1;
    } else if (dropId.startsWith("alt-row:")) {
      const rowId = dropId.slice("alt-row:".length);
      const idx   = otherAlts.findIndex(a => a.id === rowId);
      altInsertIdx = idx < 0 ? otherAlts.length : idx;
    } else {
      altInsertIdx = otherAlts.length;
    }

    const newAlts = [...otherAlts];
    newAlts.splice(altInsertIdx, 0, ...groupMembers);
    const newRows = updateAltsDeep(curRows, parentId, newAlts);

    batch(() => {
      setAestheticGroups([...others, ...reordered]);
      setModRowsState(newRows);
    });
    props.onReorderAlts?.(parentId, newAlts.map(a => a.id));
  };

  // ── onReorderAlts for nested ModRuleItems inside this alt section ──────────
  const handleNestedReorderAlts = (parentId: string, orderedIds: string[]) => {
    setModRowsState(cur => {
      const parentRow = findRowByIdDeep(cur, parentId);
      if (!parentRow) return cur;
      const altMap = new Map((parentRow.alternatives ?? []).map(x => [x.id, x]));
      const newAlts = orderedIds.map(xid => altMap.get(xid)).filter((x): x is ModRow => Boolean(x));
      return updateAltsDeep(cur, parentId, newAlts);
    });
    props.onReorderAlts?.(parentId, orderedIds);
  };

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div class="mt-0.5 pb-1" ref={containerRef}>
      {/* Alt group drag ghost */}
      <Show when={isGroupDrag() && engine.dragPointer()}>
        <div
          class="pointer-events-none fixed z-[100] w-60 rounded-md border border-primary/40 bg-card shadow-2xl ring-1 ring-primary/20"
          style={{ left: `${engine.dragPointer()!.x + 12}px`, top: `${engine.dragPointer()!.y - 16}px` }}
        >
          <div class="flex items-center gap-2 px-3 py-2">
            <FolderOpenIcon class="h-4 w-4 shrink-0 text-primary" />
            <span class="truncate text-sm font-medium text-foreground">{draggingAltGroupData()?.name}</span>
            <span class="ml-1 shrink-0 text-xs text-muted-foreground">{draggingAltGroupData()?.blocks.length} alts</span>
          </div>
        </div>
      </Show>

      {/* Alt drag ghost */}
      <Show when={!isGroupDrag() && engine.draggingId() && engine.dragPointer()}>
        <div
          class="pointer-events-none fixed z-50 w-80 rounded-md border border-primary/40 bg-card shadow-2xl ring-1 ring-primary/20"
          style={{ left: `${engine.dragPointer()!.x + 12}px`, top: `${engine.dragPointer()!.y - 24}px`, opacity: "0.95" }}
        >
          <div class="flex items-center gap-3 px-2 py-2">
            <div class="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted">
              <Show
                when={draggingAlt()?.modrinth_id && modIcons().get(draggingAlt()!.modrinth_id!)}
                fallback={<PackageIcon class="h-4 w-4 text-muted-foreground" />}
              >
                <img src={modIcons().get(draggingAlt()!.modrinth_id!)!} alt={draggingAlt()?.name} class="h-8 w-8 object-cover" />
              </Show>
            </div>
            <span class="truncate font-medium text-foreground">{draggingAlt()?.name}</span>
          </div>
        </div>
      </Show>

      {/* Alt TL items */}
      <For each={altTopLevelItems()}>
        {(item, tlIndex) => {
          const id = altTlId(item);
          const isDraggingThis = () => {
            const dId = engine.draggingId();
            if (!dId) return false;
            return isGroupDrag()
              ? (item.kind === "alt-group" && item.id === dId)
              : (item.kind === "alt-row" && item.row.id === dId);
          };
          const tlOffset = () => isDraggingThis() ? 0 : (previewAltTLTranslates().get(id) ?? 0);
          const isLastTLItem = () => tlIndex() === altTopLevelItems().length - 1;

          if (item.kind === "alt-row") {
            const alt = item.row;
            const isTarget = () =>
              !isDraggingThis() && (
                engine.hoveredDropId() === alt.id ||
                engine.hoveredDropId() === `alt-after:${alt.id}`
              );
            return (
              <div
                data-draggable-id={id}
                data-draggable-mid-id={alt.id}
                style={{
                  transform:  engine.anyDragging() ? `translateY(${tlOffset()}px)` : "none",
                  transition: engine.anyDragging() ? "transform 150ms ease" : "none",
                  position:   "relative",
                  "z-index":  isDraggingThis() ? "0" : "1",
                }}
                class={isDraggingThis() ? "opacity-0 pointer-events-none" : isTarget() ? "rounded-md ring-1 ring-primary/40" : ""}
              >
                <ModRuleItem
                  row={alt}
                  depth={props.depth + 1}
                  isLast={isLastTLItem()}
                  onStartDrag={(altId, e) => handleAltDragStart(altId, "row", e)}
                  onReorderAlts={handleNestedReorderAlts}
                />
              </div>
            );
          }

          // Alt group
          const group = item;
          const isGroupDropTarget = () => !isGroupDrag() && engine.hoveredDropId() === `alt-group-drop:${group.id}`;
          const isBeforeTarget    = () =>
            engine.hoveredDropId() === `alt-group:${group.id}` ||
            engine.hoveredDropId() === `alt-tl-group:${group.id}`;
          const isAfterTarget     = () =>
            engine.hoveredDropId() === `alt-group-after:${group.id}` ||
            engine.hoveredDropId() === `alt-tl-group-after:${group.id}`;

          return (
            <div
              data-draggable-id={id}
              class={`relative ml-6 pl-4 ${isLastTLItem() ? "" : "mb-3"} ${isDraggingThis() ? "opacity-0 pointer-events-none" : ""}`}
              style={{
                transform:  engine.anyDragging() && !isDraggingThis() ? `translateY(${tlOffset()}px)` : "none",
                transition: engine.anyDragging() ? "transform 150ms ease" : "none",
                position:   "relative",
                "z-index":  isDraggingThis() ? "0" : "1",
              }}
            >
              {/* Connector column */}
              <div class="pointer-events-none absolute inset-y-0 left-0 w-4">
                <Show
                  when={!isLastTLItem()}
                  fallback={<div class="absolute left-2 top-0 h-1/2 w-px bg-border/35" />}
                >
                  <div class="absolute -bottom-3 left-2 top-0 w-px bg-border/35" />
                </Show>
                <div class="absolute left-2 top-1/2 h-px w-2 bg-border/35" />
              </div>

              {/* Before-group drop indicator */}
              <Show when={isBeforeTarget()}>
                <div class="mb-1 h-0.5 rounded bg-primary" />
              </Show>

              {/* Group box */}
              <div class={`rounded-xl border bg-muted/10 p-2 shadow-sm transition-colors ${
                isGroupDropTarget() ? "border-primary/40 bg-primary/5" : "border-border/70"
              }`}>
                {/* Group header */}
                <div class="mb-2 flex items-center gap-2 px-1">
                  <button
                    onClick={() => toggleGroupCollapsed(group.id)}
                    class="flex items-center text-xs font-medium text-muted-foreground transition-colors hover:text-foreground"
                    title={group.collapsed ? "Expand" : "Collapse"}
                  >
                    <Show when={group.collapsed} fallback={<ChevronDownIcon class="h-3.5 w-3.5" />}>
                      <ChevronRightIcon class="h-3.5 w-3.5" />
                    </Show>
                  </button>
                  <div
                    class="cursor-grab touch-none"
                    onPointerDown={(e) => handleAltDragStart(group.id, "group", e)}
                    title="Drag to reorder group"
                  >
                    <FolderOpenIcon class="h-3.5 w-3.5 text-primary" />
                  </div>
                  <Show
                    when={editingGroupId() === group.id}
                    fallback={
                      <span
                        class="flex-1 cursor-pointer text-xs font-medium uppercase tracking-wider text-muted-foreground"
                        onClick={() => startGroupRename(group.id, group.name)}
                      >
                        {group.name}
                      </span>
                    }
                  >
                    <input
                      type="text"
                      value={groupNameDraft()}
                      onInput={e => setGroupNameDraft(e.currentTarget.value)}
                      onBlur={() => commitGroupRename(group.id)}
                      onKeyDown={e => {
                        if (e.key === "Enter" || e.key === "Escape") commitGroupRename(group.id);
                      }}
                      class="flex-1 rounded bg-transparent text-xs font-medium text-muted-foreground outline-none"
                      autofocus
                    />
                  </Show>
                  <span class="text-[10px] text-muted-foreground">{group.blocks.length} alts</span>
                  <button
                    onClick={() => removeAestheticGroup(group.id)}
                    class="flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                    title="Remove group"
                  >
                    <XIcon class="h-3 w-3" />
                  </button>
                </div>

                {/* Group members */}
                <Show when={!group.collapsed}>
                  <div class="space-y-1">
                    <For each={group.blocks}>
                      {(alt) => {
                        const isDraggingAlt = () => engine.draggingId() === alt.id && !isGroupDrag();
                        const isTarget      = () =>
                          !isDraggingAlt() && (
                            engine.hoveredDropId() === alt.id ||
                            engine.hoveredDropId() === `alt-after:${alt.id}`
                          );
                        const offset = () => isDraggingAlt() ? 0 : (previewGroupedTranslates().get(alt.id) ?? 0);
                        return (
                          <div
                            data-draggable-id={alt.id}
                            data-draggable-mid-id={alt.id}
                            style={{
                              transform:  engine.anyDragging() ? `translateY(${offset()}px)` : "none",
                              transition: engine.anyDragging() ? "transform 150ms ease" : "none",
                              position:   "relative",
                              "z-index":  isDraggingAlt() ? "0" : "1",
                            }}
                            class={isDraggingAlt() ? "opacity-0 pointer-events-none" : isTarget() ? "rounded-md ring-1 ring-primary/40" : ""}
                          >
                            <ModRuleItem
                              row={alt}
                              depth={0}
                              onStartDrag={(altId, e) => handleAltDragStart(altId, "row", e)}
                              onReorderAlts={handleNestedReorderAlts}
                            />
                          </div>
                        );
                      }}
                    </For>
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
  );
}
