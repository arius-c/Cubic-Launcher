/**
 * ModListEditor — scrollable mod rule list with top-level drag-and-drop.
 *
 * Uses the unified useDragEngine for all drag operations.
 * Drop-target detection is fully position-based (no elementFromPoint).
 * Alt drag-and-drop is handled per-row inside AltSection.
 */
import { For, Show, createMemo, createSignal, createEffect, batch, onMount, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { ModRow } from "../lib/types";
import type { TopLevelItem } from "../store";
import { appendDebugTrace } from "../lib/debugTrace";
import {
  topLevelItems, modListCards, selectedModListName, search, modRowsState, setModRowsState,
  aestheticGroups, setAestheticGroups, activeAccount, modIcons,
  editingGroupId, groupNameDraft, setGroupNameDraft,
  toggleGroupCollapsed, startGroupRename, commitGroupRename, removeAestheticGroup, onToggleEnabled,
  activeContentTab, setActiveContentTab, setAddModModalOpen,
  selectedMcVersion, selectedModLoader,
} from "../store";
import type { ContentTabId } from "../store";
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
  enabled: boolean;
  onToggleEnabled: () => void;
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
        onClick={props.onToggleEnabled}
        class={`flex h-4 w-7 items-center rounded-full px-[3px] transition-colors ${props.enabled ? "bg-green-500/80" : "bg-muted"}`}
        title={props.enabled ? "Group enabled — click to disable all mods" : "Group disabled — click to enable all mods"}
      >
        <div class={`h-2.5 w-2.5 rounded-full bg-white shadow transition-transform ${props.enabled ? "translate-x-[12px]" : "translate-x-0"}`} />
      </button>
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

// ── Content tab view (resource packs, data packs, shaders) ─────────────────

// Global counter that increments when content is added, so sections reload.
const [contentVersion, setContentVersion] = createSignal(0);
export function bumpContentVersion() { setContentVersion(v => v + 1); }

type ContentEntry = { id: string; source: string; versionRules: Array<{ kind: string; mcVersions: string[]; loader: string }> };
type ContentGroupData = { id: string; name: string; collapsed: boolean; entryIds: string[] };
type ContentTopLevelItem =
  | { kind: "entry"; entry: ContentEntry }
  | { kind: "group"; id: string; name: string; collapsed: boolean; entries: ContentEntry[] };

const CONTENT_TAB_LABELS: Record<string, string> = {
  resourcepack: "Resource Packs",
  datapack: "Data Packs",
  shader: "Shaders",
};

// ── Content Advanced Panel (version rules only) ──────────────────────────────
import { MOD_LOADERS } from "../lib/types";
import { minecraftVersions, mcWithSnapshots, showSnapshots, setShowSnapshots } from "../store";

function ContentAdvancedPanel(props: {
  entry: ContentEntry;
  name: string;
  modlistName: string;
  contentType: string;
  onClose: () => void;
  onUpdate: (entryId: string, rules: ContentEntry["versionRules"]) => void;
}) {
  const ALL_LOADERS = ["any", ...MOD_LOADERS];
  const [rules, setRules] = createSignal(props.entry.versionRules);
  const [addingRule, setAddingRule] = createSignal(false);
  const [draftKind, setDraftKind] = createSignal<"exclude" | "only">("exclude");
  const [draftVersions, setDraftVersions] = createSignal<string[]>([]);
  const [draftLoader, setDraftLoader] = createSignal("any");

  const versions = () => showSnapshots() ? mcWithSnapshots() : minecraftVersions();

  const commitRule = () => {
    if (draftVersions().length === 0) return;
    const updated = [...rules(), { kind: draftKind(), mcVersions: draftVersions(), loader: draftLoader() }];
    setRules(updated);
    setAddingRule(false);
    setDraftVersions([]);
    setDraftLoader("any");
    setDraftKind("exclude");
    void save(updated);
  };

  const removeRule = (idx: number) => {
    const updated = rules().filter((_, i) => i !== idx);
    setRules(updated);
    void save(updated);
  };

  const save = async (vr: ContentEntry["versionRules"]) => {
    try {
      await invoke("save_content_version_rules_command", {
        input: {
          modlistName: props.modlistName,
          contentType: props.contentType,
          entryId: props.entry.id,
          versionRules: vr.map(r => ({ kind: r.kind, mc_versions: r.mcVersions, loader: r.loader })),
        },
      });
      props.onUpdate(props.entry.id, vr);
    } catch { /* */ }
  };

  return (
    <div
      class="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-black/60 px-4 py-8 backdrop-blur-sm"
      onClick={e => { if (e.target === e.currentTarget) props.onClose(); }}
    >
      <div class="flex w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xl">
        <div class="flex items-center justify-between border-b border-border px-6 py-4 shrink-0">
          <div>
            <h2 class="text-lg font-semibold text-foreground">Advanced</h2>
            <p class="text-sm text-muted-foreground truncate max-w-md">{props.name}</p>
          </div>
          <button onClick={props.onClose} class="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground">
            <XIcon class="h-4 w-4" />
          </button>
        </div>
        <div class="flex-1 overflow-y-auto max-h-[70vh]">
          <div class="px-5 py-2 bg-muted/30 border-b border-border">
            <h3 class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Version Rules</h3>
          </div>
          <div class="p-4 space-y-2">
            <For each={rules()}>
              {(rule, idx) => (
                <div class="flex flex-wrap items-center gap-2 rounded-md border border-border bg-background p-2">
                  <select
                    value={rule.kind}
                    onChange={e => {
                      const updated = rules().map((r, i) => i === idx() ? { ...r, kind: e.currentTarget.value } : r);
                      setRules(updated);
                      void save(updated);
                    }}
                    class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                  >
                    <option value="exclude">Exclude when</option>
                    <option value="only">Only when</option>
                  </select>
                  <select
                    value={rule.mcVersions[0] ?? ""}
                    onChange={e => {
                      const updated = rules().map((r, i) => i === idx() ? { ...r, mcVersions: [e.currentTarget.value] } : r);
                      setRules(updated);
                      void save(updated);
                    }}
                    class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                  >
                    <option value="">Any version</option>
                    <For each={versions()}>{v => <option value={v}>{v}</option>}</For>
                  </select>
                  <select
                    value={rule.loader}
                    onChange={e => {
                      const updated = rules().map((r, i) => i === idx() ? { ...r, loader: e.currentTarget.value } : r);
                      setRules(updated);
                      void save(updated);
                    }}
                    class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                  >
                    <For each={ALL_LOADERS}>{l => <option value={l}>{l === "any" ? "Any loader" : l}</option>}</For>
                  </select>
                  <button onClick={() => removeRule(idx())} class="ml-auto flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-destructive/10 hover:text-destructive">
                    <XIcon class="h-3.5 w-3.5" />
                  </button>
                </div>
              )}
            </For>

            <Show when={addingRule()} fallback={
              <button onClick={() => setAddingRule(true)} class="flex items-center gap-1.5 text-xs text-primary hover:text-primary/80 transition-colors">
                <MaterialIcon name="add" size="sm" />
                Add Version Rule
              </button>
            }>
              <div class="rounded-md border border-primary/30 bg-primary/5 p-3 space-y-2">
                <div class="flex flex-wrap items-center gap-2">
                  <select value={draftKind()} onChange={e => setDraftKind(e.currentTarget.value as "exclude" | "only")} class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground">
                    <option value="exclude">Exclude when</option>
                    <option value="only">Only when</option>
                  </select>
                  <select value={draftVersions()[0] ?? ""} onChange={e => setDraftVersions(e.currentTarget.value ? [e.currentTarget.value] : [])} class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground">
                    <option value="">Select version...</option>
                    <For each={versions()}>{v => <option value={v}>{v}</option>}</For>
                  </select>
                  <select value={draftLoader()} onChange={e => setDraftLoader(e.currentTarget.value)} class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground">
                    <For each={ALL_LOADERS}>{l => <option value={l}>{l === "any" ? "Any loader" : l}</option>}</For>
                  </select>
                </div>
                <div class="flex items-center gap-2">
                  <label class="flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer">
                    <input type="checkbox" checked={showSnapshots()} onChange={e => setShowSnapshots(e.currentTarget.checked)} class="rounded border-border" />
                    Show Snapshots
                  </label>
                </div>
                <div class="flex gap-2">
                  <button onClick={commitRule} disabled={draftVersions().length === 0} class="rounded-md bg-primary px-3 py-1 text-xs text-white disabled:opacity-50">Add</button>
                  <button onClick={() => setAddingRule(false)} class="rounded-md bg-secondary px-3 py-1 text-xs text-secondary-foreground">Cancel</button>
                </div>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Content Tab View ────────────────────────────────────────────────────────
function ContentTabView(props: { type: string; modlistName: string; onAddContent: () => void }) {
  let listContainerRef: HTMLDivElement | undefined;

  const [entries, setEntries] = createSignal<ContentEntry[]>([]);
  const [groups, setGroups] = createSignal<ContentGroupData[]>([]);
  const [meta, setMeta] = createSignal<Map<string, { name: string; iconUrl?: string }>>(new Map());
  const [selectedIds, setSelectedIds] = createSignal<Set<string>>(new Set());
  const [cEditingGroupId, setCEditingGroupId] = createSignal<string | null>(null);
  const [cGroupNameDraft, setCGroupNameDraft] = createSignal("");
  const [advancedEntryId, setAdvancedEntryId] = createSignal<string | null>(null);

  const label = () => CONTENT_TAB_LABELS[props.type] ?? props.type;

  const load = async () => {
    if (!props.modlistName) return;
    try {
      const snap: any = await invoke("load_content_list_command", { input: { modlistName: props.modlistName, contentType: props.type } });
      const list: ContentEntry[] = (snap.entries ?? []).map((e: any) => ({
        id: e.id,
        source: e.source,
        versionRules: (e.versionRules ?? []).map((vr: any) => ({ kind: vr.kind, mcVersions: vr.mcVersions, loader: vr.loader })),
      }));
      const grps: ContentGroupData[] = snap.groups ?? [];
      setEntries(list);
      setGroups(grps);
      const modrinthIds = list.filter(e => e.source === "modrinth").map(e => e.id);
      if (modrinthIds.length > 0) {
        try {
          const param = encodeURIComponent(JSON.stringify(modrinthIds));
          const res = await fetch(`https://api.modrinth.com/v2/projects?ids=${param}`, { headers: { "User-Agent": "CubicLauncher/0.1.0" } });
          if (res.ok) {
            const projects: Array<{ id: string; slug: string; title: string; icon_url?: string | null }> = await res.json();
            const m = new Map<string, { name: string; iconUrl?: string }>();
            for (const p of projects) {
              if (p.slug) m.set(p.slug, { name: p.title, iconUrl: p.icon_url ?? undefined });
              if (p.id) m.set(p.id, { name: p.title, iconUrl: p.icon_url ?? undefined });
            }
            setMeta(m);
          }
        } catch { /* best effort */ }
      }
    } catch { setEntries([]); setGroups([]); }
  };

  createEffect(() => { contentVersion(); if (props.modlistName) void load(); });

  // ── Group membership ───────────────────────────────────────────────────
  const groupByEntryId = () => {
    const map = new Map<string, string>();
    for (const g of groups()) for (const id of g.entryIds) map.set(id, g.id);
    return map;
  };

  // ── Top-level items ────────────────────────────────────────────────────
  const contentTLItems = createMemo((): ContentTopLevelItem[] => {
    const grps = groups();
    const allEntries = entries();
    const entryMap = new Map(allEntries.map(e => [e.id, e]));
    const items: ContentTopLevelItem[] = [];
    const emittedGroups = new Set<string>();
    for (const entry of allEntries) {
      const gId = groupByEntryId().get(entry.id);
      if (gId) {
        if (!emittedGroups.has(gId)) {
          emittedGroups.add(gId);
          const g = grps.find(gg => gg.id === gId)!;
          items.push({
            kind: "group", id: g.id, name: g.name, collapsed: g.collapsed,
            entries: g.entryIds.map(eid => entryMap.get(eid)).filter((e): e is ContentEntry => !!e),
          });
        }
      } else {
        items.push({ kind: "entry", entry });
      }
    }
    return items;
  });

  const ctlId = (item: ContentTopLevelItem) => item.kind === "entry" ? item.entry.id : `group:${item.id}`;

  // ── Drag engine ────────────────────────────────────────────────────────
  const engine = useDragEngine({
    containerRef: () => listContainerRef,
    getItems: () => contentTLItems().map((item): DragItem =>
      item.kind === "entry"
        ? { kind: "row", id: item.entry.id }
        : { kind: "group", id: item.id }
    ),
    onCommit: (fromId, dropId, fromKind) => commitContentDrop(fromId, dropId, fromKind),
  });

  const isDraggingGroup = () => engine.draggingKind() === "group";

  // ── Enhanced drop detection (same logic as mods) ───────────────────────
  const detectContentDropTarget = (cursorY: number): string | null => {
    const items = contentTLItems();
    const heights = engine.cachedHeights();
    const tops = engine.cachedTops();
    const dragId = engine.draggingId()!;
    const isGroupDrag = isDraggingGroup();
    const dragTlId = isGroupDrag ? `group:${dragId}` : dragId;

    for (const item of items) {
      const id = ctlId(item);
      if (id === dragTlId) continue;

      const y = tops.get(id);
      const h = heights.get(id) ?? 40;
      if (y === undefined) continue;

      if (cursorY < y + h) {
        if (item.kind === "group") {
          const gId = item.id;
          const relY = cursorY - y;
          if (isGroupDrag) return cursorY >= y + h / 2 ? `tl-group-after:${gId}` : `tl-group:${gId}`;
          const draggedIsGroupMember = groupByEntryId().get(dragId) === gId;
          if (!draggedIsGroupMember && relY < h * 0.25) return `tl-group:${gId}`;
          if (!draggedIsGroupMember && relY > h * 0.75) return `tl-group-after:${gId}`;
          // Middle zone — group-drop
          const midYs = engine.cachedMidYs();
          const candidates = item.entries.filter(e => e.id !== dragTlId);
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
          const eId = item.entry.id;
          return cursorY >= y + h / 2 ? `row-after:${eId}` : eId;
        }
        const eId = item.entry.id;
        return cursorY >= y + h / 2 ? `row-after:${eId}` : eId;
      }
    }
    return null;
  };

  const handleStartDrag = (id: string, kind: "row" | "group", event: PointerEvent | MouseEvent) => {
    engine.startDrag(id, kind, event);
    const onMoveOverride = (ev: PointerEvent) => {
      if (!engine.draggingId()) return;
      const target = detectContentDropTarget(ev.clientY);
      if (target !== null) engine.setHoveredDropId(target);
    };
    const onUpCleanup = () => {
      window.removeEventListener("pointermove", onMoveOverride);
      window.removeEventListener("pointerup", onUpCleanup);
    };
    window.addEventListener("pointermove", onMoveOverride);
    window.addEventListener("pointerup", onUpCleanup);
  };

  // ── Preview items ──────────────────────────────────────────────────────
  const previewTLItems = createMemo(() => {
    const items = contentTLItems();
    const dragging = engine.draggingId();
    const drop = engine.hoveredDropId();
    if (!dragging || !drop) return items;
    if (drop.startsWith("group-drop:")) return items;
    const dragTlId = isDraggingGroup() ? `group:${dragging}` : dragging;
    const draggingItem = items.find(i => ctlId(i) === dragTlId);
    if (!draggingItem) return items;
    const rest = items.filter(i => ctlId(i) !== dragTlId);
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
      const idx = rest.findIndex(i => i.kind === "entry" && i.entry.id === rid);
      insertIdx = idx < 0 ? rest.length : idx + 1;
    } else {
      const idx = rest.findIndex(i => i.kind === "entry" && i.entry.id === drop);
      insertIdx = idx < 0 ? rest.length : idx;
    }
    const result = [...rest];
    result.splice(insertIdx, 0, draggingItem);
    return result;
  });

  const previewTranslates = createMemo(() => {
    if (!engine.draggingId() || !engine.hoveredDropId() || engine.hoveredDropId()?.startsWith("group-drop:")) return new Map<string, number>();
    return computePreviewTranslates(
      contentTLItems().map(ctlId),
      previewTLItems().map(ctlId),
      engine.cachedHeights(),
      0
    );
  });

  const previewGroupRowTranslates = createMemo(() => {
    const map = new Map<string, number>();
    const dragging = engine.draggingId();
    const drop = engine.hoveredDropId();
    if (!dragging || !drop) return map;
    for (const item of contentTLItems()) {
      if (item.kind !== "group") continue;
      const ids = item.entries.map(e => e.id);
      if (!ids.includes(dragging)) continue;
      let insertIdx: number;
      if (drop.startsWith("row-after:")) {
        const ai = ids.indexOf(drop.slice("row-after:".length));
        if (ai < 0) continue;
        insertIdx = ai + 1;
      } else if (ids.includes(drop)) {
        insertIdx = ids.indexOf(drop);
      } else continue;
      const fromIdx = ids.indexOf(dragging);
      const next = [...ids];
      next.splice(fromIdx, 1);
      next.splice(insertIdx > fromIdx ? insertIdx - 1 : insertIdx, 0, dragging);
      const translates = computePreviewTranslates(ids, next, engine.cachedHeights(), 0);
      for (const [id, offset] of translates) map.set(id, offset);
    }
    return map;
  });

  // ── Selection ──────────────────────────────────────────────────────────
  const toggleSelect = (id: string) => {
    setSelectedIds(cur => { const next = new Set(cur); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  };
  const selectedCount = () => selectedIds().size;

  // ── Group operations ───────────────────────────────────────────────────
  const createContentGroup = () => {
    const sel = [...selectedIds()];
    if (sel.length === 0) return;
    const gId = `cg-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    const newGroup: ContentGroupData = { id: gId, name: "New Group", collapsed: false, entryIds: sel };
    const updated = groups().map(g => ({ ...g, entryIds: g.entryIds.filter(id => !selectedIds().has(id)) })).filter(g => g.entryIds.length > 0);
    updated.push(newGroup);
    setGroups(updated);
    setSelectedIds(new Set());
    void saveGroups(updated);
  };

  const removeContentGroup = (gId: string) => {
    const updated = groups().filter(g => g.id !== gId);
    setGroups(updated);
    void saveGroups(updated);
  };

  const toggleContentGroupCollapsed = (gId: string) => {
    const updated = groups().map(g => g.id === gId ? { ...g, collapsed: !g.collapsed } : g);
    setGroups(updated);
    void saveGroups(updated);
  };

  const startContentGroupRename = (gId: string, name: string) => {
    setCEditingGroupId(gId);
    setCGroupNameDraft(name);
  };

  const commitContentGroupRename = (gId: string) => {
    const draft = cGroupNameDraft().trim();
    if (draft) {
      const updated = groups().map(g => g.id === gId ? { ...g, name: draft } : g);
      setGroups(updated);
      void saveGroups(updated);
    }
    setCEditingGroupId(null);
  };

  // ── Persist ────────────────────────────────────────────────────────────
  const saveGroups = async (grps: ContentGroupData[]) => {
    try { await invoke("save_content_groups_command", { input: { modlistName: props.modlistName, contentType: props.type, groups: grps } }); } catch { /* */ }
  };

  const saveOrder = async (ordered: ContentEntry[]) => {
    try { await invoke("reorder_content_command", { input: { modlistName: props.modlistName, contentType: props.type, orderedIds: ordered.map(e => e.id) } }); } catch { /* */ }
  };

  const remove = async (id: string) => {
    try {
      await invoke("remove_content_command", { input: { modlistName: props.modlistName, contentType: props.type, id } });
      setEntries(cur => cur.filter(e => e.id !== id));
      const updatedGroups = groups().map(g => ({ ...g, entryIds: g.entryIds.filter(eid => eid !== id) })).filter(g => g.entryIds.length > 0);
      setGroups(updatedGroups);
      setSelectedIds(cur => { const next = new Set(cur); next.delete(id); return next; });
    } catch { /* */ }
  };

  // ── Commit drop ────────────────────────────────────────────────────────
  const removeFromArr = (arr: string[], id: string) => arr.filter(x => x !== id);

  const commitContentGroupMove = (gId: string, dropId: string) => {
    const group = groups().find(g => g.id === gId);
    if (!group) return;
    const groupEntryIds = new Set(group.entryIds);
    const groupEntries = group.entryIds.map(id => entries().find(e => e.id === id)).filter((e): e is ContentEntry => !!e);
    const otherEntries = entries().filter(e => !groupEntryIds.has(e.id));
    let insertIdx: number;
    if (dropId.startsWith("tl-group:")) {
      const tgId = dropId.slice("tl-group:".length);
      const tg = groups().find(g => g.id === tgId);
      const first = tg?.entryIds[0];
      insertIdx = first ? otherEntries.findIndex(e => e.id === first) : otherEntries.length;
      if (insertIdx < 0) insertIdx = otherEntries.length;
    } else if (dropId.startsWith("tl-group-after:")) {
      const tgId = dropId.slice("tl-group-after:".length);
      const tg = groups().find(g => g.id === tgId);
      let last = -1;
      if (tg) for (let i = otherEntries.length - 1; i >= 0; i--) { if (tg.entryIds.includes(otherEntries[i].id)) { last = i; break; } }
      insertIdx = last < 0 ? otherEntries.length : last + 1;
    } else if (dropId.startsWith("row-after:")) {
      const rid = dropId.slice("row-after:".length);
      const idx = otherEntries.findIndex(e => e.id === rid);
      insertIdx = idx < 0 ? otherEntries.length : idx + 1;
    } else {
      const idx = otherEntries.findIndex(e => e.id === dropId);
      insertIdx = idx < 0 ? otherEntries.length : idx;
    }
    const newEntries = [...otherEntries];
    newEntries.splice(insertIdx, 0, ...groupEntries);
    setEntries(newEntries);
    void saveOrder(newEntries);
  };

  const commitContentRowMove = (fromId: string, dropId: string) => {
    const grps = groups().map(g => ({ ...g, entryIds: [...g.entryIds] }));
    const membership = groupByEntryId();
    const srcGroupId = membership.get(fromId) ?? null;
    if (srcGroupId) {
      const sg = grps.find(g => g.id === srcGroupId);
      if (sg) sg.entryIds = removeFromArr(sg.entryIds, fromId);
    }
    if (dropId.startsWith("group-drop:")) {
      const tgId = dropId.slice("group-drop:".length);
      const tg = grps.find(g => g.id === tgId);
      if (tg && !tg.entryIds.includes(fromId)) tg.entryIds.push(fromId);
      const entry = entries().find(e => e.id === fromId)!;
      const others = entries().filter(e => e.id !== fromId);
      let lastIdx = -1;
      if (tg) for (let i = others.length - 1; i >= 0; i--) { if (tg.entryIds.includes(others[i].id)) { lastIdx = i; break; } }
      const insertIdx = lastIdx < 0 ? others.length : lastIdx + 1;
      const newEntries = [...others];
      newEntries.splice(insertIdx, 0, entry);
      batch(() => { setGroups(grps.filter(g => g.entryIds.length > 0)); setEntries(newEntries); });
      void saveOrder(newEntries);
      void saveGroups(grps.filter(g => g.entryIds.length > 0));
      return;
    }
    // If dropping on/after a row in a group, join that group
    {
      let targetRowId: string | null = null;
      if (dropId.startsWith("row-after:")) targetRowId = dropId.slice("row-after:".length);
      else if (!dropId.startsWith("tl-group:") && !dropId.startsWith("tl-group-after:")) targetRowId = dropId;
      if (targetRowId) {
        const tgtGroupId = membership.get(targetRowId);
        if (tgtGroupId) {
          const tg = grps.find(g => g.id === tgtGroupId);
          if (tg && !tg.entryIds.includes(fromId)) {
            const tidx = tg.entryIds.indexOf(targetRowId);
            if (dropId.startsWith("row-after:")) tg.entryIds.splice(tidx < 0 ? tg.entryIds.length : tidx + 1, 0, fromId);
            else tg.entryIds.splice(tidx < 0 ? 0 : tidx, 0, fromId);
          }
        }
      }
    }
    const entry = entries().find(e => e.id === fromId)!;
    const others = entries().filter(e => e.id !== fromId);
    const filteredGroups = grps.filter(g => g.entryIds.length > 0);
    let insertIdx: number;
    if (dropId.startsWith("tl-group:")) {
      const tgId = dropId.slice("tl-group:".length);
      const tg = filteredGroups.find(g => g.id === tgId);
      const first = tg?.entryIds[0];
      insertIdx = first ? others.findIndex(e => e.id === first) : others.length;
      if (insertIdx < 0) insertIdx = others.length;
    } else if (dropId.startsWith("tl-group-after:")) {
      const tgId = dropId.slice("tl-group-after:".length);
      const tg = filteredGroups.find(g => g.id === tgId);
      let last = -1;
      if (tg) for (let i = others.length - 1; i >= 0; i--) { if (tg.entryIds.includes(others[i].id)) { last = i; break; } }
      insertIdx = last < 0 ? others.length : last + 1;
    } else if (dropId.startsWith("row-after:")) {
      const rid = dropId.slice("row-after:".length);
      const idx = others.findIndex(e => e.id === rid);
      insertIdx = idx < 0 ? others.length : idx + 1;
    } else {
      const idx = others.findIndex(e => e.id === dropId);
      insertIdx = idx < 0 ? others.length : idx;
    }
    const newEntries = [...others];
    newEntries.splice(insertIdx, 0, entry);
    batch(() => { setGroups(filteredGroups); setEntries(newEntries); });
    void saveOrder(newEntries);
    void saveGroups(filteredGroups);
  };

  const commitContentDrop = (fromId: string, dropId: string, fromKind: "row" | "group") => {
    if (fromId === dropId) return;
    if (fromKind === "group") commitContentGroupMove(fromId, dropId);
    else commitContentRowMove(fromId, dropId);
  };

  // ── Dragging info helpers ──────────────────────────────────────────────
  const draggingEntry = () => {
    const id = engine.draggingId();
    if (!id || isDraggingGroup()) return null;
    return entries().find(e => e.id === id) ?? null;
  };
  const draggingEntryIcon = () => {
    const e = draggingEntry();
    return e ? meta().get(e.id)?.iconUrl : undefined;
  };
  const draggingGroupItem = () => {
    const id = engine.draggingId();
    if (!id || !isDraggingGroup()) return null;
    return contentTLItems().find(i => i.kind === "group" && i.id === id) as (ContentTopLevelItem & { kind: "group" }) | undefined ?? null;
  };

  // ── Entry row renderer (matches ModRuleItem styling) ──────────────────
  // ── Version-based resolution check ─────────────────────────────────
  const isEntryResolved = (entry: ContentEntry): boolean | null => {
    const rules = entry.versionRules;
    if (rules.length === 0) return true; // no rules = always active
    const mcVer = selectedMcVersion();
    const loader = selectedModLoader();
    for (const rule of rules) {
      const versionMatch = rule.mcVersions.length === 0 || rule.mcVersions.includes(mcVer);
      const loaderMatch = rule.loader === "any" || rule.loader === loader;
      if (rule.kind === "exclude" && versionMatch && loaderMatch) return false;
      if (rule.kind === "only" && !(versionMatch && loaderMatch)) return false;
    }
    return true;
  };

  // ── Entry row (click-vs-drag threshold, same as ModRuleItem) ──────────
  const DRAG_THRESHOLD = 5;

  const EntryRow = (rowProps: { entry: ContentEntry; onStartDrag: (id: string, e: PointerEvent) => void }) => {
    const info = () => meta().get(rowProps.entry.id);
    const isSelected = () => selectedIds().has(rowProps.entry.id);
    const isLocal = () => rowProps.entry.source === "local";
    const resolved = () => isEntryResolved(rowProps.entry);

    const [pendingClickPos, setPendingClickPos] = createSignal<{ x: number; y: number } | null>(null);

    const handlePointerDown = (event: PointerEvent) => {
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
          rowProps.onStartDrag(rowProps.entry.id, event);
        }
      };
      const onUp = () => {
        if (pendingClickPos()) {
          toggleSelect(rowProps.entry.id);
          setPendingClickPos(null);
        }
      };
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      onCleanup(() => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
      });
    });

    const stopDragPropagation = (event: MouseEvent | PointerEvent) => event.stopPropagation();

    return (
      <div
        class={`group flex items-center gap-3 rounded-md px-2 py-2 transition-colors select-none cursor-grab active:cursor-grabbing ${
          isSelected() ? "bg-primary/10 ring-1 ring-primary/20" : "hover:bg-muted/50"
        }`}
        onPointerDown={handlePointerDown}
      >
        {/* Icon */}
        <div class="flex h-8 w-8 shrink-0 items-center justify-center overflow-hidden rounded-md bg-muted">
          <Show when={info()?.iconUrl} fallback={<PackageIcon class="h-4 w-4 text-muted-foreground" />}>
            <img src={info()!.iconUrl} alt="" class="h-8 w-8 object-cover" loading="lazy" />
          </Show>
        </div>
        {/* Info */}
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-1.5">
            <span class={`truncate font-medium ${
              resolved() === true ? "text-green-400" : resolved() === false ? "text-red-400" : "text-foreground"
            }`}>{info()?.name ?? rowProps.entry.id}</span>
            <span class={`inline-flex items-center rounded-md border px-1.5 py-0.5 text-[10px] font-medium ${
              isLocal() ? "border-warning/40 bg-warning/10 text-warning" : "border-border bg-secondary text-secondary-foreground"
            }`}>
              {isLocal() ? "Local" : "Modrinth"}
            </span>
          </div>
        </div>
        {/* Actions */}
        <div class="flex shrink-0 items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          <button
            onClick={(e) => { e.stopPropagation(); setAdvancedEntryId(rowProps.entry.id); }}
            onPointerDown={stopDragPropagation}
            onMouseDown={stopDragPropagation}
            class="flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            title="Advanced settings"
          >
            <MaterialIcon name="settings" size="sm" />
            Advanced
          </button>
        </div>
      </div>
    );
  };

  // ── Render ─────────────────────────────────────────────────────────────
  const advancedEntry = () => { const id = advancedEntryId(); return id ? entries().find(e => e.id === id) ?? null : null; };

  return (
    <div class="flex flex-1 flex-col overflow-hidden">
      {/* Advanced panel */}
      <Show when={advancedEntry()}>
        <ContentAdvancedPanel
          entry={advancedEntry()!}
          name={meta().get(advancedEntry()!.id)?.name ?? advancedEntry()!.id}
          modlistName={props.modlistName}
          contentType={props.type}
          onClose={() => setAdvancedEntryId(null)}
          onUpdate={(entryId, rules) => {
            setEntries(cur => cur.map(e => e.id === entryId ? { ...e, versionRules: rules } : e));
          }}
        />
      </Show>

      {/* Action bar */}
      <Show when={selectedCount() > 0} fallback={
        <div class="px-6 py-2 bg-bgPanel border-b border-borderColor shrink-0 flex items-center gap-3">
          <button
            onClick={props.onAddContent}
            class="px-4 py-1.5 rounded-lg bg-primary hover:bg-brandPurpleHover text-white text-sm font-medium flex items-center gap-2 transition-colors duration-75"
          >
            <MaterialIcon name="add" size="md" />
            Add {label()}
          </button>
        </div>
      }>
        <header class="h-14 bg-primary/20 border-b border-primary flex items-center px-6 justify-between shrink-0">
          <div class="flex items-center gap-4">
            <span class="text-sm font-medium text-primary border-r border-primary/30 pr-4">
              {selectedCount()} selected
            </span>
            <div class="flex items-center gap-2">
              <button
                onClick={createContentGroup}
                class="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-primary hover:text-white hover:bg-primary/40 rounded-lg border border-primary/30 transition-colors duration-75"
              >
                <MaterialIcon name="folder_open" size="md" />
                Create Group
              </button>
              <button
                onClick={() => { for (const id of selectedIds()) void remove(id); setSelectedIds(new Set()); }}
                class="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-primary hover:text-white hover:bg-primary/40 rounded-lg border border-primary/30 transition-colors duration-75"
              >
                <MaterialIcon name="delete" size="md" />
                Delete
              </button>
            </div>
          </div>
          <button onClick={() => setSelectedIds(new Set())} class="text-primary hover:text-white p-1 transition-colors duration-75">
            <MaterialIcon name="close" size="lg" />
          </button>
        </header>
      </Show>

      {/* Content list */}
      <div class="flex-1 p-4" style={{ overflow: engine.anyDragging() ? "hidden" : "auto" }}>
        <Show when={entries().length > 0} fallback={
          <div class="flex flex-col items-center justify-center py-16 text-center">
            <div class="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-muted">
              <PackageIcon class="h-8 w-8 text-muted-foreground" />
            </div>
            <h3 class="mb-2 text-lg font-semibold text-foreground">No {label()}</h3>
            <p class="mb-6 max-w-xs text-sm text-muted-foreground">
              Add {label().toLowerCase()} from Modrinth or upload local files.
            </p>
            <button onClick={props.onAddContent} class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90">
              Add Your First {label().replace(/s$/, "")}
            </button>
          </div>
        }>
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
                        <Show when={draggingEntryIcon()} fallback={<PackageIcon class="h-4 w-4 text-muted-foreground" />}>
                          <img src={draggingEntryIcon()!} alt="" class="h-8 w-8 object-cover" />
                        </Show>
                      </div>
                      <div class="min-w-0 flex-1">
                        <span class="truncate font-medium text-foreground">{meta().get(draggingEntry()?.id ?? "")?.name ?? draggingEntry()?.id}</span>
                      </div>
                    </div>
                  }
                >
                  <div class="flex items-center gap-3 px-3 py-2">
                    <FolderOpenIcon class="h-5 w-5 shrink-0 text-primary" />
                    <div class="min-w-0 flex-1">
                      <div class="truncate font-medium text-foreground">{draggingGroupItem()!.name}</div>
                      <div class="text-xs text-muted-foreground">{draggingGroupItem()!.entries.length} items</div>
                    </div>
                  </div>
                </Show>
              </div>
            </Show>

            {/* List */}
            <div class="space-y-0.5" ref={listContainerRef}>
              <For each={contentTLItems()}>
                {(item) => {
                  const id = ctlId(item);
                  const isDragging = () => {
                    const dId = engine.draggingId();
                    if (!dId) return false;
                    return isDraggingGroup() ? `group:${dId}` === id : dId === id;
                  };
                  const offset = () => isDragging() ? 0 : (previewTranslates().get(id) ?? 0);

                  if (item.kind === "entry") {
                    return (
                      <div
                        data-draggable-id={id}
                        data-draggable-mid-id={id}
                        style={{
                          transform: engine.anyDragging() ? `translateY(${offset()}px)` : "none",
                          transition: engine.anyDragging() ? "transform 150ms ease" : "none",
                          position: "relative",
                          "z-index": isDragging() ? "0" : "1",
                        }}
                        class={isDragging() ? "opacity-0 pointer-events-none" : ""}
                      >
                        <EntryRow
                          entry={item.entry}
                          onStartDrag={(eId, e) => handleStartDrag(eId, "row", e)}
                        />
                      </div>
                    );
                  }

                  // Group
                  const group = item;
                  const isGroupDrop = () => engine.hoveredDropId() === `group-drop:${group.id}`;
                  const isBeforeTarget = () => engine.hoveredDropId() === `tl-group:${group.id}`;
                  const isAfterTarget = () => engine.hoveredDropId() === `tl-group-after:${group.id}`;

                  return (
                    <div
                      data-draggable-id={id}
                      style={{
                        transform: engine.anyDragging() ? `translateY(${offset()}px)` : "none",
                        transition: engine.anyDragging() ? "transform 150ms ease" : "none",
                        position: "relative",
                        "z-index": isDragging() ? "0" : "1",
                      }}
                      class={`mb-2 ${isDragging() ? "opacity-0 pointer-events-none" : ""}`}
                    >
                      <Show when={isBeforeTarget()}>
                        <div class="mb-1 h-0.5 rounded bg-primary" />
                      </Show>

                      <div class={`rounded-xl border p-2 shadow-sm transition-colors ${
                        isGroupDrop() ? "border-primary/40 bg-primary/5" : "border-border/70 bg-muted/10"
                      }`}>
                        <div class="flex items-center gap-2 px-1 py-1">
                          <button
                            onClick={() => toggleContentGroupCollapsed(group.id)}
                            class="flex items-center text-sm font-medium text-muted-foreground transition-colors hover:text-foreground"
                          >
                            <Show when={group.collapsed} fallback={<ChevronDownIcon class="h-4 w-4" />}>
                              <ChevronRightIcon class="h-4 w-4" />
                            </Show>
                          </button>
                          <div class="cursor-grab touch-none" onPointerDown={(e) => handleStartDrag(group.id, "group", e)}>
                            <FolderOpenIcon class="h-4 w-4 text-primary" />
                          </div>
                          <Show
                            when={cEditingGroupId() === group.id}
                            fallback={
                              <span class="flex-1 cursor-pointer text-sm font-medium text-foreground" onClick={() => startContentGroupRename(group.id, group.name)}>
                                {group.name}
                              </span>
                            }
                          >
                            <input
                              type="text"
                              value={cGroupNameDraft()}
                              onInput={e => setCGroupNameDraft(e.currentTarget.value)}
                              onBlur={() => commitContentGroupRename(group.id)}
                              onKeyDown={e => { if (e.key === "Enter" || e.key === "Escape") commitContentGroupRename(group.id); }}
                              class="flex-1 rounded bg-transparent text-sm font-medium text-foreground outline-none"
                              autofocus
                            />
                          </Show>
                          <span class="shrink-0 text-xs text-muted-foreground">{group.entries.length} items</span>
                          <button
                            onClick={() => removeContentGroup(group.id)}
                            class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                          >
                            <XIcon class="h-3.5 w-3.5" />
                          </button>
                        </div>
                        <Show when={!group.collapsed}>
                          <div class="mt-1 space-y-0.5">
                            <For each={group.entries}>
                              {(entry) => {
                                const isDraggingRow = () => engine.draggingId() === entry.id && engine.draggingKind() === "row";
                                const rowOffset = () => isDraggingRow() ? 0 : (previewGroupRowTranslates().get(entry.id) ?? 0);
                                const isTarget = () =>
                                  !isDraggingRow() && (engine.hoveredDropId() === entry.id || engine.hoveredDropId() === `row-after:${entry.id}`);
                                return (
                                  <div
                                    data-draggable-id={entry.id}
                                    data-draggable-mid-id={entry.id}
                                    style={{
                                      transform: engine.anyDragging() ? `translateY(${rowOffset()}px)` : "none",
                                      transition: engine.anyDragging() ? "transform 150ms ease" : "none",
                                      position: "relative",
                                      "z-index": isDraggingRow() ? "0" : "1",
                                    }}
                                    class={isDraggingRow() ? "opacity-0 pointer-events-none" : isTarget() ? "rounded-md ring-1 ring-primary/40" : ""}
                                  >
                                    <EntryRow entry={entry} onStartDrag={(eId, e) => handleStartDrag(eId, "row", e)} />
                                  </div>
                                );
                              }}
                            </For>
                            <Show when={group.entries.length === 0}>
                              <div class="rounded-md border border-dashed border-border bg-background/40 px-4 py-4 text-center text-sm text-muted-foreground">
                                Drag items here to place them in this group.
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
          </>
        </Show>
      </div>
    </div>
  );
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
    const items   = topLevelItems();
    const heights = engine.cachedHeights();
    const tops    = engine.cachedTops();
    const dragId  = engine.draggingId()!;
    const isGroupDrag = isDraggingGroup();
    const dragTlId = isGroupDrag ? `group:${dragId}` : dragId;

    for (const item of items) {
      const id = tlId(item);
      if (id === dragTlId) continue;

      const y = tops.get(id);
      const h = heights.get(id) ?? 40;
      if (y === undefined) continue;

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

  // ── Tab definitions ─────────────────────────────────────────────────────────
  const TABS: Array<{ id: ContentTabId; label: string; icon: string }> = [
    { id: "mods",         label: "Mods",           icon: "extension" },
    { id: "resourcepack", label: "Resource Packs",  icon: "palette" },
    { id: "datapack",     label: "Data Packs",      icon: "database" },
    { id: "shader",       label: "Shaders",         icon: "auto_awesome" },
  ];

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
                  void (async () => {
                    try {
                      const { open } = await import("@tauri-apps/plugin-dialog");
                      const selected = await open({ title: "Import Mod List (rules.json)", filters: [{ name: "JSON", extensions: ["json"] }], multiple: false });
                      if (!selected) return;
                      const { invoke } = await import("@tauri-apps/api/core");
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

          {/* Tab bar */}
          <div class="mt-3 flex gap-1 -mb-3">
            <For each={TABS}>
              {(tab) => {
                const isActive = () => activeContentTab() === tab.id;
                return (
                  <button
                    onClick={() => setActiveContentTab(tab.id)}
                    class={`flex items-center gap-1.5 px-3 py-2 text-sm font-medium rounded-t-md transition-colors border-b-2 ${
                      isActive()
                        ? "border-primary text-primary bg-primary/5"
                        : "border-transparent text-muted-foreground hover:text-foreground hover:bg-muted/30"
                    }`}
                  >
                    <MaterialIcon name={tab.icon} size="sm" />
                    {tab.label}
                  </button>
                );
              }}
            </For>
          </div>
        </div>

        {/* Tab content */}
        <Show when={activeContentTab() === "mods"} fallback={
          <ContentTabView
            type={activeContentTab() as string}
            modlistName={selectedModListName()}
            onAddContent={() => setAddModModalOpen(true)}
          />
        }>
          <ActionBar onAddMod={props.onAddMod} onDeleteSelected={props.onDeleteSelected} />

          {/* Scrollable mod list */}
          <div class="flex-1 p-4" style={{ overflow: engine.anyDragging() ? "hidden" : "auto" }}>
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
                                enabled={group.blocks.every(r => r.enabled)}
                                onToggleEnabled={() => {
                                  const handler = onToggleEnabled();
                                  if (!handler) return;
                                  const allEnabled = group.blocks.every(r => r.enabled);
                                  for (const r of group.blocks) handler(r.id, !allEnabled);
                                }}
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
      </Show>
    </div>
  );
}
