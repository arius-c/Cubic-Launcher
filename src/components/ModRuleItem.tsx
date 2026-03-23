import { For, Show, createSignal, onMount, onCleanup, batch } from "solid-js";
import type { ModRow, AestheticGroup } from "../lib/types";
import {
  selectedIds, expandedRows, modIcons,
  functionalGroupsByBlockId, conflictModIds, conflictPairsForId, rowMap,
  aestheticGroups, setAestheticGroups, modRowsState, setModRowsState, removeRowsFromAestheticGroups,
  toggleSelected, toggleExpanded, functionalGroupTagClass,
  removeFunctionalGroupMember, tagFilter, functionalGroups, sortOrder, tagFilterForcedExpanded,
  toggleGroupCollapsed, removeAestheticGroup,
  editingGroupId, groupNameDraft, setGroupNameDraft, startGroupRename, commitGroupRename,
  linksByModId, removeLink, removeIncompatibility,
  setAdvancedPanelModId, selectedCount,
} from "../store";
import {
  ChevronRightIcon, ChevronDownIcon, AlertTriangleIcon, PackageIcon, XIcon, FolderOpenIcon,
  MaterialIcon,
} from "./icons";

interface ModRuleItemProps {
  row: ModRow;
  depth?: number;
  isLast?: boolean;
  onStartDrag?: (rowId: string, event: PointerEvent | MouseEvent) => void;
  onReorderAlts?: (orderedIds: string[]) => void;
}

export function ModRuleItem(props: ModRuleItemProps) {
  const depth      = () => props.depth ?? 0;
  const isSelected = () => selectedIds().includes(props.row.id);
  const isExpanded = () => expandedRows().includes(props.row.id) || tagFilterForcedExpanded().has(props.row.id);
  const hasAlts    = () => (props.row.alternatives?.length ?? 0) > 0;
  const isLocal    = () => props.row.kind === "local";
  const hasConflict   = () => conflictModIds().has(props.row.id);
  const conflictPairs = () => conflictPairsForId().get(props.row.id) ?? [];
  const fGroups       = () => functionalGroupsByBlockId().get(props.row.id) ?? [];
  const iconUrl    = () => props.row.modrinth_id ? modIcons().get(props.row.modrinth_id) : undefined;
  const stopDragPropagation = (event: MouseEvent | PointerEvent) => event.stopPropagation();
  const containingGroup = () => aestheticGroups().find(group => group.blockIds.includes(props.row.id)) ?? null;
  // Returns true if alt (or any of its descendants) matches the active tag filter.
  const altMatchesFilter = (alt: ModRow): boolean => {
    const tf = tagFilter();
    if (tf.size === 0) return true;
    const fGroups = functionalGroups();
    const check = (row: ModRow): boolean =>
      fGroups.some(g => tf.has(g.id) && g.modIds.includes(row.id)) ||
      (row.alternatives ?? []).some(check);
    return check(alt);
  };

  const scopedAltGroups = () => {
    const so = sortOrder();
    return aestheticGroups()
      .filter(group => group.scopeRowId === props.row.id)
      .map(group => {
        let blocks = group.blockIds
          .map(id => (props.row.alternatives ?? []).find(alt => alt.id === id))
          .filter((alt): alt is ModRow => Boolean(alt))
          .filter(altMatchesFilter);
        if (so === "name-az") blocks = [...blocks].sort((a, b) => a.name.localeCompare(b.name));
        if (so === "name-za") blocks = [...blocks].sort((a, b) => b.name.localeCompare(a.name));
        return { ...group, blocks }; // blockIds spread via ...group stays original
      })
      .filter(group => group.blocks.length > 0);
  };

  // ── Row click-vs-drag ──────────────────────────────────────────────────────
  const DRAG_THRESHOLD = 5;
  const [pendingClickPos, setPendingClickPos] = createSignal<{ x: number; y: number } | null>(null);
  const handleRowPointerDown = (event: PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    setPendingClickPos({ x: event.clientX, y: event.clientY });
  };

  // ── Alt drag-and-drop ──────────────────────────────────────────────────────
  const [altDraggingId, setAltDraggingId] = createSignal<string | null>(null);
  const [altHoveredDropId, setAltHoveredDropId] = createSignal<string | null>(null);
  const [altDragPointer, setAltDragPointer] = createSignal<{ x: number; y: number } | null>(null);
  const draggingAlt = () => (props.row.alternatives ?? []).find(a => a.id === altDraggingId()) ?? null;

  // ── Alt GROUP drag-and-drop ────────────────────────────────────────────────
  let altGroupsContainerRef: HTMLDivElement | undefined;
  const [altGroupDraggingId, setAltGroupDraggingId] = createSignal<string | null>(null);
  const [altGroupHoveredDropId, setAltGroupHoveredDropId] = createSignal<string | null>(null);
  const [altGroupDragPointer, setAltGroupDragPointer] = createSignal<{ x: number; y: number } | null>(null);
  const [altTLItemHeights, setAltTLItemHeights] = createSignal<Map<string, number>>(new Map());
  const draggingAltGroupData = () => scopedAltGroups().find(g => g.id === altGroupDraggingId()) ?? null;

  const altGroupMembership = () => {
    const map = new Map<string, string>();
    for (const g of aestheticGroups().filter(g => g.scopeRowId === props.row.id)) {
      for (const id of g.blockIds) map.set(id, g.id);
    }
    return map;
  };

  // ── Unified alt top-level items system ──────────────────────────────────────
  type AltTLItem =
    | { kind: 'alt-group'; id: string; name: string; collapsed: boolean; blocks: ModRow[]; scopeRowId?: string | null; blockIds: string[] }
    | { kind: 'alt-row'; row: ModRow };

  const altTlId = (item: AltTLItem): string =>
    item.kind === 'alt-row' ? item.row.id : `alt-group:${item.id}`;

  const anyAltDragging = () => altDraggingId() != null || altGroupDraggingId() != null;

  const altTopLevelItems = (): AltTLItem[] => {
    const membership = altGroupMembership();
    const groups = scopedAltGroups();
    const groupMap = new Map(groups.map(g => [g.id, g]));
    const seenGroups = new Set<string>();
    const items: AltTLItem[] = [];

    const alts = props.row.alternatives ?? [];
    for (const alt of alts) {
      const gId = membership.get(alt.id);
      if (gId) {
        if (!seenGroups.has(gId)) {
          seenGroups.add(gId);
          const group = groupMap.get(gId);
          if (group) {
            items.push({ kind: 'alt-group', id: group.id, name: group.name, collapsed: group.collapsed, blocks: group.blocks, scopeRowId: group.scopeRowId, blockIds: group.blockIds });
          }
        }
      } else {
        // Only include if it passes display filter
        if (altMatchesFilter(alt)) {
          items.push({ kind: 'alt-row', row: alt });
        }
      }
    }

    return items;
  };

  const previewAltTLItems = (): AltTLItem[] => {
    const items = altTopLevelItems();

    // Group drag
    if (altGroupDraggingId() && altGroupHoveredDropId()) {
      const dragging = altGroupDraggingId()!;
      const drop = altGroupHoveredDropId()!;
      const draggingItem = items.find(i => i.kind === 'alt-group' && i.id === dragging);
      if (!draggingItem) return items;
      const rest = items.filter(i => !(i.kind === 'alt-group' && i.id === dragging));
      let insertIdx: number;

      if (drop.startsWith('alt-group-after:')) {
        const gId = drop.slice('alt-group-after:'.length);
        const idx = rest.findIndex(i => i.kind === 'alt-group' && i.id === gId);
        insertIdx = idx < 0 ? rest.length : idx + 1;
      } else if (drop.startsWith('alt-group:')) {
        const gId = drop.slice('alt-group:'.length);
        const idx = rest.findIndex(i => i.kind === 'alt-group' && i.id === gId);
        insertIdx = idx < 0 ? rest.length : idx;
      } else if (drop.startsWith('alt-row-after:')) {
        const rowId = drop.slice('alt-row-after:'.length);
        const idx = rest.findIndex(i => i.kind === 'alt-row' && i.row.id === rowId);
        insertIdx = idx < 0 ? rest.length : idx + 1;
      } else if (drop.startsWith('alt-row:')) {
        const rowId = drop.slice('alt-row:'.length);
        const idx = rest.findIndex(i => i.kind === 'alt-row' && i.row.id === rowId);
        insertIdx = idx < 0 ? rest.length : idx;
      } else {
        return items;
      }

      const result = [...rest];
      result.splice(insertIdx, 0, draggingItem);
      return result;
    }

    // Individual alt drag
    if (altDraggingId() && altHoveredDropId()) {
      const dragging = altDraggingId()!;
      const drop = altHoveredDropId()!;

      // Dropping into a group — no TL change
      if (drop.startsWith('alt-group-drop:')) return items;

      // Find the dragging alt in TL list (only if it's an alt-row, i.e., ungrouped)
      const draggingIdx = items.findIndex(i => i.kind === 'alt-row' && i.row.id === dragging);
      if (draggingIdx < 0) return items; // grouped alt — within-group drag, no TL change

      const draggingItem = items[draggingIdx];
      const rest = items.filter((_, idx) => idx !== draggingIdx);
      let insertIdx: number;

      if (drop.startsWith('alt-tl-group:')) {
        const gId = drop.slice('alt-tl-group:'.length);
        const idx = rest.findIndex(i => i.kind === 'alt-group' && i.id === gId);
        insertIdx = idx < 0 ? rest.length : idx;
      } else if (drop.startsWith('alt-tl-group-after:')) {
        const gId = drop.slice('alt-tl-group-after:'.length);
        const idx = rest.findIndex(i => i.kind === 'alt-group' && i.id === gId);
        insertIdx = idx < 0 ? rest.length : idx + 1;
      } else if (drop.startsWith('alt-after:')) {
        const afterId = drop.slice('alt-after:'.length);
        const idx = rest.findIndex(i => i.kind === 'alt-row' && i.row.id === afterId);
        if (idx < 0) return items;
        insertIdx = idx + 1;
      } else {
        // Before an ungrouped alt
        const idx = rest.findIndex(i => i.kind === 'alt-row' && i.row.id === drop);
        if (idx < 0) return items;
        insertIdx = idx;
      }

      const result = [...rest];
      result.splice(insertIdx, 0, draggingItem);
      return result;
    }

    return items;
  };

  const previewAltTLTranslates = (): Map<string, number> => {
    if (!anyAltDragging()) return new Map();
    // Dropping into a group — no TL change needed
    if (altHoveredDropId()?.startsWith('alt-group-drop:')) return new Map();
    // Need either group drag+drop or alt drag+drop
    if (!(altGroupDraggingId() && altGroupHoveredDropId()) && !(altDraggingId() && altHoveredDropId())) {
      return new Map();
    }

    const items = altTopLevelItems();
    const preview = previewAltTLItems();
    const heights = altTLItemHeights();
    const FALLBACK = 48;

    let nat = 0;
    const natPos = new Map<string, number>();
    for (const item of items) {
      const id = altTlId(item);
      natPos.set(id, nat);
      nat += heights.get(id) ?? FALLBACK;
    }

    let pre = 0;
    const prePos = new Map<string, number>();
    for (const item of preview) {
      const id = altTlId(item);
      prePos.set(id, pre);
      pre += heights.get(id) ?? FALLBACK;
    }

    const offsets = new Map<string, number>();
    for (const item of items) {
      const id = altTlId(item);
      offsets.set(id, (prePos.get(id) ?? 0) - (natPos.get(id) ?? 0));
    }
    return offsets;
  };

  const removeFrom = (ids: string[], id: string) => ids.filter(i => i !== id);
  const insertBefore = (ids: string[], id: string, beforeId: string) => {
    const next = removeFrom(ids, id);
    const idx = next.findIndex(i => i === beforeId);
    if (idx < 0) return [...next, id];
    next.splice(idx, 0, id);
    return next;
  };

  const commitAltDrop = (fromId: string, dropId: string) => {
    if (fromId === dropId) return;

    // Use raw aestheticGroups for correctness — scopedAltGroups() applies
    // display filters that must not affect the persisted state.
    const parentId      = props.row.id;
    const rawScoped     = aestheticGroups().filter(g => g.scopeRowId === parentId);
    const membership    = altGroupMembership();
    const groups        = rawScoped.map(g => ({ ...g, blockIds: [...g.blockIds] }));
    let ungroupedIds    = (props.row.alternatives ?? [])
      .map(a => a.id)
      .filter(id => !membership.has(id));

    // ── Remove from source ───────────────────────────────────────────
    const srcGroupId = membership.get(fromId) ?? null;
    if (srcGroupId) {
      const src = groups.find(g => g.id === srcGroupId);
      if (src) src.blockIds = removeFrom(src.blockIds, fromId);
    } else {
      ungroupedIds = removeFrom(ungroupedIds, fromId);
    }

    // ── Insert at target ─────────────────────────────────────────────
    // Track whether we need to use interleaved ordering for TL-group targets
    let useTLOrdering = false;
    let tlInsertTarget: { kind: 'before-group'; gId: string } | { kind: 'after-group'; gId: string } | null = null;

    if (dropId.startsWith("alt-group-drop:")) {
      const tgtId = dropId.replace("alt-group-drop:", "");
      const tgt = groups.find(g => g.id === tgtId);
      if (tgt) tgt.blockIds = [...removeFrom(tgt.blockIds, fromId), fromId];
    } else if (dropId.startsWith("alt-tl-group:")) {
      // Position the alt before the first member of the target group (alt stays ungrouped)
      const tgId = dropId.slice("alt-tl-group:".length);
      ungroupedIds = [...ungroupedIds, fromId];
      useTLOrdering = true;
      tlInsertTarget = { kind: 'before-group', gId: tgId };
    } else if (dropId.startsWith("alt-tl-group-after:")) {
      // Position the alt after the last member of the target group (alt stays ungrouped)
      const tgId = dropId.slice("alt-tl-group-after:".length);
      ungroupedIds = [...ungroupedIds, fromId];
      useTLOrdering = true;
      tlInsertTarget = { kind: 'after-group', gId: tgId };
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

    // ── Pre-compute new state ─────────────────────────────────────────
    // Always use interleaved ordering: walk the original alternatives array,
    // emitting each group's (updated) blockIds on first encounter and ungrouped
    // alts in their (updated) positions.  This preserves the relative layout of
    // groups vs ungrouped alts so groups don't jump to the top.
    const interleaved: string[] = [];
    const seenGroups = new Set<string>();
    const ungroupedSet = new Set(ungroupedIds);
    let ungroupedPos = 0;
    for (const alt of (props.row.alternatives ?? [])) {
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
    // Safety: add any groups not encountered in the walk
    for (const g of groups) {
      if (!seenGroups.has(g.id) && g.blockIds.length > 0) interleaved.push(...g.blockIds);
    }
    // Safety: add any ungrouped IDs not encountered
    for (const id of ungroupedIds) {
      if (!interleaved.includes(id)) interleaved.push(id);
    }

    // For TL-group targets, the dragged alt was appended to ungroupedIds above
    // but needs to be repositioned relative to the target group.
    let newOrderedIds: string[];
    if (useTLOrdering && tlInsertTarget) {
      const withoutDragged = interleaved.filter(id => id !== fromId);
      let insertIdx: number;
      const tg = groups.find(g => g.id === tlInsertTarget!.gId);
      if (tlInsertTarget.kind === 'before-group') {
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

    // Build new modRowsState with reordered alternatives
    const curRows   = modRowsState();
    const parentRow = curRows.find(r => r.id === parentId);
    const altMap    = new Map((parentRow?.alternatives ?? []).map(a => [a.id, a]));
    const newAlts   = newOrderedIds
      .map(id => altMap.get(id))
      .filter((a): a is ModRow => Boolean(a));
    const newRows   = curRows.map(r =>
      r.id !== parentId ? r : { ...r, alternatives: newAlts }
    );

    // Build new aestheticGroups — preserve non-scoped groups, update scoped ones
    const filteredGroups: AestheticGroup[] = groups
      .filter(g => g.blockIds.length > 0)
      .map(g => ({
        id: g.id, name: g.name, collapsed: g.collapsed,
        blockIds: g.blockIds, scopeRowId: g.scopeRowId,
      }));
    const newAestheticGroups = [
      ...aestheticGroups().filter(g => g.scopeRowId !== parentId),
      ...filteredGroups,
    ];

    // Atomic update — set pre-computed values directly (never function updaters)
    batch(() => {
      setAestheticGroups(newAestheticGroups);
      setModRowsState(newRows);
    });

    props.onReorderAlts?.(newOrderedIds);
  };

  const handleAltDragStart = (altId: string, event: PointerEvent | MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    setAltDraggingId(altId);
    setAltHoveredDropId(null);
    setAltDragPointer({ x: event.clientX, y: event.clientY });
    // Measure all alt TL item heights
    const heights = new Map<string, number>();
    const container = altGroupsContainerRef
      ?? document.querySelector<HTMLElement>(`[data-alt-groups-container="${props.row.id}"]`);
    if (container) {
      container.querySelectorAll<HTMLElement>('[data-alt-tl-item-id]').forEach(el => {
        heights.set(el.dataset.altTlItemId!, el.getBoundingClientRect().height);
      });
    }
    setAltTLItemHeights(heights);
    document.body.style.userSelect = "none";
  };

  const finishAltDrag = () => {
    const fromId = altDraggingId();
    const dropId = altHoveredDropId();
    if (fromId && dropId) commitAltDrop(fromId, dropId);
    setAltDraggingId(null);
    setAltHoveredDropId(null);
    setAltDragPointer(null);
    setAltTLItemHeights(new Map());
    document.body.style.userSelect = "";
  };

  const handleAltGroupDragStart = (groupId: string, event: PointerEvent | MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    const heights = new Map<string, number>();
    const container = altGroupsContainerRef
      ?? document.querySelector<HTMLElement>(`[data-alt-groups-container="${props.row.id}"]`);
    if (container) {
      container.querySelectorAll<HTMLElement>('[data-alt-tl-item-id]').forEach(el => {
        heights.set(el.dataset.altTlItemId!, el.getBoundingClientRect().height);
      });
    }
    setAltTLItemHeights(heights);
    setAltGroupDraggingId(groupId);
    setAltGroupHoveredDropId(null);
    setAltGroupDragPointer({ x: event.clientX, y: event.clientY });
    document.body.style.userSelect = "none";
  };

  const commitAltGroupDrop = (fromGroupId: string, dropId: string) => {
    const parentId = props.row.id;
    const scoped = aestheticGroups().filter(g => g.scopeRowId === parentId);
    const others = aestheticGroups().filter(g => g.scopeRowId !== parentId);
    const dragging = scoped.find(g => g.id === fromGroupId);
    if (!dragging) return;

    // Reorder aesthetic groups
    const rest = scoped.filter(g => g.id !== fromGroupId);
    let insertIdx: number;
    if (dropId.startsWith('alt-group-after:')) {
      const tgId = dropId.slice('alt-group-after:'.length);
      const idx = rest.findIndex(g => g.id === tgId);
      insertIdx = idx < 0 ? rest.length : idx + 1;
    } else if (dropId.startsWith('alt-group:')) {
      const tgId = dropId.slice('alt-group:'.length);
      const idx = rest.findIndex(g => g.id === tgId);
      insertIdx = idx < 0 ? rest.length : idx;
    } else {
      // Dropped near ungrouped alt — keep group order unchanged but reorder alternatives array
      insertIdx = rest.length; // append at end
    }
    const reordered = [...rest];
    reordered.splice(insertIdx, 0, dragging);

    // Also reorder alternatives array so group members move to the correct position
    const groupMemberIds = new Set(dragging.blockIds);
    const curRows = modRowsState();
    const parentRow = curRows.find(r => r.id === parentId);
    if (!parentRow) return;
    const curAlts = parentRow.alternatives ?? [];
    const groupMembers = dragging.blockIds
      .map(id => curAlts.find(a => a.id === id))
      .filter((a): a is ModRow => Boolean(a));
    const otherAlts = curAlts.filter(a => !groupMemberIds.has(a.id));

    let altInsertIdx: number;
    if (dropId.startsWith('alt-group-after:')) {
      const tgId = dropId.slice('alt-group-after:'.length);
      const tg = scoped.find(g => g.id === tgId);
      let last = -1;
      if (tg) for (let i = otherAlts.length - 1; i >= 0; i--) {
        if (tg.blockIds.includes(otherAlts[i].id)) { last = i; break; }
      }
      altInsertIdx = last < 0 ? otherAlts.length : last + 1;
    } else if (dropId.startsWith('alt-group:')) {
      const tgId = dropId.slice('alt-group:'.length);
      const tg = scoped.find(g => g.id === tgId);
      const first = tg?.blockIds[0];
      altInsertIdx = first ? otherAlts.findIndex(a => a.id === first) : otherAlts.length;
      if (altInsertIdx < 0) altInsertIdx = otherAlts.length;
    } else if (dropId.startsWith('alt-row-after:')) {
      const rowId = dropId.slice('alt-row-after:'.length);
      const idx = otherAlts.findIndex(a => a.id === rowId);
      altInsertIdx = idx < 0 ? otherAlts.length : idx + 1;
    } else if (dropId.startsWith('alt-row:')) {
      const rowId = dropId.slice('alt-row:'.length);
      const idx = otherAlts.findIndex(a => a.id === rowId);
      altInsertIdx = idx < 0 ? otherAlts.length : idx;
    } else {
      altInsertIdx = otherAlts.length;
    }

    const newAlts = [...otherAlts];
    newAlts.splice(altInsertIdx, 0, ...groupMembers);
    const newRows = curRows.map(r =>
      r.id !== parentId ? r : { ...r, alternatives: newAlts }
    );

    batch(() => {
      setAestheticGroups([...others, ...reordered]);
      setModRowsState(newRows);
    });
  };

  const finishAltGroupDrag = () => {
    const fromId = altGroupDraggingId();
    const dropId = altGroupHoveredDropId();
    if (fromId && dropId) commitAltGroupDrop(fromId, dropId);
    setAltGroupDraggingId(null);
    setAltGroupHoveredDropId(null);
    setAltGroupDragPointer(null);
    setAltTLItemHeights(new Map());
    document.body.style.userSelect = "";
  };

  onMount(() => {
    const onMove = (event: PointerEvent) => {
      // ── Group drag: detect at TL item level ─────────────────────────
      if (altGroupDraggingId()) {
        setAltGroupDragPointer({ x: event.clientX, y: event.clientY });
        const el = document.elementFromPoint(event.clientX, event.clientY);
        const tlEl = el?.closest('[data-alt-tl-item-id]') as HTMLElement | null;
        if (tlEl) {
          const itemId = tlEl.dataset.altTlItemId!;
          if (itemId === `alt-group:${altGroupDraggingId()}`) return;
          const rect = tlEl.getBoundingClientRect();
          const isBottom = event.clientY >= rect.top + rect.height / 2;
          if (itemId.startsWith('alt-group:')) {
            const gId = itemId.slice('alt-group:'.length);
            setAltGroupHoveredDropId(isBottom ? `alt-group-after:${gId}` : `alt-group:${gId}`);
          } else {
            // Ungrouped alt row
            setAltGroupHoveredDropId(isBottom ? `alt-row-after:${itemId}` : `alt-row:${itemId}`);
          }
        }
        return;
      }

      // ── Click-vs-drag threshold ─────────────────────────────────────
      const pending = pendingClickPos();
      if (pending) {
        const dx = Math.abs(event.clientX - pending.x);
        const dy = Math.abs(event.clientY - pending.y);
        if (dx > DRAG_THRESHOLD || dy > DRAG_THRESHOLD) {
          setPendingClickPos(null);
          props.onStartDrag?.(props.row.id, event);
        }
      }

      // ── Individual alt drag: TL-item-aware detection ────────────────
      if (altDraggingId()) {
        setAltDragPointer({ x: event.clientX, y: event.clientY });
        const el = document.elementFromPoint(event.clientX, event.clientY);

        // First, find the nearest TL item (not the dragging item)
        const tlEl = el?.closest('[data-alt-tl-item-id]') as HTMLElement | null;
        if (tlEl) {
          const itemId = tlEl.dataset.altTlItemId!;

          if (itemId.startsWith('alt-group:')) {
            // Hovering over a group
            const gId = itemId.slice('alt-group:'.length);
            const rect = tlEl.getBoundingClientRect();
            const relY = event.clientY - rect.top;

            if (relY < rect.height * 0.2) {
              // Top 20% → before group
              setAltHoveredDropId(`alt-tl-group:${gId}`);
            } else if (relY > rect.height * 0.8) {
              // Bottom 20% → after group
              setAltHoveredDropId(`alt-tl-group-after:${gId}`);
            } else {
              // Middle → find nearest [data-alt-id] within the group for fine-grained positioning
              const altEls = Array.from(tlEl.querySelectorAll<HTMLElement>('[data-alt-id]'))
                .filter(ael => ael.dataset.altId !== altDraggingId());
              if (altEls.length > 0) {
                let nearest = altEls[0];
                for (const ael of altEls) {
                  const rNearest = nearest.getBoundingClientRect();
                  const rEl = ael.getBoundingClientRect();
                  if (Math.abs(event.clientY - (rEl.top + rEl.height / 2)) <
                      Math.abs(event.clientY - (rNearest.top + rNearest.height / 2))) {
                    nearest = ael;
                  }
                }
                const hId = nearest.dataset.altId!;
                const altRect = nearest.getBoundingClientRect();
                const isBottom = event.clientY >= altRect.top + altRect.height / 2;
                setAltHoveredDropId(isBottom ? `alt-after:${hId}` : hId);
              } else {
                // No alt items in group — drop into group
                const srcGroupId = altGroupMembership().get(altDraggingId()!);
                if (srcGroupId !== gId) {
                  setAltHoveredDropId(`alt-group-drop:${gId}`);
                }
              }
            }
          } else {
            // Hovering over an ungrouped alt TL item
            // Find the [data-alt-id] within
            const altEl = tlEl.querySelector<HTMLElement>('[data-alt-id]');
            if (altEl && altEl.dataset.altId !== altDraggingId()) {
              const hId = altEl.dataset.altId!;
              const altRect = altEl.getBoundingClientRect();
              const isBottom = event.clientY >= altRect.top + altRect.height / 2;
              setAltHoveredDropId(isBottom ? `alt-after:${hId}` : hId);
            }
          }
        } else {
          setAltHoveredDropId(null);
        }
      }
    };
    const onUp = () => {
      if (pendingClickPos()) {
        toggleSelected(props.row.id);
        setPendingClickPos(null);
      }
      if (altGroupDraggingId()) finishAltGroupDrag();
      else if (altDraggingId()) finishAltDrag();
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    onCleanup(() => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      document.body.style.userSelect = "";
    });
  });

  // ── Live preview order for within-group alts while dragging ────────────────
  const ALT_ITEM_HEIGHT = 48; // px — must match py-2 (16px) + h-8 icon (32px)

  const previewGroupedOrder = (groupId: string): string[] => {
    const group = scopedAltGroups().find(g => g.id === groupId);
    if (!group) return [];
    const ids = group.blocks.map(a => a.id);
    const dragging = altDraggingId();
    const drop = altHoveredDropId();
    if (!dragging || !ids.includes(dragging) || !drop) return ids;

    let insertIdx: number;
    if (drop === `alt-group-drop:${groupId}`) {
      insertIdx = ids.length;
    } else if (drop.startsWith("alt-after:")) {
      const afterId = drop.replace("alt-after:", "");
      const ai = ids.indexOf(afterId);
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

  return (
    <div class={depth() > 0 ? "relative ml-6 pl-4" : ""}>
      {/* ── Alt GROUP drag ghost ─────────────────────────────────────── */}
      <Show when={altGroupDraggingId() && altGroupDragPointer()}>
        <div
          class="pointer-events-none fixed z-[100] w-60 rounded-md border border-primary/40 bg-card shadow-2xl ring-1 ring-primary/20"
          style={{ left: `${altGroupDragPointer()!.x + 12}px`, top: `${altGroupDragPointer()!.y - 16}px` }}
        >
          <div class="flex items-center gap-2 px-3 py-2">
            <FolderOpenIcon class="h-4 w-4 shrink-0 text-primary" />
            <span class="truncate text-sm font-medium text-foreground">{draggingAltGroupData()?.name}</span>
            <span class="ml-1 shrink-0 text-xs text-muted-foreground">{draggingAltGroupData()?.blocks.length} mods</span>
          </div>
        </div>
      </Show>

      {/* ── Alt drag ghost (full row replica) ───────────────────────── */}
      <Show when={altDraggingId() && altDragPointer()}>
        <div
          class="pointer-events-none fixed z-50 w-80 rounded-md border border-primary/40 bg-card shadow-2xl ring-1 ring-primary/20"
          style={{ left: `${altDragPointer()!.x + 12}px`, top: `${altDragPointer()!.y - 24}px`, opacity: "0.95" }}
        >
          <div class="flex items-center gap-3 px-2 py-2">
            <div class="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted">
              <Show
                when={draggingAlt()?.modrinth_id && modIcons().get(draggingAlt()!.modrinth_id!)}
                fallback={<PackageIcon class="h-4 w-4 text-muted-foreground" />}
              >
                <img
                  src={modIcons().get(draggingAlt()!.modrinth_id!)!}
                  alt={draggingAlt()?.name}
                  class="h-8 w-8 object-cover"
                />
              </Show>
            </div>
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-1.5">
                <span class="truncate font-medium text-foreground">{draggingAlt()?.name}</span>
                <span class="inline-flex shrink-0 items-center rounded-md border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-medium text-secondary-foreground">
                  {draggingAlt()?.kind === "local" ? "Local" : "Modrinth"}
                </span>
              </div>
            </div>
          </div>
        </div>
      </Show>

      <Show when={depth() > 0}>
        <div class="pointer-events-none absolute inset-y-0 left-0 w-4">
          <Show
            when={!props.isLast}
            fallback={<div class="absolute left-2 top-0 h-7 w-px bg-border/35" />}
          >
            <div class="absolute bottom-0 left-2 top-0 w-px bg-border/35" />
          </Show>
          <div class="absolute left-2 top-7 h-px w-3 bg-border/35" />
        </div>
      </Show>

      {/* ── Main row ─────────────────────────────────────────────────── */}
      <div
        class={`group flex items-center gap-3 rounded-md px-2 py-2 transition-colors select-none ${
          props.onStartDrag ? "cursor-grab active:cursor-grabbing" : "cursor-pointer"
        } ${
          isSelected() ? "bg-primary/10 ring-1 ring-primary/20" : "hover:bg-muted/50"
        }`}
        onPointerDown={handleRowPointerDown}
      >

        {/* Mod icon */}
        <div class="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted">
          <Show when={iconUrl()} fallback={<PackageIcon class="h-4 w-4 text-muted-foreground" />}>
            <img
              src={iconUrl()!}
              alt={props.row.name}
              class="h-8 w-8 object-cover"
              onError={e => { e.currentTarget.style.display = "none"; }}
            />
          </Show>
        </div>

        {/* Info ────────────────────────────────────────────────────── */}
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-1.5">
            <span class="truncate font-medium text-foreground">{props.row.name}</span>

            {/* Source badge */}
            <span class={`inline-flex items-center rounded-md border px-1.5 py-0.5 text-[10px] font-medium ${
              isLocal()
                ? "border-warning/40 bg-warning/10 text-warning"
                : "border-border bg-secondary text-secondary-foreground"
            }`}>
              {isLocal() ? "Local" : "Modrinth"}
            </span>

            {/* Local dependency warning */}
            <Show when={isLocal()}>
              <span
                title="Manual mod — verify and add required dependencies yourself"
                class="inline-flex items-center gap-0.5 rounded-md border border-warning/40 bg-warning/10 px-1.5 py-0.5 text-[10px] font-medium text-warning"
              >
                <AlertTriangleIcon class="h-3 w-3" />
                Verify deps
              </span>
            </Show>

            {/* Conflict pills */}
            <Show when={hasConflict()}>
              <For each={conflictPairs()}>
                {pair => {
                  const wins = () => pair.winnerId === props.row.id;
                  const otherId = () => wins() ? pair.loserId : pair.winnerId;
                  const otherName = () => rowMap().get(otherId())?.name ?? otherId();
                  return (
                    <span
                      title={wins() ? `Wins against ${otherName()}` : `Loses to ${otherName()}`}
                      class={`inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[10px] font-medium ring-1 ${
                        wins()
                          ? "bg-green-500/10 text-green-500 ring-green-500/30"
                          : "bg-red-500/10 text-red-500 ring-red-500/30"
                      }`}
                    >
                      {otherName()}
                      <button
                        onClick={e => { e.stopPropagation(); removeIncompatibility(props.row.id, otherId()); }}
                        onPointerDown={stopDragPropagation}
                        class="opacity-60 hover:opacity-100 transition-opacity"
                        title="Remove incompatibility"
                      >
                        <XIcon class="h-2.5 w-2.5" />
                      </button>
                    </span>
                  );
                }}
              </For>
            </Show>

            {/* Tags */}
            <For each={props.row.tags.filter(t => t !== "Alternative" && t !== "Conflict Set")}>
              {tag => (
                <span class="inline-flex items-center rounded-md border border-border bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                  {tag}
                </span>
              )}
            </For>

            {/* Functional group tags */}
            <For each={fGroups()}>
              {g => (
                <span class={functionalGroupTagClass(g.tone)}>
                  {g.name}
                  <button
                    onClick={e => { e.stopPropagation(); removeFunctionalGroupMember(g.id, props.row.id); }}
                    onPointerDown={stopDragPropagation}
                    onMouseDown={stopDragPropagation}
                    class="ml-0.5 opacity-40 hover:opacity-100 transition-opacity"
                    title={`Remove from "${g.name}"`}
                  >
                    <XIcon class="h-2.5 w-2.5" />
                  </button>
                </span>
              )}
            </For>

            {/* Link tags */}
            <For each={linksByModId().get(props.row.id) ?? []}>
              {link => {
                const partnerName = () => rowMap().get(link.partnerId)?.name ?? link.partnerId;
                const icon = () => link.direction === 'mutual' ? '\u21C4' : link.direction === 'requires' ? '\u2192' : '\u2190';
                const label = () => `${icon()} ${partnerName()}`;
                const title = () => {
                  if (link.direction === 'mutual') return `Linked with ${partnerName()} (mutual)`;
                  if (link.direction === 'requires') return `Requires ${partnerName()}`;
                  return `Required by ${partnerName()}`;
                };
                return (
                  <span
                    title={title()}
                    class="inline-flex items-center gap-0.5 rounded-md border border-cyan-500/30 bg-cyan-500/10 px-1.5 py-0.5 text-[10px] font-medium text-cyan-400"
                  >
                    <MaterialIcon name="link" size="sm" class="-ml-0.5" />
                    {label()}
                    <button
                      onClick={e => { e.stopPropagation(); removeLink(props.row.id, link.partnerId); }}
                      onPointerDown={stopDragPropagation}
                      onMouseDown={stopDragPropagation}
                      class="ml-0.5 opacity-40 hover:opacity-100 transition-opacity"
                      title={`Remove link with "${partnerName()}"`}
                    >
                      <XIcon class="h-2.5 w-2.5" />
                    </button>
                  </span>
                );
              }}
            </For>
          </div>

          <Show when={props.row.modrinth_id && !isLocal()}>
            <div class="mt-0.5 text-xs text-muted-foreground/60">{props.row.modrinth_id}</div>
          </Show>
        </div>

        {/* Actions ─────────────────────────────────────────────────── */}
        <div class="flex shrink-0 items-center gap-1">
          <Show when={hasAlts()}>
            <button
              onClick={() => toggleExpanded(props.row.id)}
              onPointerDown={stopDragPropagation}
              onMouseDown={stopDragPropagation}
              class={`flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium transition-colors ${
                isExpanded()
                  ? "bg-primary/15 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              }`}
              title={isExpanded() ? "Collapse alternatives" : "Show alternatives"}
            >
              <ChevronRightIcon
                class={`h-3.5 w-3.5 transition-transform duration-150 ${isExpanded() ? "rotate-90" : ""}`}
              />
              {props.row.alternatives!.length} alt{props.row.alternatives!.length > 1 ? "s" : ""}
            </button>
          </Show>

          <Show when={containingGroup()}>
            <button
              onClick={() => removeRowsFromAestheticGroups([props.row.id])}
              onPointerDown={stopDragPropagation}
              onMouseDown={stopDragPropagation}
              class="flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              title={`Remove from ${containingGroup()!.name}`}
            >
              <XIcon class="h-3.5 w-3.5" />
              Ungroup
            </button>
          </Show>

          <button
            onClick={e => { e.stopPropagation(); setAdvancedPanelModId(props.row.id); }}
            onPointerDown={stopDragPropagation}
            onMouseDown={stopDragPropagation}
            disabled={selectedCount() >= 2}
            class="flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-40 disabled:cursor-not-allowed"
            title="Advanced mod settings"
          >
            <MaterialIcon name="settings" size="sm" />
            Advanced
          </button>
        </div>
      </div>

      {/* ── Alternatives (expanded) ─────────────────────────────────── */}
      <Show when={isExpanded() && hasAlts()}>
        <div class="mt-0.5 pb-1" data-alt-groups-container={props.row.id} ref={altGroupsContainerRef}>
          <For each={altTopLevelItems()}>
            {(item, tlIndex) => {
              const id = altTlId(item);
              const isDraggingThis = () => {
                if (item.kind === 'alt-row') return altDraggingId() === item.row.id;
                return altGroupDraggingId() === item.id;
              };
              const tlOffset = () => isDraggingThis() ? 0 : (previewAltTLTranslates().get(id) ?? 0);
              const isLastTLItem = () => tlIndex() === altTopLevelItems().length - 1;

              if (item.kind === 'alt-row') {
                // Render ungrouped alt
                const alt = item.row;
                const isTarget = () => !isDraggingThis() && (altHoveredDropId() === alt.id || altHoveredDropId() === `alt-after:${alt.id}`);
                return (
                  <div
                    data-alt-tl-item-id={id}
                    data-alt-id={alt.id}
                    style={{
                      transform: anyAltDragging() ? `translateY(${tlOffset()}px)` : "none",
                      transition: anyAltDragging() ? "transform 150ms ease" : "none",
                      position: "relative",
                      "z-index": isDraggingThis() ? "0" : "1",
                    }}
                    class={isDraggingThis() ? "opacity-0" : isTarget() ? "rounded-md ring-1 ring-primary/40" : ""}
                  >
                    <ModRuleItem
                      row={alt}
                      depth={depth() + 1}
                      isLast={isLastTLItem()}
                      onStartDrag={handleAltDragStart}
                      onReorderAlts={(orderedIds) => {
                        setModRowsState(cur => cur.map(topRow => {
                          if (topRow.id !== props.row.id) return topRow;
                          return { ...topRow, alternatives: (topRow.alternatives ?? []).map(a => {
                            if (a.id !== alt.id) return a;
                            const sub = new Map((a.alternatives ?? []).map(x => [x.id, x]));
                            return { ...a, alternatives: orderedIds.map(xid => sub.get(xid)).filter((x): x is ModRow => !!x) };
                          })};
                        }));
                      }}
                    />
                  </div>
                );
              }

              // Render group
              const group = item;
              const isGroupDropTarget = () => altDraggingId() && altHoveredDropId() === `alt-group-drop:${group.id}`;
              const isBeforeTarget = () => altGroupHoveredDropId() === `alt-group:${group.id}` || altHoveredDropId() === `alt-tl-group:${group.id}`;
              const isAfterTarget = () => altGroupHoveredDropId() === `alt-group-after:${group.id}` || altHoveredDropId() === `alt-tl-group-after:${group.id}`;

              return (
                <div
                  data-alt-tl-item-id={id}
                  data-alt-group-id={group.id}
                  data-scoped-alt-group-id={group.id}
                  class={`relative ml-6 pl-4 ${isLastTLItem() ? "" : "mb-3"} ${isDraggingThis() ? "opacity-0 pointer-events-none" : ""}`}
                  style={{
                    transform: anyAltDragging() && !isDraggingThis()
                      ? `translateY(${tlOffset()}px)`
                      : "none",
                    transition: anyAltDragging() ? "transform 150ms ease" : "none",
                    position: "relative",
                    "z-index": isDraggingThis() ? "0" : "1",
                  }}
                >
                  {/* connector column — outside the group box */}
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
                  {/* group box */}
                  <div class={`rounded-xl border bg-muted/10 p-2 shadow-sm transition-colors ${
                    isGroupDropTarget()
                      ? "border-primary/40 bg-primary/5"
                      : "border-border/70"
                  }`}>
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
                        onPointerDown={(e) => handleAltGroupDragStart(group.id, e)}
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
                      <span class="text-[10px] text-muted-foreground">{group.blocks.length} mods</span>
                      <button
                        onClick={() => removeAestheticGroup(group.id)}
                        class="flex h-5 w-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                        title="Remove group"
                      >
                        <XIcon class="h-3 w-3" />
                      </button>
                    </div>
                    <Show when={!group.collapsed}>
                      <div class="space-y-1" data-alt-container>
                        <For each={group.blocks}>
                          {(alt, index) => {
                            const isDragging = () => altDraggingId() === alt.id;
                            const isTarget = () => !isDragging() && (altHoveredDropId() === alt.id || altHoveredDropId() === `alt-after:${alt.id}`);
                            const previewIdx = () => {
                              if (!altDraggingId()) return index();
                              const order = previewGroupedOrder(group.id);
                              const i = order.indexOf(alt.id);
                              return i >= 0 ? i : index();
                            };
                            const offset = () => isDragging() ? 0 : (previewIdx() - index()) * ALT_ITEM_HEIGHT;
                            return (
                              <div
                                data-alt-id={alt.id}
                                data-alt-index={String(index())}
                                style={{
                                  transform: altDraggingId() ? `translateY(${offset()}px)` : "none",
                                  transition: altDraggingId() ? "transform 150ms ease" : "none",
                                  position: "relative",
                                  "z-index": isDragging() ? "0" : "1",
                                }}
                                class={isDragging() ? "opacity-0" : isTarget() ? "rounded-md ring-1 ring-primary/40" : ""}
                              >
                                <ModRuleItem row={alt} depth={0} onStartDrag={handleAltDragStart}
                                  onReorderAlts={(orderedIds) => {
                                    setModRowsState(cur => cur.map(topRow => {
                                      if (topRow.id !== props.row.id) return topRow;
                                      return { ...topRow, alternatives: (topRow.alternatives ?? []).map(a => {
                                        if (a.id !== alt.id) return a;
                                        const sub = new Map((a.alternatives ?? []).map(x => [x.id, x]));
                                        return { ...a, alternatives: orderedIds.map(xid => sub.get(xid)).filter((x): x is ModRow => !!x) };
                                      })};
                                    }));
                                  }}
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
      </Show>
    </div>
  );
}
