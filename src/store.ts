import { createSignal, createMemo } from "solid-js";
import type {
  ModRow, ModListCard, ModrinthResult, AestheticGroup, FunctionalGroup,
  IncompatibilityRule, LinkRule, DownloadProgressItem, LauncherUiError, AccountSummary,
  VersionRule, CustomConfig,
} from "./lib/types";
import { LAUNCH_STAGES } from "./lib/types";
export { LAUNCH_STAGES } from "./lib/types";

// ── Demo data used ONLY in the browser (not in the real Tauri app) ────────────

export const DEMO_MOD_LISTS: ModListCard[] = [
  { name: "My First Pack",  status: "Ready",     accent: "from-primary/30 via-primary/10 to-transparent",   description: "Create a mod list by clicking + above." },
];

export const DEMO_ACCOUNTS: AccountSummary[] = [
  { id: "demo", gamertag: "Not logged in", email: "", status: "offline", lastMode: "offline" },
];

export const MOCK_MODRINTH: ModrinthResult[] = [
  { id: "sodium",         name: "Sodium",           author: "jellysquid3", description: "Modern rendering optimization mod with strong Fabric support.",            categories: ["Optimization", "Rendering"] },
  { id: "iris",           name: "Iris Shaders",     author: "coderbot",    description: "Shader support compatible with Sodium.",                                   categories: ["Shaders", "Rendering"] },
  { id: "xaeros-minimap", name: "Xaero's Minimap",  author: "xaero96",     description: "Compact minimap for utility and navigation.",                              categories: ["Utility", "Map"] },
  { id: "fabric-api",     name: "Fabric API",       author: "FabricMC",    description: "Core modding API — almost always required by Fabric mods.",                categories: ["Library", "Core"] },
  { id: "lithium",        name: "Lithium",           author: "jellysquid3", description: "General-purpose optimization mod.",                                        categories: ["Optimization"] },
  { id: "create",         name: "Create",            author: "simibubi",    description: "Automation and aesthetics with rotating contraptions.",                    categories: ["Content", "Tech"] },
  { id: "journeymap",     name: "JourneyMap",        author: "techbrew",    description: "Real-time in-game mapping.",                                               categories: ["Utility", "Map"] },
  { id: "modmenu",        name: "Mod Menu",          author: "Prospector",  description: "Adds a mod menu to view installed mods.",                                  categories: ["Utility"] },
];

// ── All signals start EMPTY — they are populated by the Tauri backend ─────────

export const [modListCards, setModListCards]                     = createSignal<ModListCard[]>([]);
export const [selectedModListName, setSelectedModListName]       = createSignal<string>("");
export const [search, setSearch]                                 = createSignal("");
export const [minecraftVersions, setMinecraftVersions]           = createSignal<string[]>(["1.21.1", "1.20.6", "1.20.4", "1.19.4"]);
export const [selectedMcVersion, setSelectedMcVersion]           = createSignal("1.21.1");
export const [selectedModLoader, setSelectedModLoader]           = createSignal<string>("Fabric");
export const [launchState, setLaunchState]                       = createSignal<"idle" | "resolving" | "ready" | "running">("idle");
export const [launchProgress, setLaunchProgress]                 = createSignal(0);
export const [launchStageLabel, setLaunchStageLabel]             = createSignal(LAUNCH_STAGES[0].label);
export const [launchStageDetail, setLaunchStageDetail]           = createSignal(LAUNCH_STAGES[0].detail);
export const [launchLogs, setLaunchLogs]                         = createSignal<string[]>([]);
export const [logViewerOpen, setLogViewerOpen]                   = createSignal(false);
export const [downloadItems, setDownloadItems]                   = createSignal<DownloadProgressItem[]>([]);
export const [launcherErrors, setLauncherErrors]                 = createSignal<LauncherUiError[]>([]);
export const [errorCenterOpen, setErrorCenterOpen]               = createSignal(false);
export const [accountsModalOpen, setAccountsModalOpen]           = createSignal(false);
export const [accounts, setAccounts]                             = createSignal<AccountSummary[]>([]);
export const [activeAccountId, setActiveAccountId]               = createSignal<string>("");
export const [instancePresentationOpen, setInstancePresentationOpen] = createSignal(false);
export const [verificationModalOpen, setVerificationModalOpen]   = createSignal(false);
export const [instancePresentation, setInstancePresentation]     = createSignal({ iconLabel: "ML", iconAccent: "", notes: "", iconImage: "" });
export const [exportModalOpen, setExportModalOpen]               = createSignal(false);
export const [exportOptions, setExportOptions]                   = createSignal({ rulesJson: true, modJars: false, configFiles: false, resourcePacks: false, otherFiles: false });
export const [settingsModalOpen, setSettingsModalOpen]           = createSignal(false);
export const [settingsTab, setSettingsTab]                       = createSignal<"global" | "modlist">("global");
export const [globalSettings, setGlobalSettings]                 = createSignal({ minRamMb: 2048, maxRamMb: 4096, customJvmArgs: "-XX:+UseG1GC -XX:+ParallelRefProcEnabled", profilerEnabled: false, wrapperCommand: "", javaPathOverride: "" });
export const [modlistOverrides, setModlistOverrides]             = createSignal({ minRamEnabled: false, minRamMb: 2048, maxRamEnabled: false, maxRamMb: 4096, customArgsEnabled: false, customJvmArgs: "", profilerEnabled: false, profilerActive: false, wrapperEnabled: false, wrapperCommand: "" });
export const [addModModalOpen, setAddModModalOpen]               = createSignal(false);
export const [addModSearch, setAddModSearch]                     = createSignal("");
export const [addModMode, setAddModMode]                         = createSignal<"modrinth" | "local">("modrinth");
export const [selectedIds, setSelectedIds]                       = createSignal<string[]>([]);
export const [expandedRows, setExpandedRows]                     = createSignal<string[]>([]);
export const [modRowsState, setModRowsState]                     = createSignal<ModRow[]>([]);
export const [aestheticGroups, setAestheticGroups]               = createSignal<AestheticGroup[]>([]);
export const [functionalGroups, setFunctionalGroups]             = createSignal<FunctionalGroup[]>([]);
export type SortOrder = "default" | "name-az" | "name-za";
export const [tagFilter, setTagFilter]                           = createSignal<Set<string>>(new Set());
export const [sortOrder, setSortOrder]                           = createSignal<SortOrder>("default");
export const [editingGroupId, setEditingGroupId]                 = createSignal<string | null>(null);
export const [groupNameDraft, setGroupNameDraft]                 = createSignal("");
export const [functionalGroupModalOpen, setFunctionalGroupModalOpen] = createSignal(false);
export const [newFunctionalGroupName, setNewFunctionalGroupName] = createSignal("");
export const [functionalGroupTone, setFunctionalGroupTone]       = createSignal<string>("violet");
export const [alternativesPanelParentId, setAlternativesPanelParentId] = createSignal<string | null>(null);
export const [savedIncompatibilities, setSavedIncompatibilities] = createSignal<IncompatibilityRule[]>([]);
export const [draftIncompatibilities, setDraftIncompatibilities] = createSignal<IncompatibilityRule[]>([]);
export const [incompatibilityModalOpen, setIncompatibilityModalOpen] = createSignal(false);
export const [incompatibilityFocusId, setIncompatibilityFocusId] = createSignal<string | null>(null);
export const [renameRuleModalOpen, setRenameRuleModalOpen]       = createSignal(false);
export const [renameRuleTargetId, setRenameRuleTargetId]         = createSignal<string | null>(null);
export const [renameRuleDraft, setRenameRuleDraft]               = createSignal("");
export const [createModlistModalOpen, setCreateModlistModalOpen] = createSignal(false);
export const [createModlistName, setCreateModlistName]           = createSignal("");
export const [createModlistDescription, setCreateModlistDescription] = createSignal("");
export const [createModlistBusy, setCreateModlistBusy]           = createSignal(false);
export const [localJarRuleName, setLocalJarRuleName]             = createSignal("");
export const [appLoading, setAppLoading]                         = createSignal(true);
/** Maps Modrinth project slug → icon URL, populated lazily after editor load. */
export const [modIcons, setModIcons]                             = createSignal<Map<string, string>>(new Map());
export const [savedLinks, setSavedLinks]                         = createSignal<LinkRule[]>([]);
export const [draftLinks, setDraftLinks]                         = createSignal<LinkRule[]>([]);
export const [linkModalOpen, setLinkModalOpen]                   = createSignal(false);
export const [linkModalModIds, setLinkModalModIds]               = createSignal<string[]>([]);
export const [linksOverviewOpen, setLinksOverviewOpen]           = createSignal(false);
/** Set of mod IDs that the resolver considers active for the current version + loader. */
export const [resolvedModIds, setResolvedModIds]                 = createSignal<Set<string>>(new Set());
export const [versionRules, setVersionRules]                     = createSignal<VersionRule[]>([]);
export const [customConfigs, setCustomConfigs]                   = createSignal<CustomConfig[]>([]);
export const [advancedPanelModId, setAdvancedPanelModId]         = createSignal<string | null>(null);

// ── Computed / Memos ──────────────────────────────────────────────────────────

function collectRowsRecursive(rows: ModRow[], map: Map<string, ModRow>) {
  for (const row of rows) {
    map.set(row.id, row);
    if (row.alternatives?.length) collectRowsRecursive(row.alternatives, map);
  }
}

export const rowMap = createMemo(() => {
  const map = new Map<string, ModRow>();
  collectRowsRecursive(modRowsState(), map);
  return map;
});

export const selectedModList = createMemo(() =>
  modListCards().find(m => m.name === selectedModListName()) ?? modListCards()[0] ?? null
);

export const activeAccount = createMemo(() =>
  accounts().find(a => a.id === activeAccountId()) ?? accounts()[0] ?? null
);

export const selectedCount = createMemo(() => selectedIds().length);

export function rowOrDescendantMatchesTag(row: ModRow, fGroups: FunctionalGroup[], tf: Set<string>): boolean {
  if (fGroups.some(g => tf.has(g.id) && g.modIds.includes(row.id))) return true;
  return (row.alternatives ?? []).some(alt => rowOrDescendantMatchesTag(alt, fGroups, tf));
}

export type TopLevelItem =
  | { kind: 'group'; id: string; name: string; collapsed: boolean; blocks: ModRow[] }
  | { kind: 'row'; row: ModRow };

export const topLevelItems = createMemo<TopLevelItem[]>(() => {
  const topGroups = aestheticGroups().filter(g => !g.scopeRowId);
  const groupByRow = new Map<string, string>();
  for (const g of topGroups) for (const id of g.blockIds) groupByRow.set(id, g.id);

  const q       = search().trim().toLowerCase();
  const tf      = tagFilter();
  const so      = sortOrder();
  const fGroups = functionalGroups();

  const addedGroups = new Set<string>();
  const items: TopLevelItem[]    = [];
  const ungroupedSlots: number[] = [];
  const ungroupedRows: ModRow[]  = [];

  for (const row of modRowsState()) {
    const gId = groupByRow.get(row.id);
    if (gId) {
      if (!addedGroups.has(gId)) {
        addedGroups.add(gId);
        const group = topGroups.find(g => g.id === gId)!;
        let blocks = group.blockIds
          .map(id => modRowsState().find(r => r.id === id))
          .filter((r): r is ModRow => Boolean(r))
          .filter(r => !q || rowMatchesQuery(r, q))
          .filter(r => tf.size === 0 || rowOrDescendantMatchesTag(r, fGroups, tf));
        if (so === "name-az") blocks = [...blocks].sort((a, b) => a.name.localeCompare(b.name));
        if (so === "name-za") blocks = [...blocks].sort((a, b) => b.name.localeCompare(a.name));
        if (blocks.length > 0 || (!q && tf.size === 0)) {
          items.push({ kind: 'group', id: gId, name: group.name, collapsed: group.collapsed, blocks });
        }
      }
    } else {
      if (q && !rowMatchesQuery(row, q)) continue;
      if (tf.size > 0 && !rowOrDescendantMatchesTag(row, fGroups, tf)) continue;
      ungroupedSlots.push(items.length);
      ungroupedRows.push(row);
      items.push({ kind: 'row', row });
    }
  }

  if (so !== "default") {
    const sorted = [...ungroupedRows].sort((a, b) =>
      so === "name-az" ? a.name.localeCompare(b.name) : b.name.localeCompare(a.name)
    );
    let si = 0;
    return items.map(item => item.kind === 'row' ? ({ kind: 'row', row: sorted[si++] } as TopLevelItem) : item);
  }

  return items;
});

export const filteredRows = createMemo(() => {
  const q       = search().trim().toLowerCase();
  const tf      = tagFilter();
  const so      = sortOrder();
  const fGroups = functionalGroups();
  let rows = modRowsState();
  if (q) rows = rows.filter(r => rowMatchesQuery(r, q));
  if (tf.size > 0) rows = rows.filter(r => rowOrDescendantMatchesTag(r, fGroups, tf));
  if (so === "name-az") rows = [...rows].sort((a, b) => a.name.localeCompare(b.name));
  if (so === "name-za") rows = [...rows].sort((a, b) => b.name.localeCompare(a.name));
  return rows;
});

export const manualModRows = createMemo(() => {
  const alts = modRowsState().flatMap(r => (r.alternatives ?? []).filter(a => a.kind === "local"));
  return [...modRowsState().filter(r => r.kind === "local"), ...alts];
});

export const manualModWarningCount = createMemo(() => manualModRows().length);

export const completedDownloads = createMemo(() => downloadItems().filter(i => i.status === "complete"));

export const latestError = createMemo(() => launcherErrors()[0] ?? null);

export const activeLaunchStage = createMemo(() => ({ label: launchStageLabel(), detail: launchStageDetail(), progress: launchProgress() }));

export const playButtonLabel = createMemo(() => {
  if (launchState() === "resolving") return `Resolving ${launchProgress()}%`;
  if (launchState() === "ready")     return "Launch Ready";
  return "Play";
});

export const functionalGroupsByBlockId = createMemo(() => {
  const map = new Map<string, FunctionalGroup[]>();
  for (const g of functionalGroups()) {
    for (const id of g.modIds) {
      const cur = map.get(id) ?? [];
      cur.push(g);
      map.set(id, cur);
    }
  }
  return map;
});

export const advancedPanelMod = createMemo(() => {
  const id = advancedPanelModId();
  return id ? (rowMap().get(id) ?? null) : null;
});

export const parentIdByChildId = createMemo(() => {
  const map = new Map<string, string>();
  function walk(rows: ModRow[], pid: string | null) {
    for (const row of rows) {
      if (pid !== null) map.set(row.id, pid);
      walk(row.alternatives ?? [], row.id);
    }
  }
  walk(modRowsState(), null);
  return map;
});

export const conflictModIds = createMemo(() => {
  const ids = new Set<string>();
  for (const r of savedIncompatibilities()) { ids.add(r.winnerId); ids.add(r.loserId); }
  return ids;
});

/** Returns all conflict pairs that involve a given row id. */
export const conflictPairsForId = createMemo(() => {
  const byId = new Map<string, Array<{ winnerId: string; loserId: string }>>();
  for (const rule of savedIncompatibilities()) {
    for (const id of [rule.winnerId, rule.loserId]) {
      const list = byId.get(id) ?? [];
      list.push(rule);
      byId.set(id, list);
    }
  }
  return byId;
});

export const linksByModId = createMemo(() => {
  const map = new Map<string, Array<{ partnerId: string; direction: 'mutual' | 'requires' | 'required-by' }>>();
  const links = savedLinks();
  const fromTo = new Set(links.map(l => `${l.fromId}|${l.toId}`));
  for (const link of links) {
    const reverse = fromTo.has(`${link.toId}|${link.fromId}`);
    const fromList = map.get(link.fromId) ?? [];
    if (!fromList.some(e => e.partnerId === link.toId)) {
      fromList.push({ partnerId: link.toId, direction: reverse ? 'mutual' : 'requires' });
      map.set(link.fromId, fromList);
    }
    if (!reverse) {
      const toList = map.get(link.toId) ?? [];
      if (!toList.some(e => e.partnerId === link.fromId)) {
        toList.push({ partnerId: link.fromId, direction: 'required-by' });
        map.set(link.toId, toList);
      }
    }
  }
  return map;
});

export const groupNameByBlockId = createMemo(() => {
  const map = new Map<string, string>();
  for (const g of aestheticGroups()) for (const id of g.blockIds) map.set(id, g.name);
  return map;
});

function findDirectParentId(rows: ModRow[], targetId: string): string | null {
  for (const row of rows) {
    if ((row.alternatives ?? []).some(alt => alt.id === targetId)) return row.id;
    const nested = findDirectParentId(row.alternatives ?? [], targetId);
    if (nested) return nested;
  }
  return null;
}

export const filteredGroupSections = createMemo(() => {
  const q       = search().trim().toLowerCase();
  const tf      = tagFilter();
  const so      = sortOrder();
  const fGroups = functionalGroups();
  return aestheticGroups().filter(g => !g.scopeRowId).map(g => {
    let blocks = g.blockIds
      .map(id => rowMap().get(id))
      .filter((r): r is ModRow => Boolean(r))
      .filter(r => !q || rowMatchesQuery(r, q))
      .filter(r => tf.size === 0 || rowOrDescendantMatchesTag(r, fGroups, tf));
    if (so === "name-az") blocks = [...blocks].sort((a, b) => a.name.localeCompare(b.name));
    if (so === "name-za") blocks = [...blocks].sort((a, b) => b.name.localeCompare(a.name));
    return { ...g, blocks };
  });
});

/** Set of row IDs that should be force-expanded because a descendant (alternative) matches
 *  the active tag filter — so the user sees which nested mods caused the parent to appear. */
export const tagFilterForcedExpanded = createMemo(() => {
  const tf = tagFilter();
  if (tf.size === 0) return new Set<string>();
  const fGroups = functionalGroups();

  const anyDescendantMatches = (row: ModRow): boolean =>
    (row.alternatives ?? []).some(alt =>
      fGroups.some(g => tf.has(g.id) && g.modIds.includes(alt.id)) ||
      anyDescendantMatches(alt)
    );

  const forced = new Set<string>();
  const processRow = (row: ModRow) => {
    if (anyDescendantMatches(row)) forced.add(row.id);
    for (const alt of (row.alternatives ?? [])) processRow(alt);
  };
  for (const row of modRowsState()) processRow(row);
  return forced;
});

/** Given any row ID, resolve to the top-level rule that owns it.
 *  For top-level IDs (no "-alternative-") returns the ID unchanged.
 *  For alternative IDs (format: rule-N-alternative-M-…) returns rule-N-<name>. */
export function resolveToTopLevelId(rowId: string): string {
  const match = rowId.match(/^(rule-\d+)-alternative-/);
  if (!match) return rowId;
  // find the top-level row whose ID starts with rule-N-
  const prefix = match[1] + "-";
  for (const row of modRowsState()) {
    if (row.id.startsWith(prefix) && !row.id.includes("-alternative-")) return row.id;
  }
  return rowId;
}

/** Returns the parent row regardless of whether it already has alternatives.
 *  The panel can be opened for any rule (top-level or alternative). If an
 *  alternative ID is given it is resolved to its owning top-level rule. */
export const alternativesPanelParent = createMemo(() => {
  const id = alternativesPanelParentId();
  return id ? (rowMap().get(id) ?? null) : null;
});

export const focusedIncompatibilityMod = createMemo(() => {
  const id = incompatibilityFocusId();
  return id ? rowMap().get(id) ?? null : null;
});

export const priorityParadoxDetected = createMemo(() =>
  createsPriorityParadox(draftIncompatibilities(), modRowsState().map(r => r.id))
);

/** Returns the single selected row ID (any depth) when exactly one row is
 *  selected. Used to enable the "Alternatives" contextual button so any mod
 *  (including alternatives) can open its own alternatives panel. */
export const selectedTopLevelId = createMemo(() => {
  const ids = selectedIds();
  return ids.length === 1 ? ids[0] : null;
});

// ── Helpers ───────────────────────────────────────────────────────────────────

export function wait(ms: number) { return new Promise<void>(res => setTimeout(res, ms)); }

export function rowMatchesQuery(row: ModRow, q: string): boolean {
  return row.name.toLowerCase().includes(q) || (row.alternatives ?? []).some(a => a.name.toLowerCase().includes(q));
}

export function functionalGroupTagClass(tone: string): string {
  const base = "inline-flex items-center gap-0.5 rounded-md border px-1.5 py-0.5 text-[10px]";
  const map: Record<string, string> = {
    violet: `${base} border-primary/30 bg-primary/15 text-primary`,
    sky:    `${base} border-sky-500/30 bg-sky-500/15 text-sky-300`,
    amber:  `${base} border-amber-500/30 bg-amber-500/15 text-amber-300`,
  };
  return map[tone] ?? map["violet"];
}

function createsPriorityParadox(rules: IncompatibilityRule[], allIds: string[]): boolean {
  const adj = new Map<string, string[]>();
  for (const r of rules) { const e = adj.get(r.winnerId) ?? []; e.push(r.loserId); adj.set(r.winnerId, e); }
  const visited = new Set<string>(), stack = new Set<string>();
  function dfs(n: string): boolean {
    if (stack.has(n)) return true;
    if (visited.has(n)) return false;
    visited.add(n); stack.add(n);
    for (const nb of adj.get(n) ?? []) if (dfs(nb)) return true;
    stack.delete(n); return false;
  }
  for (const id of allIds) if (!visited.has(id) && dfs(id)) return true;
  return false;
}

export function moveItemBefore<T extends { id: string }>(items: T[], draggedId: string, targetId: string): T[] {
  const dragged = items.find(i => i.id === draggedId);
  if (!dragged) return items;
  const rest = items.filter(i => i.id !== draggedId);
  const idx = rest.findIndex(i => i.id === targetId);
  if (idx < 0) return items;
  rest.splice(idx, 0, dragged);
  return rest;
}

export function upsertDownloadProgress(update: { filename: string; progress: number }) {
  setDownloadItems(cur => {
    const exists = cur.some(i => i.filename === update.filename);
    const updated: DownloadProgressItem = { filename: update.filename, progress: update.progress, status: update.progress >= 100 ? "complete" : "downloading" };
    if (exists) return cur.map(i => i.filename === update.filename ? updated : i);
    return [...cur, updated];
  });
}

export function pushUiError(error: Omit<LauncherUiError, "id">) {
  setLauncherErrors(cur => [{ id: `ui-error-${Date.now()}`, ...error }, ...cur.slice(0, 19)]);
}

// ── Action handlers ───────────────────────────────────────────────────────────

export function toggleExpanded(id: string) {
  setExpandedRows(cur => cur.includes(id) ? cur.filter(e => e !== id) : [...cur, id]);
}

export function toggleSelected(id: string) {
  setSelectedIds(cur => cur.includes(id) ? cur.filter(e => e !== id) : [...cur, id]);
}

export function toggleGroupCollapsed(id: string) {
  setAestheticGroups(cur => cur.map(g => g.id === id ? { ...g, collapsed: !g.collapsed } : g));
}

function normalizedGroupName(name: string) {
  return name.trim().toLocaleLowerCase();
}

function aestheticGroupNameExists(name: string, excludeId?: string) {
  const normalized = normalizedGroupName(name);
  return aestheticGroups().some(group => group.id !== excludeId && normalizedGroupName(group.name) === normalized);
}

function functionalGroupNameExists(name: string, excludeId?: string) {
  const normalized = normalizedGroupName(name);
  return functionalGroups().some(group => group.id !== excludeId && normalizedGroupName(group.name) === normalized);
}

export function startGroupRename(id: string, name: string) {
  setEditingGroupId(id); setGroupNameDraft(name);
}

export function nextAestheticGroupName(scopeRowId: string | null = null) {
  const usedNumbers = new Set(
    aestheticGroups()
      .filter(group => (group.scopeRowId ?? null) === scopeRowId)
      .filter(group => group.blockIds.length > 0)
      .map(group => /^Group\s+(\d+)$/i.exec(group.name.trim()))
      .filter((match): match is RegExpExecArray => Boolean(match))
      .map(match => Number(match[1]))
      .filter(number => Number.isFinite(number) && number > 0)
  );

  let next = 1;
  while (usedNumbers.has(next)) next += 1;
  return `Group ${next}`;
}

export function commitGroupRename(id: string) {
  const n = groupNameDraft().trim();
  if (!n) {
    setEditingGroupId(null);
    return;
  }
  if (aestheticGroupNameExists(n, id)) {
    pushUiError({ title: "Duplicate group name", message: `A visual group named '${n}' already exists.`, detail: "Choose a unique visual group name.", severity: "warning", scope: "launch" });
    return;
  }
  setAestheticGroups(cur => cur.map(g => g.id === id ? { ...g, name: n } : g));
  setEditingGroupId(null);
}

export function createAestheticGroup() {
  const ctx = selectionContext();
  if (ctx === "empty") {
    pushUiError({ title: "No mods selected", message: "Select one or more mods before creating a visual group.", detail: "Visual groups are created from the currently selected mods.", severity: "warning", scope: "launch" });
    return;
  }
  if (ctx === "mixed") {
    pushUiError({ title: "Mixed selection", message: "A visual group can only contain mods from the same level.", detail: "Select top-level mods only, or alternatives that share the same direct parent.", severity: "warning", scope: "launch" });
    return;
  }

  const selected = selectedIds();
  const pMap = parentIdByChildId();
  const scopeRowId = ctx === "same-parent" ? (pMap.get(selected[0]) ?? null) : null;

  const id = `ag-${Date.now()}`;
  const name = nextAestheticGroupName(scopeRowId);
  setAestheticGroups(cur => {
    const withoutSelected = cur.map(group => ({
      ...group,
      blockIds: group.blockIds.filter(blockId => !selected.includes(blockId)),
    }));
    return [...withoutSelected, { id, name, collapsed: false, blockIds: selected, scopeRowId }];
  });
  startGroupRename(id, name);
}

export function removeRowsFromAestheticGroups(rowIds: string[]) {
  setAestheticGroups(cur => cur
    .map(group => ({
      ...group,
      blockIds: group.blockIds.filter(blockId => !rowIds.includes(blockId)),
    }))
    .filter(group => group.blockIds.length > 0)
  );
}

export function removeAestheticGroup(id: string) {
  setAestheticGroups(cur => cur.filter(group => group.id !== id));
  if (editingGroupId() === id) setEditingGroupId(null);
}

/** Returns "top-level" | "same-parent" | "mixed" | "empty".
 *  Uses parentIdByChildId — never parses ID strings. */
export function selectionContext(): "top-level" | "same-parent" | "mixed" | "empty" {
  const ids = selectedIds();
  if (ids.length === 0) return "empty";
  const pMap = parentIdByChildId();
  const parents = new Set(ids.map(id => pMap.get(id) ?? null));
  if (parents.size !== 1) return "mixed";
  const parent = [...parents][0];
  return parent === null ? "top-level" : "same-parent";
}

export function toggleTagFilter(groupId: string) {
  setTagFilter(cur => {
    const next = new Set(cur);
    if (next.has(groupId)) next.delete(groupId);
    else next.add(groupId);
    return next;
  });
}

export function removeFunctionalGroupMember(groupId: string, modId: string) {
  setFunctionalGroups(cur =>
    cur
      .map(g => g.id !== groupId ? g : { ...g, modIds: g.modIds.filter(id => id !== modId) })
      .filter(g => g.modIds.length > 0)
  );
  // If the group was deleted entirely, remove it from the active tag filter
  if (!functionalGroups().some(g => g.id === groupId)) {
    setTagFilter(cur => { const next = new Set(cur); next.delete(groupId); return next; });
  }
}

export function createFunctionalGroup() {
  const name = newFunctionalGroupName().trim();
  if (!name || selectedIds().length === 0) return;

  if (functionalGroupNameExists(name)) {
    pushUiError({ title: "Duplicate tag name", message: `A functional group named '${name}' already exists.`, detail: "Functional group names must be unique.", severity: "warning", scope: "launch" });
    return;
  }

  const ctx = selectionContext();
  if (ctx === "mixed") {
    pushUiError({ title: "Mixed selection", message: "A functional tag can only contain mods at the same level.", detail: "Select either top-level rules only, or alternatives of the same parent rule only.", severity: "warning", scope: "launch" });
    return;
  }

  setFunctionalGroups(cur => [...cur, { id: `fg-${Date.now()}`, name, tone: functionalGroupTone(), modIds: selectedIds() }]);
  setFunctionalGroupModalOpen(false); setNewFunctionalGroupName("");
}

export function removeIncompatibility(modAId: string, modBId: string) {
  setSavedIncompatibilities(cur => cur.filter(r =>
    !((r.winnerId === modAId && r.loserId === modBId) ||
      (r.winnerId === modBId && r.loserId === modAId))
  ));
}

export function openIncompatibilityEditor() {
  if (selectedIds().length === 0) return;
  setDraftIncompatibilities(savedIncompatibilities().map(r => ({ ...r })));
  setIncompatibilityFocusId(selectedIds()[0]);
  setIncompatibilityModalOpen(true);
}

export function openLinkModal() {
  const ids = selectedIds();
  if (ids.length < 2) return;
  setDraftLinks(savedLinks().map(l => ({ ...l })));
  setLinkModalModIds([...ids]);
  setLinkModalOpen(true);
}

export function toggleDraftLink(fromId: string, toId: string) {
  setDraftLinks(cur => {
    const hasLink = cur.some(l => l.fromId === fromId && l.toId === toId);
    if (hasLink) {
      return cur.filter(l => !(l.fromId === fromId && l.toId === toId));
    }
    return [...cur, { fromId, toId }];
  });
}

export function saveDraftLinks() {
  setSavedLinks(draftLinks().map(l => ({ ...l })));
  setLinkModalOpen(false);
}

export function removeLink(fromId: string, toId: string) {
  setSavedLinks(cur => cur.filter(l => !(
    (l.fromId === fromId && l.toId === toId) ||
    (l.fromId === toId && l.toId === fromId)
  )));
}

export function setPairConflictEnabled(baseId: string, otherId: string, enabled: boolean) {
  setDraftIncompatibilities(cur => {
    const without = cur.filter(r => !((r.winnerId === baseId && r.loserId === otherId) || (r.winnerId === otherId && r.loserId === baseId)));
    if (!enabled) return without;
    return [...without, { winnerId: baseId, loserId: otherId }];
  });
}

export function setPairWinner(baseId: string, otherId: string, winnerId: string) {
  setDraftIncompatibilities(cur => {
    const without = cur.filter(r => !((r.winnerId === baseId && r.loserId === otherId) || (r.winnerId === otherId && r.loserId === baseId)));
    return [...without, { winnerId, loserId: winnerId === baseId ? otherId : baseId }];
  });
}

export function toggleActiveAccountConnection() {
  setAccounts(cur => cur.map(a =>
    a.id === activeAccountId()
      ? { ...a, status: a.status === "online" ? "offline" : "online", lastMode: a.status === "online" ? "offline" : "microsoft" }
      : a
  ));
}

export function dismissError(id: string) {
  setLauncherErrors(cur => cur.filter(e => e.id !== id));
}

/** Open the alternatives panel for any row (top-level or alternative).
 *  The panel will show THAT row's own alternatives, not the root rule's. */
export function openAlternativesPanel(rowId: string) {
  setAlternativesPanelParentId(rowId);
}

export function openRenameRule(id: string, name: string) {
  setRenameRuleTargetId(id); setRenameRuleDraft(name); setRenameRuleModalOpen(true);
}

export function resetLaunchUiState() {
  setLaunchState("idle"); setLaunchProgress(0);
  setLaunchStageLabel(LAUNCH_STAGES[0].label); setLaunchStageDetail(LAUNCH_STAGES[0].detail);
  setLaunchLogs([]);
  setDownloadItems([]);
}

export function addModToFunctionalGroup(groupId: string, modId: string) {
  setFunctionalGroups(cur => cur.map(g =>
    g.id === groupId && !g.modIds.includes(modId) ? { ...g, modIds: [...g.modIds, modId] } : g
  ));
}

export function createFunctionalGroupForMod(name: string, modId: string) {
  const trimmed = name.trim();
  if (!trimmed) return;
  setFunctionalGroups(cur => [...cur, { id: `fg-${Date.now()}`, name: trimmed, tone: "violet", modIds: [modId] }]);
}

export function addVersionRule(rule: Omit<VersionRule, 'id'>) {
  setVersionRules(cur => [...cur, { ...rule, id: `vr-${Date.now()}` }]);
}

export function removeVersionRule(id: string) {
  setVersionRules(cur => cur.filter(r => r.id !== id));
}

export function updateVersionRule(id: string, patch: Partial<Omit<VersionRule, 'id' | 'modId'>>) {
  setVersionRules(cur => cur.map(r => r.id === id ? { ...r, ...patch } : r));
}

export function addCustomConfig(modId: string) {
  setCustomConfigs(cur => [...cur, { id: `cc-${Date.now()}`, modId, mcVersions: [], loader: 'any', targetPath: '', files: [] }]);
}

export function removeCustomConfig(id: string) {
  setCustomConfigs(cur => cur.filter(c => c.id !== id));
}

export function updateCustomConfig(id: string, patch: Partial<Omit<CustomConfig, 'id' | 'modId'>>) {
  setCustomConfigs(cur => cur.map(c => c.id === id ? { ...c, ...patch } : c));
}
