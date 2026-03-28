/**
 * ModRuleItem — renders a single mod rule row.
 *
 * Handles row display (icon, name, tags, actions) and the click-vs-drag threshold.
 * Alt drag-and-drop logic lives in AltSection (rendered below the row when expanded).
 */
import { For, Show, createSignal, onMount, onCleanup } from "solid-js";
import type { ModRow } from "../lib/types";
import {
  selectedIds, expandedRows, modIcons,
  functionalGroupsByBlockId, conflictModIds, conflictPairsForId, rowMap,
  aestheticGroups, removeRowsFromAestheticGroups,
  toggleSelected, toggleExpanded, functionalGroupTagClass,
  removeFunctionalGroupMember, tagFilter, functionalGroups, tagFilterForcedExpanded,
  linksByModId, removeLink, removeIncompatibility,
  setAdvancedPanelModId, selectedCount, resolvedModIds,
} from "../store";
import {
  ChevronRightIcon, AlertTriangleIcon, PackageIcon, XIcon, MaterialIcon,
} from "./icons";
import { AltSection } from "./AltSection";

interface ModRuleItemProps {
  row: ModRow;
  depth?: number;
  isLast?: boolean;
  /** Called when the user starts dragging this row (parent handles the drag session). */
  onStartDrag?: (rowId: string, event: PointerEvent | MouseEvent) => void;
  onReorderAlts?: (parentId: string, orderedIds: string[]) => void;
}

export function ModRuleItem(props: ModRuleItemProps) {
  const depth      = () => props.depth ?? 0;
  const isSelected = () => selectedIds().includes(props.row.id);
  const isExpanded = () =>
    expandedRows().includes(props.row.id) || tagFilterForcedExpanded().has(props.row.id);
  const hasAlts    = () => (props.row.alternatives?.length ?? 0) > 0;
  const isLocal    = () => props.row.kind === "local";
  const hasConflict   = () => conflictModIds().has(props.row.id);
  const conflictPairs = () => conflictPairsForId().get(props.row.id) ?? [];
  const fGroups       = () => functionalGroupsByBlockId().get(props.row.id) ?? [];
  const iconUrl    = () => props.row.modrinth_id ? modIcons().get(props.row.modrinth_id) : undefined;
  const containingGroup = () =>
    aestheticGroups().find(group => group.blockIds.includes(props.row.id)) ?? null;
  const isResolved = () => {
    const ids = resolvedModIds();
    if (ids.size === 0) return null; // resolution not yet run
    return ids.has(props.row.primaryModId ?? props.row.id);
  };

  const stopDragPropagation = (event: MouseEvent | PointerEvent) => event.stopPropagation();

  // Returns true if alt (or any of its descendants) matches the active tag filter.
  const altMatchesFilter = (alt: ModRow): boolean => {
    const tf = tagFilter();
    if (tf.size === 0) return true;
    const fg = functionalGroups();
    const check = (row: ModRow): boolean =>
      fg.some(g => tf.has(g.id) && g.modIds.includes(row.id)) ||
      (row.alternatives ?? []).some(check);
    return check(alt);
  };
  const hasVisibleAlts = () =>
    hasAlts() && (props.row.alternatives ?? []).some(altMatchesFilter);

  // ── Click-vs-drag threshold ────────────────────────────────────────────────
  const DRAG_THRESHOLD = 5;
  const [pendingClickPos, setPendingClickPos] = createSignal<{ x: number; y: number } | null>(null);

  const handleRowPointerDown = (event: PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    setPendingClickPos({ x: event.clientX, y: event.clientY });
  };

  onMount(() => {
    const onMove = (event: PointerEvent) => {
      const pending = pendingClickPos();
      if (!pending) return;
      const dx = Math.abs(event.clientX - pending.x);
      const dy = Math.abs(event.clientY - pending.y);
      if (dx > DRAG_THRESHOLD || dy > DRAG_THRESHOLD) {
        setPendingClickPos(null);
        props.onStartDrag?.(props.row.id, event);
      }
    };
    const onUp = () => {
      if (pendingClickPos()) {
        toggleSelected(props.row.id);
        setPendingClickPos(null);
      }
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup",   onUp);
    onCleanup(() => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup",   onUp);
    });
  });

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div class={depth() > 0 ? "relative ml-6 pl-4" : ""}>
      {/* Connector lines for nested depth */}
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

      {/* Main row */}
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

        {/* Info */}
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-1.5">
            <span class={`truncate font-medium ${
              isResolved() === true ? "text-green-400" : isResolved() === false ? "text-red-400" : "text-foreground"
            }`}>{props.row.name}</span>

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
                  const wins    = () => pair.winnerId === props.row.id;
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
                const icon = () =>
                  link.direction === "mutual" ? "\u21C4"
                  : link.direction === "requires" ? "\u2192"
                  : "\u2190";
                const label = () => `${icon()} ${partnerName()}`;
                const title = () =>
                  link.direction === "mutual" ? `Linked with ${partnerName()} (mutual)`
                  : link.direction === "requires" ? `Requires ${partnerName()}`
                  : `Required by ${partnerName()}`;
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

        {/* Actions */}
        <div class="flex shrink-0 items-center gap-1">
          <Show when={hasVisibleAlts()}>
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

      {/* Alternatives section (handles its own drag state) */}
      <Show when={isExpanded() && hasVisibleAlts()}>
        <AltSection
          parentRow={props.row}
          depth={depth()}
          onReorderAlts={props.onReorderAlts}
        />
      </Show>
    </div>
  );
}
