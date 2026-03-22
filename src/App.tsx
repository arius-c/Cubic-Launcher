import { createEffect, createSignal, onMount, onCleanup, batch } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { appendDebugTrace, clearDebugTrace } from "./lib/debugTrace";

import {
  setModListCards, selectedModListName, setSelectedModListName,
  modRowsState, setModRowsState, setAccounts, setActiveAccountId,
  setLaunchState, setLaunchProgress, setLaunchStageLabel, setLaunchStageDetail,
  setLaunchLogs, setLogViewerOpen, setLauncherErrors,
  globalSettings, modlistOverrides, selectedMcVersion, selectedModLoader,
  createModlistName, createModlistDescription, setCreateModlistModalOpen,
  setCreateModlistBusy, createModlistBusy,
  renameRuleDraft, renameRuleTargetId, setRenameRuleModalOpen,
  selectedIds, setSelectedIds, setExpandedRows, activeAccount, setAddModModalOpen,
  setSettingsModalOpen, localJarRuleName, setLocalJarRuleName,
  setGlobalSettings, setModlistOverrides,
  upsertDownloadProgress, pushUiError, resetLaunchUiState,
  aestheticGroups, functionalGroups, setAestheticGroups, setFunctionalGroups, setSavedIncompatibilities,
  draftIncompatibilities, setIncompatibilityModalOpen,
  instancePresentation, setInstancePresentation,
  exportOptions, setExportModalOpen, setInstancePresentationOpen,
  alternativesPanelParent, setAlternativesPanelParentId,
  launchState, selectedModList, setAppLoading, setModIcons, modIcons,
  LAUNCH_STAGES, wait,
  DEMO_MOD_LISTS, DEMO_ACCOUNTS,
} from "./store";
import type { AestheticGroup, FunctionalGroup, ModRow } from "./lib/types";

// ── Row state helpers ─────────────────────────────────────────────────────────

/**
 * Rebuild position-based row IDs after an in-place reorder or delete.
 * Preserves the same object reference for rows whose ID did not change,
 * so SolidJS <For> doesn't recreate components unnecessarily.
 */
function rebuildRowIds(rows: ModRow[]): ModRow[] {
  const rebuildAlternatives = (alternatives: ModRow[], ruleIndex: number, parentPath: string): ModRow[] =>
    alternatives.map((alt, altIndex) => {
      const namePart = alt.id.replace(/^rule-\d+(?:-alternative-\d+)*-/, "");
      const nextPath = `${parentPath}-alternative-${altIndex + 1}`;
      const nextId = `rule-${ruleIndex}${nextPath}-${namePart}`;
      const nextAlternatives = rebuildAlternatives(alt.alternatives ?? [], ruleIndex, nextPath);
      const unchanged =
        nextId === alt.id &&
        nextAlternatives.length === (alt.alternatives?.length ?? 0) &&
        nextAlternatives.every((candidate, index) => candidate === (alt.alternatives ?? [])[index]);

      return unchanged ? alt : { ...alt, id: nextId, alternatives: nextAlternatives };
    });

  return rows.map((row, ruleIndex) => {
    const namePart = row.id.replace(/^rule-\d+-/, "");
    const newId = `rule-${ruleIndex}-${namePart}`;
    const alternatives = rebuildAlternatives(row.alternatives ?? [], ruleIndex, "");
    const unchanged =
      newId === row.id &&
      alternatives.length === (row.alternatives?.length ?? 0) &&
      alternatives.every((candidate, index) => candidate === (row.alternatives ?? [])[index]);

    return unchanged ? row : { ...row, id: newId, alternatives };
  });
}

/**
 * Merge a new rows snapshot into the current state while preserving existing
 * object references for rows that haven't changed.
 *
 * SolidJS's <For> compares items by reference (===).  When loadEditorSnapshot
 * replaces the whole array with new objects, <For> destroys every component
 * and recreates it — visually appearing as a "close/reopen".  By keeping the
 * same reference for unchanged rows, only genuinely different rows re-render.
 */
function smartSetModRows(current: ModRow[], next: ModRow[]): ModRow[] {
  if (current.length === 0) return next;

  const byId = new Map(current.map(r => [r.id, r]));
  return next.map(nextRow => {
    const cur = byId.get(nextRow.id);
    if (!cur) return nextRow; // newly added row

    const mergedAlternatives = smartSetModRows(cur.alternatives ?? [], nextRow.alternatives ?? []);
    const alternativesUnchanged =
      mergedAlternatives.length === (cur.alternatives?.length ?? 0) &&
      mergedAlternatives.every((alt, index) => alt === (cur.alternatives ?? [])[index]);

    // Shallow-compare fields that affect rendering.
    if (
      cur.name       === nextRow.name &&
      cur.kind       === nextRow.kind &&
      cur.area       === nextRow.area &&
      cur.modrinth_id=== nextRow.modrinth_id &&
      cur.note       === nextRow.note &&
      cur.tags.length === nextRow.tags.length &&
      cur.tags.every((tag, index) => tag === nextRow.tags[index]) &&
      alternativesUnchanged
    ) {
      return cur; // unchanged — preserve reference so <For> skips re-render
    }
    return { ...nextRow, alternatives: mergedAlternatives };
  });
}

function findRowNamePath(rows: ModRow[], targetId: string, parentPath: string[] = []): string[] | null {
  for (const row of rows) {
    const path = [...parentPath, row.name];
    if (row.id === targetId) return path;
    const nested = findRowNamePath(row.alternatives ?? [], targetId, path);
    if (nested) return nested;
  }
  return null;
}

function findRowIdByNamePath(rows: ModRow[], namePath: string[]): string | null {
  if (namePath.length === 0) return null;

  const [head, ...rest] = namePath;
  const row = rows.find(candidate => candidate.name === head);
  if (!row) return null;
  if (rest.length === 0) return row.id;
  return findRowIdByNamePath(row.alternatives ?? [], rest);
}

function buildDefaultIconLabel(modlistName: string): string {
  const initials = modlistName
    .split(/\s+/)
    .filter(Boolean)
    .map(segment => segment[0])
    .join("")
    .slice(0, 3)
    .toUpperCase();

  return initials || "ML";
}

function normalizeIdentifier(value: string): string {
  return value
    .split("")
    .map(character => /[a-z0-9]/i.test(character) ? character.toLowerCase() : "-")
    .join("")
    .replace(/^-+|-+$/g, "");
}

function renameRowId(rowId: string, newName: string): string {
  const alternativeMatch = rowId.match(/^(rule-\d+(?:-alternative-\d+)*)-/);
  if (alternativeMatch) return `${alternativeMatch[1]}-${normalizeIdentifier(newName)}`;

  const ruleMatch = rowId.match(/^(rule-\d+)-/);
  if (ruleMatch) return `${ruleMatch[1]}-${normalizeIdentifier(newName)}`;

  return rowId;
}

function buildIdRemap(previousRows: ModRow[], nextRows: ModRow[]): Map<string, string> {
  const remap = new Map<string, string>();

  const collect = (previous: ModRow[], next: ModRow[]) => {
    previous.forEach((previousRow, index) => {
      const nextRow = next[index];
      if (!nextRow) return;
      remap.set(previousRow.id, nextRow.id);
      collect(previousRow.alternatives ?? [], nextRow.alternatives ?? []);
    });
  };

  collect(previousRows, nextRows);

  return remap;
}

function reorderAlternativesInRows(rows: ModRow[], parentId: string, orderedAltIds: string[]): ModRow[] {
  return rows.map(row => {
    if (row.id === parentId && row.alternatives) {
      const altMap = new Map(row.alternatives.map(alt => [alt.id, alt]));
      const reordered = orderedAltIds.map(id => altMap.get(id)).filter(Boolean) as ModRow[];
      const unchanged =
        reordered.length === row.alternatives.length &&
        reordered.every((candidate, index) => candidate === row.alternatives![index]);
      return unchanged ? row : { ...row, alternatives: reordered };
    }

    const nextAlternatives = reorderAlternativesInRows(row.alternatives ?? [], parentId, orderedAltIds);
    const unchanged =
      nextAlternatives.length === (row.alternatives?.length ?? 0) &&
      nextAlternatives.every((candidate, index) => candidate === (row.alternatives ?? [])[index]);
    return unchanged ? row : { ...row, alternatives: nextAlternatives };
  });
}

function collectRowIds(rows: ModRow[]): Set<string> {
  const ids = new Set<string>();
  const collect = (items: ModRow[]) => {
    for (const row of items) {
      ids.add(row.id);
      if (row.alternatives?.length) collect(row.alternatives);
    }
  };
  collect(rows);
  return ids;
}

function remapIds(ids: string[], idRemap: Map<string, string>, validIds: Set<string>): string[] {
  return ids
    .map(id => idRemap.get(id) ?? id)
    .filter(id => validIds.has(id))
    .filter((id, index, all) => all.indexOf(id) === index);
}

function remapAestheticGroups(groups: AestheticGroup[], idRemap: Map<string, string>, validIds: Set<string>): AestheticGroup[] {
  return groups.map(group => ({
    ...group,
    blockIds: remapIds(group.blockIds, idRemap, validIds),
    scopeRowId: group.scopeRowId ? (idRemap.get(group.scopeRowId) ?? group.scopeRowId) : group.scopeRowId,
  }));
}

function remapFunctionalGroups(groups: FunctionalGroup[], idRemap: Map<string, string>, validIds: Set<string>): FunctionalGroup[] {
  return groups.map(group => ({
    ...group,
    modIds: remapIds(group.modIds, idRemap, validIds),
  }));
}

function serializeGroupsLayout(aGroups: AestheticGroup[], fGroups: FunctionalGroup[]) {
  return JSON.stringify({
    aestheticGroups: aGroups.map(group => ({
      id: group.id,
      name: group.name,
      collapsed: group.collapsed,
      blockIds: [...group.blockIds],
      scopeRowId: group.scopeRowId ?? null,
    })),
    functionalGroups: fGroups.map(group => ({
      id: group.id,
      name: group.name,
      tone: group.tone,
      modIds: [...group.modIds],
    })),
  });
}
import { Header }            from "./components/Header";
import { Sidebar }           from "./components/Sidebar";
import { ModListEditor }     from "./components/ModListEditor";
import { LaunchPanel }       from "./components/LaunchPanel";
import { AddModDialog }      from "./components/AddModDialog";
import {
  CreateModlistModal, SettingsModal, AccountsModal,
  FunctionalGroupModal, InstancePresentationModal, RenameRuleModal, IncompatibilitiesModal,
  AlternativesPanel, ErrorCenter, ExportModal,
} from "./components/Modals";

// ─────────────────────────────────────────────────────────────────────────────

const isTauri = () => "__TAURI_INTERNALS__" in window;

// ── Backend data loaders ──────────────────────────────────────────────────────

async function loadShellSnapshot(preferredName?: string | null) {
  try {
    const snap: any = await invoke("load_shell_snapshot_command", {
      selectedModlistName: preferredName ?? null,
    });

    // Always update mod list cards — even if empty (clears stale state)
    const cards = (snap.modlists ?? []).map((m: any) => ({
      name: m.name,
      status: "Ready" as const,
      accent: "from-primary/30 via-primary/10 to-transparent",
      description: m.description || (m.rule_count > 0 ? `${m.rule_count} rules` : "Empty mod list"),
    }));
    setModListCards(cards);

    // Select the preferred list or the first one
    if (cards.length > 0) {
      const sel = preferredName && cards.some((c: any) => c.name === preferredName)
        ? preferredName
        : cards[0].name;
      setSelectedModListName(sel);
    } else {
      setSelectedModListName("");
    }

    // Active account from backend
    if (snap.active_account) {
      const a = snap.active_account;
      const gtag = a.xbox_gamertag?.trim() || a.microsoft_id;
      setAccounts(cur => {
        const rest = cur.filter((x: any) => x.id !== a.microsoft_id);
        return [{ id: a.microsoft_id, gamertag: gtag, email: a.microsoft_id, status: a.status, lastMode: a.last_mode }, ...rest];
      });
      setActiveAccountId(a.microsoft_id);
    }

    // Global settings
    const gs = snap.global_settings;
    if (gs) {
      setGlobalSettings({
        minRamMb: gs.min_ram_mb ?? 2048,
        maxRamMb: gs.max_ram_mb ?? 4096,
        customJvmArgs: gs.custom_jvm_args ?? "",
        profilerEnabled: gs.profiler_enabled ?? false,
        wrapperCommand: gs.wrapper_command ?? "",
        javaPathOverride: gs.java_path_override ?? "",
      });
    }

    // Per-modlist overrides
    const ov = snap.selected_modlist_overrides;
    if (ov) {
      setModlistOverrides(cur => ({
        ...cur,
        minRamEnabled: ov.min_ram_mb != null,
        minRamMb: ov.min_ram_mb ?? cur.minRamMb,
        maxRamEnabled: ov.max_ram_mb != null,
        maxRamMb: ov.max_ram_mb ?? cur.maxRamMb,
        customArgsEnabled: ov.custom_jvm_args != null,
        customJvmArgs: ov.custom_jvm_args ?? cur.customJvmArgs,
        profilerEnabled: ov.profiler_enabled != null,
        profilerActive: ov.profiler_enabled ?? cur.profilerActive,
        wrapperEnabled: ov.wrapper_command != null,
        wrapperCommand: ov.wrapper_command ?? cur.wrapperCommand,
      }));
    }

    return snap;
  } catch (err) {
    pushUiError({ title: "Could not load launcher state", message: "The backend returned an error on startup.", detail: String(err), severity: "error", scope: "launch" });
    return null;
  }
}

async function loadEditorSnapshot(modlistName: string, resetGroups = false) {
  if (!modlistName) return null;
  try {
    const snap: any = await invoke("load_modlist_editor_command", {
      selectedModlistName: modlistName,
    });

    // Smart merge: preserve unchanged row references so SolidJS <For> doesn't
    // destroy and recreate every component on every reload.
    setModRowsState(cur => smartSetModRows(cur, snap.rows ?? []));

    // Only wipe frontend-only groups when switching to a different list.
    if (resetGroups) {
      setAestheticGroups([]);
      setFunctionalGroups([]);
    }
    const incompat: Array<{ winner_id: string; loser_id: string }> = snap.incompatibilities ?? [];
    setSavedIncompatibilities(incompat.map(r => ({ winnerId: r.winner_id, loserId: r.loser_id })));
    return snap;
  } catch (err) {
    pushUiError({ title: "Could not load mod list", message: `Failed to read rules for '${modlistName}'.`, detail: String(err), severity: "error", scope: "launch" });
    setModRowsState([]);
    return null;
  }
}

async function loadModlistPresentation(modlistName: string) {
  if (!modlistName) {
    setInstancePresentation({ iconLabel: "ML", iconAccent: "", notes: "" });
    return null;
  }

  if (!isTauri()) {
    setInstancePresentation({ iconLabel: buildDefaultIconLabel(modlistName), iconAccent: "", notes: "" });
    return null;
  }

  try {
    const presentation: any = await invoke("load_modlist_presentation_command", { modlistName });
    setInstancePresentation({
      iconLabel: presentation.iconLabel ?? buildDefaultIconLabel(modlistName),
      iconAccent: presentation.iconAccent ?? "",
      notes: presentation.notes ?? "",
    });
    return presentation;
  } catch (err) {
    setInstancePresentation({ iconLabel: buildDefaultIconLabel(modlistName), iconAccent: "", notes: "" });
    pushUiError({ title: "Could not load notes", message: `Saved notes for '${modlistName}' could not be loaded.`, detail: String(err), severity: "error", scope: "launch" });
    return null;
  }
}

async function loadModlistGroups(modlistName: string, rows: ModRow[]) {
  if (!modlistName) {
    setAestheticGroups([]);
    setFunctionalGroups([]);
    return null;
  }

  if (!isTauri()) {
    setAestheticGroups([]);
    setFunctionalGroups([]);
    return null;
  }

  try {
    const layout: any = await invoke("load_modlist_groups_command", { modlistName });
    const availableIds = new Set<string>();
    const collect = (items: ModRow[]) => {
      for (const row of items) {
        availableIds.add(row.id);
        if (row.alternatives?.length) collect(row.alternatives);
      }
    };
    collect(rows);

    const nextAestheticGroups = (layout.aestheticGroups ?? []).map((group: any) => ({
      id: group.id,
      name: group.name,
      collapsed: Boolean(group.collapsed),
      blockIds: (group.blockIds ?? []).filter((id: string) => availableIds.has(id)),
      scopeRowId: group.scopeRowId ?? null,
    })).filter((group: any) => group.blockIds.length > 0);
    const nextFunctionalGroups = (layout.functionalGroups ?? []).map((group: any) => ({
      id: group.id,
      name: group.name,
      tone: group.tone,
      modIds: (group.modIds ?? []).filter((id: string) => availableIds.has(id)),
    }));

    setAestheticGroups(nextAestheticGroups);
    setFunctionalGroups(nextFunctionalGroups);
    return layout;
  } catch (err) {
    setAestheticGroups([]);
    setFunctionalGroups([]);
    pushUiError({ title: "Could not load groups", message: `Saved groups for '${modlistName}' could not be loaded.`, detail: String(err), severity: "error", scope: "launch" });
    return null;
  }
}

// ── Icon fetching ─────────────────────────────────────────────────────────────

function collectModrinthIds(rows: ModRow[]): Set<string> {
  const ids = new Set<string>();
  const collect = (list: ModRow[]) => {
    for (const r of list) {
      if (r.modrinth_id) ids.add(r.modrinth_id);
      if (r.alternatives) collect(r.alternatives);
    }
  };
  collect(rows);
  return ids;
}

async function fetchModIcons(rows: ModRow[]) {
  const ids = [...collectModrinthIds(rows)];
  if (ids.length === 0) return;

  // Skip IDs we already have icons for
  const missing = ids.filter(id => !modIcons().has(id));
  if (missing.length === 0) return;

  try {
    const param = encodeURIComponent(JSON.stringify(missing));
    const res = await fetch(
      `https://api.modrinth.com/v2/projects?ids=${param}`,
      { headers: { "User-Agent": "CubicLauncher/0.1.0" } }
    );
    if (!res.ok) return;
    const projects: Array<{ slug: string; icon_url?: string | null }> = await res.json();
    const updated = new Map(modIcons());
    for (const p of projects) {
      if (p.slug && p.icon_url) updated.set(p.slug, p.icon_url);
    }
    setModIcons(updated);
  } catch {
    // Icons are decorative — swallow errors silently
  }
}

// ─────────────────────────────────────────────────────────────────────────────

export default function App() {
  const [groupLayoutReady, setGroupLayoutReady] = createSignal(false);
  const [lastSavedGroupLayout, setLastSavedGroupLayout] = createSignal("");

  createEffect(() => {
    const modlistName = selectedModListName();
    const ready = groupLayoutReady();
    const serialized = serializeGroupsLayout(aestheticGroups(), functionalGroups());

    if (!ready || !modlistName || !isTauri() || serialized === lastSavedGroupLayout()) return;

    setLastSavedGroupLayout(serialized);
    void invoke("save_modlist_groups_command", {
      input: {
        modlistName,
        aestheticGroups: aestheticGroups(),
        functionalGroups: functionalGroups(),
      },
    }).catch(err => {
      setLastSavedGroupLayout("");
      pushUiError({ title: "Could not save groups", message: "The mod-list group layout could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
    });
  });

  // ── Startup ────────────────────────────────────────────────────────────────
  onMount(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    const boot = async () => {
      if (!isTauri()) {
        // Browser-only mode: use demo data so the UI is explorable
        setModListCards(DEMO_MOD_LISTS);
        setSelectedModListName(DEMO_MOD_LISTS[0]?.name ?? "");
        setAccounts(DEMO_ACCOUNTS);
        setActiveAccountId(DEMO_ACCOUNTS[0]?.id ?? "");
        setAppLoading(false);
        return;
      }

      const tracePath = await clearDebugTrace();
      appendDebugTrace("app.boot", { tracePath, phase: "start" });

      // Real Tauri app: load everything from the backend
      const snap = await loadShellSnapshot(null);
      appendDebugTrace("app.boot", {
        phase: "shell-loaded",
        modlists: (snap?.modlists ?? []).map((modlist: any) => modlist.name),
      });
      setAppLoading(false);

      // Load editor data + icons for the selected mod list
      const firstList = snap?.modlists?.[0]?.name ?? "";
      if (firstList) {
        const editorSnapshot = await loadEditorSnapshot(firstList);
        await loadModlistPresentation(firstList);
        await loadModlistGroups(firstList, editorSnapshot?.rows ?? modRowsState());
        setLastSavedGroupLayout(serializeGroupsLayout(aestheticGroups(), functionalGroups()));
        setGroupLayoutReady(true);
        appendDebugTrace("app.boot", {
          phase: "editor-loaded",
          firstList,
          rowCount: editorSnapshot?.rows?.length ?? modRowsState().length,
        });
        void fetchModIcons(modRowsState());
      }

      // Wire up Tauri event listeners
      const { listen } = await import("@tauri-apps/api/event");

      unlisteners.push(await listen<{ filename: string; percentage: number }>("download-progress", ev => {
        if (ev.payload) upsertDownloadProgress({ filename: ev.payload.filename, progress: ev.payload.percentage });
      }));

      unlisteners.push(await listen<any>("launcher-error", ev => {
        if (!ev.payload) return;
        const p = ev.payload;
        setLauncherErrors(cur => {
          const e = { id: p.id ?? `le-${Date.now()}`, title: p.title, message: p.message, detail: p.detail ?? p.message, severity: p.severity ?? "error", scope: p.scope ?? "launch" };
          return [e, ...cur.filter((x: any) => x.id !== e.id)];
        });
      }));

      unlisteners.push(await listen<any>("launch-progress", ev => {
        if (!ev.payload) return;
        setLaunchState(ev.payload.state);
        setLaunchProgress(ev.payload.progress);
        setLaunchStageLabel(ev.payload.stage);
        setLaunchStageDetail(ev.payload.detail);
      }));

      unlisteners.push(await listen<any>("minecraft-log", ev => {
        if (ev.payload) {
          setLaunchLogs(c => [...c, `[${ev.payload.stream}] ${ev.payload.line}`]);
          setLogViewerOpen(true);
        }
      }));

      unlisteners.push(await listen<any>("minecraft-exit", ev => {
        if (!ev.payload) return;
        setLaunchState("idle");
        setLaunchLogs(c => [...c, `[exit] Process finished with code ${ev.payload.exitCode ?? ev.payload.exit_code ?? "unknown"}`]);
      }));

      if (disposed) unlisteners.forEach(u => u());
    };

    void boot();
    onCleanup(() => { disposed = true; unlisteners.forEach(u => u()); });
  });

  // ── Handlers ──────────────────────────────────────────────────────────────

  const handleSelectModList = async (name: string) => {
    appendDebugTrace("modlist.select", { name });
    setSelectedModListName(name);
    setSelectedIds([]);
    setGroupLayoutReady(false);
    if (!isTauri()) return;
    await loadShellSnapshot(name);
    const editorSnapshot = await loadEditorSnapshot(name, true);
    await loadModlistPresentation(name);
    await loadModlistGroups(name, editorSnapshot?.rows ?? modRowsState());
    setLastSavedGroupLayout(serializeGroupsLayout(aestheticGroups(), functionalGroups()));
    setGroupLayoutReady(true);
    void fetchModIcons(modRowsState());
  };

  const handleReorderRules = async (orderedIds: string[]) => {
    if (!isTauri() || !selectedModListName()) return;
    try {
      await invoke("reorder_rules_command", {
        input: { modlistName: selectedModListName(), orderedRowIds: orderedIds },
      });
      const currentRows = modRowsState();
      const rebuiltRows = rebuildRowIds(currentRows);
      const idRemap = buildIdRemap(currentRows, rebuiltRows);
      const validIds = collectRowIds(rebuiltRows);
      batch(() => {
        setModRowsState(rebuiltRows);
        setAestheticGroups(groups => remapAestheticGroups(groups, idRemap, validIds));
        setFunctionalGroups(groups => remapFunctionalGroups(groups, idRemap, validIds));
        setSavedIncompatibilities(rules => rules.map(r => ({
          winnerId: idRemap.get(r.winnerId) ?? r.winnerId,
          loserId:  idRemap.get(r.loserId)  ?? r.loserId,
        })));
        setSelectedIds(ids => remapIds(ids, idRemap, validIds));
        setExpandedRows(ids => remapIds(ids, idRemap, validIds));
        setAlternativesPanelParentId(id => (id && idRemap.has(id) ? idRemap.get(id)! : id));
      });
    } catch (err) {
      pushUiError({ title: "Reorder failed", message: "The new rule order could not be saved.", detail: String(err), severity: "error", scope: "launch" });
      await loadEditorSnapshot(selectedModListName()); // restore original order on failure
    }
  };

  const handleSaveAlternativeOrder = async (parentId: string, orderedAltIds: string[]) => {
    if (!selectedModListName()) return;
    appendDebugTrace("alts.reorder.frontend", { parentId, orderedAltIds, modlistName: selectedModListName() });

    // Optimistic: reorder alternatives in local state immediately, at any depth.
    setModRowsState(cur => reorderAlternativesInRows(cur, parentId, orderedAltIds));

    if (!isTauri()) return;
    try {
      await invoke("save_alternative_order_command", {
        input: { modlistName: selectedModListName(), parentRowId: parentId, orderedAlternativeIds: orderedAltIds },
      });
      appendDebugTrace("alts.reorder.frontend", { parentId, status: "saved" });
    } catch (err) {
      appendDebugTrace("alts.reorder.frontend", { parentId, status: "error", detail: String(err) });
      pushUiError({ title: "Could not save alternative order", message: "The fallback order could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
      await loadEditorSnapshot(selectedModListName()); // restore on failure
    }
  };

  const handleAddAlternative = async (parentId: string, altRowId: string) => {
    if (!isTauri() || !selectedModListName()) return;
    // If the parent is itself an alternative (contains "-alternative-"),
    // use the nested alternative command; otherwise use the flat command.
    const isNested = parentId.includes("-alternative-");
    const parentNamePath = findRowNamePath(modRowsState(), parentId);
    appendDebugTrace("alts.add.frontend", {
      modlistName: selectedModListName(),
      parentId,
      altRowId,
      isNested,
      parentNamePath,
    });
    try {
      if (isNested) {
        await invoke("add_nested_alternative_command", {
          input: { modlistName: selectedModListName(), parentAltRowId: parentId, alternativeRowId: altRowId },
        });
      } else {
        await invoke("add_alternative_command", {
          input: { modlistName: selectedModListName(), parentRowId: parentId, alternativeRowId: altRowId },
        });
      }
      await loadEditorSnapshot(selectedModListName());
      if (parentNamePath) {
        const nextParentId = findRowIdByNamePath(modRowsState(), parentNamePath);
        if (nextParentId) setAlternativesPanelParentId(nextParentId);
        appendDebugTrace("alts.add.frontend", {
          status: "reloaded",
          parentId,
          nextParentId,
          parentNamePath,
        });
      }
      void fetchModIcons(modRowsState());
    } catch (err) {
      appendDebugTrace("alts.add.frontend", { status: "error", parentId, altRowId, detail: String(err) });
      pushUiError({ title: "Could not add alternative", message: "The mod could not be added as a fallback.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleRemoveAlternative = async (altRowId: string) => {
    if (!isTauri() || !selectedModListName()) return;
    const activePanelPath = alternativesPanelParent()
      ? findRowNamePath(modRowsState(), alternativesPanelParent()!.id)
      : null;
    appendDebugTrace("alts.remove.frontend", {
      modlistName: selectedModListName(),
      altRowId,
      activePanelId: alternativesPanelParent()?.id ?? null,
      activePanelPath,
    });
    try {
      await invoke("remove_alternative_command", {
        input: { modlistName: selectedModListName(), alternativeRowId: altRowId },
      });
      await loadEditorSnapshot(selectedModListName());
      if (activePanelPath) {
        const nextParentId = findRowIdByNamePath(modRowsState(), activePanelPath);
        if (nextParentId) setAlternativesPanelParentId(nextParentId);
        else setAlternativesPanelParentId(null);
        appendDebugTrace("alts.remove.frontend", {
          status: "reloaded",
          altRowId,
          nextParentId: nextParentId ?? null,
          activePanelPath,
        });
      }
    } catch (err) {
      appendDebugTrace("alts.remove.frontend", { status: "error", altRowId, detail: String(err) });
      pushUiError({ title: "Could not remove alternative", message: "The mod could not be detached as a fallback.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleSwitchAccount = async (id: string) => {
    setActiveAccountId(id);
    if (!isTauri()) return;
    try {
      await invoke("switch_active_account_command", { microsoftId: id });
      await loadShellSnapshot(selectedModListName() || null);
    } catch (err) {
      pushUiError({ title: "Account switch failed", message: "Could not activate the selected account.", detail: String(err), severity: "error", scope: "account" });
    }
  };

  const handleCreateModlist = async () => {
    const name = createModlistName().trim();
    if (!name || createModlistBusy()) return;
    setCreateModlistBusy(true);

    if (!isTauri()) {
      // Browser demo: just add to the list locally
      setModListCards(cur => [...cur, { name, status: "Ready", accent: "from-primary/30 via-primary/10 to-transparent", description: createModlistDescription().trim() || "New mod list." }]);
      setSelectedModListName(name);
      setModRowsState([]);
      setCreateModlistModalOpen(false);
      setCreateModlistBusy(false);
      return;
    }

    try {
      await invoke("create_modlist_command", {
        input: {
          name,
          author: activeAccount()?.gamertag ?? "Author",
          description: createModlistDescription().trim(),
        },
      });
      setCreateModlistModalOpen(false);
      await loadShellSnapshot(name);
      const editorSnapshot = await loadEditorSnapshot(name);
      await loadModlistPresentation(name);
      await loadModlistGroups(name, editorSnapshot?.rows ?? modRowsState());
      setLastSavedGroupLayout(serializeGroupsLayout(aestheticGroups(), functionalGroups()));
      setGroupLayoutReady(true);
    } catch (err) {
      pushUiError({ title: "Failed to create mod list", message: `'${name}' could not be created.`, detail: String(err), severity: "error", scope: "launch" });
    } finally {
      setCreateModlistBusy(false);
    }
  };

  const handleAddModrinth = async (modId: string, displayName: string) => {
    // Optimistic insert: append a placeholder row immediately so the list
    // never fully re-renders.  The backend normalises the rule name the same
    // way we compute the ID here, so the reference is stable after the reload.
    const tempId = `rule-${modRowsState().length}-${modId}`;
    const tempRow: ModRow = {
      id: tempId,
      name: displayName,
      modrinth_id: modId,
      kind: "modrinth",
      area: "Rule",
      note: "Primary option with 1 mod.",
      tags: [],
      alternatives: [],
    };
    setModRowsState(c => [...c, tempRow]);
    void fetchModIcons([tempRow]);

    if (!isTauri()) return;
    if (!selectedModListName()) return;
    try {
      await invoke("add_mod_rule_command", {
        input: { modlistName: selectedModListName(), ruleName: displayName, modId, modSource: "modrinth", fileName: null },
      });
      // Smart-merge: only the new row changes (or its ID gets corrected); existing rows keep references.
      await loadEditorSnapshot(selectedModListName());
    } catch (err) {
      // Remove the optimistic row on failure.
      setModRowsState(c => c.filter(r => r.id !== tempId));
      pushUiError({ title: "Failed to add mod", message: `'${displayName}' could not be added.`, detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleUploadLocal = async () => {
    if (!isTauri()) {
      pushUiError({ title: "Desktop app required", message: "Local JAR upload requires the Cubic Launcher desktop app.", detail: "The file picker is only available when running inside Tauri.", severity: "warning", scope: "launch" });
      return;
    }
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ title: "Select a mod JAR", filters: [{ name: "JAR files", extensions: ["jar"] }], multiple: false, directory: false });
      if (!selected || !selectedModListName()) return;
      await invoke("copy_local_jar_command", {
        input: {
          sourcePath: selected as string,
          ruleName: localJarRuleName().trim(),
          modlistName: selectedModListName(),
        },
      });
      setLocalJarRuleName("");
      setAddModModalOpen(false);
      await loadEditorSnapshot(selectedModListName());
    } catch (err) {
      pushUiError({ title: "Failed to upload JAR", message: "The file could not be copied to the mod cache.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleDeleteSelected = async () => {
    const ids = selectedIds();
    if (ids.length === 0) return;
    const idSet = new Set(ids);

    // Optimistic update: remove top-level rules AND nested alternatives.
    setModRowsState(cur =>
      cur
        .filter(r => !idSet.has(r.id))
        .map(r => {
          if (!r.alternatives?.length) return r;
          const filteredAlts = r.alternatives.filter(a => !idSet.has(a.id));
          // Preserve reference if nothing changed.
          return filteredAlts.length === r.alternatives.length
            ? r
            : { ...r, alternatives: filteredAlts };
        })
    );
    setSelectedIds([]);
    if (!isTauri() || !selectedModListName()) return;
    try {
      await invoke("delete_rules_command", { input: { modlistName: selectedModListName(), rowIds: ids } });
      const currentRows = modRowsState();
      const rebuiltRows = rebuildRowIds(currentRows);
      const idRemap = buildIdRemap(currentRows, rebuiltRows);
      const validIds = collectRowIds(rebuiltRows);
      setModRowsState(rebuiltRows);
      setAestheticGroups(groups => remapAestheticGroups(groups, idRemap, validIds));
      setFunctionalGroups(groups => remapFunctionalGroups(groups, idRemap, validIds));
    } catch (err) {
      pushUiError({ title: "Failed to delete rules", message: "The selected rules could not be removed.", detail: String(err), severity: "error", scope: "launch" });
      await loadEditorSnapshot(selectedModListName()); // restore on failure
    }
  };

  const handleRenameRule = async () => {
    const id = renameRuleTargetId();
    const name = renameRuleDraft().trim();
    if (!id || !name) return;
    const nextId = renameRowId(id, name);
    const idRemap = new Map([[id, nextId]]);
    const currentRows = modRowsState().map(r => r.id === id ? { ...r, id: nextId, name } : r);
    const validIds = collectRowIds(currentRows);
    setModRowsState(currentRows);
    setAestheticGroups(groups => remapAestheticGroups(groups, idRemap, validIds));
    setFunctionalGroups(groups => remapFunctionalGroups(groups, idRemap, validIds));
    setSelectedIds(ids => remapIds(ids, idRemap, validIds));
    setRenameRuleModalOpen(false);
    if (!isTauri() || !selectedModListName()) return;
    try {
      await invoke("rename_rule_command", { input: { modlistName: selectedModListName(), rowId: id, newName: name } });
      // Optimistic update already applied — no reload needed.
    } catch (err) {
      pushUiError({ title: "Rename failed", message: "The rule name could not be saved.", detail: String(err), severity: "error", scope: "launch" });
      await loadEditorSnapshot(selectedModListName()); // restore on failure
    }
  };

  const handleSaveSettings = async () => {
    if (!isTauri()) { setSettingsModalOpen(false); return; }
    try {
      const g = globalSettings();
      await invoke("save_global_settings_command", {
        settings: { minRamMb: g.minRamMb, maxRamMb: g.maxRamMb, customJvmArgs: g.customJvmArgs, profilerEnabled: g.profilerEnabled, wrapperCommand: g.wrapperCommand, javaPathOverride: g.javaPathOverride },
      });
      if (selectedModListName()) {
        const ov = modlistOverrides();
        await invoke("save_modlist_overrides_command", {
          overrides: {
            modlistName: selectedModListName(),
            minRamMb: ov.minRamEnabled ? ov.minRamMb : null,
            maxRamMb: ov.maxRamEnabled ? ov.maxRamMb : null,
            customJvmArgs: ov.customArgsEnabled ? ov.customJvmArgs : null,
            profilerEnabled: ov.profilerEnabled ? ov.profilerActive : null,
            wrapperCommand: ov.wrapperEnabled ? ov.wrapperCommand : null,
          },
        });
      }
      setSettingsModalOpen(false);
      await loadShellSnapshot(selectedModListName() || null);
    } catch (err) {
      pushUiError({ title: "Settings not saved", message: "The settings could not be written to disk.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleSaveIncompatibilities = async () => {
    const rules = draftIncompatibilities().map(rule => ({
      winnerId: rule.winnerId,
      loserId: rule.loserId,
    }));

    if (!isTauri() || !selectedModListName()) {
      setSavedIncompatibilities(rules.map(rule => ({ ...rule })));
      setIncompatibilityModalOpen(false);
      return;
    }

    try {
      await invoke("save_incompatibilities_command", {
        input: {
          modlistName: selectedModListName(),
          rules,
        },
      });
      setSavedIncompatibilities(rules.map(rule => ({ ...rule })));
      setIncompatibilityModalOpen(false);
      await loadEditorSnapshot(selectedModListName());
    } catch (err) {
      pushUiError({ title: "Could not save incompatibilities", message: "The incompatibility rules could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleSavePresentation = async () => {
    if (!selectedModListName()) return;

    if (!isTauri()) {
      setInstancePresentationOpen(false);
      return;
    }

    try {
      const presentation = instancePresentation();
      await invoke("save_modlist_presentation_command", {
        input: {
          modlistName: selectedModListName(),
          iconLabel: presentation.iconLabel,
          iconAccent: presentation.iconAccent,
          notes: presentation.notes,
        },
      });
      setInstancePresentationOpen(false);
      await loadModlistPresentation(selectedModListName());
    } catch (err) {
      pushUiError({ title: "Could not save notes", message: "The mod-list notes and icon settings could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleExport = async () => {
    if (!selectedModListName()) return;

    if (!isTauri()) {
      pushUiError({ title: "Desktop app required", message: "Export requires the Cubic Launcher desktop app.", detail: "The native save dialog is only available inside Tauri.", severity: "warning", scope: "launch" });
      return;
    }

    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const destination = await save({
        title: "Export Mod List",
        defaultPath: `${selectedModListName()}.zip`,
        filters: [{ name: "ZIP archive", extensions: ["zip"] }],
      });
      if (!destination) return;

      const options = exportOptions();
      await invoke("export_modlist_command", {
        input: {
          modlistName: selectedModListName(),
          destinationPath: destination,
          rulesJson: options.rulesJson,
          modJars: options.modJars,
          configFiles: options.configFiles,
          resourcePacks: options.resourcePacks,
          otherFiles: options.otherFiles,
        },
      });
      setExportModalOpen(false);
    } catch (err) {
      pushUiError({ title: "Export failed", message: "The mod-list archive could not be created.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleLaunch = async () => {
    if (launchState() === "resolving" || launchState() === "running") return;
    if (!selectedModList()) {
      pushUiError({ title: "No mod list selected", message: "Select a mod list from the sidebar before launching.", detail: "", severity: "warning", scope: "launch" });
      return;
    }
    resetLaunchUiState();

    if (isTauri()) {
      try {
        await invoke("start_launch_command", {
          request: {
            modlistName: selectedModListName(),
            minecraftVersion: selectedMcVersion(),
            modLoader: selectedModLoader(),
          },
        });
        return; // backend drives the progress via events
      } catch (err) {
        pushUiError({ title: "Launch failed", message: "The backend could not start the launch.", detail: String(err), severity: "error", scope: "launch" });
        return;
      }
    }

    // Browser simulation (fallback when not in Tauri)
    setLaunchState("resolving");
    for (const stage of LAUNCH_STAGES) {
      setLaunchProgress(stage.progress);
      setLaunchStageLabel(stage.label);
      setLaunchStageDetail(stage.detail);
      setLaunchLogs(c => [...c, `[Resolver] ${stage.label} — ${stage.detail}`]);
      await wait(500);
    }
    setLaunchState("ready");
  };

  // ── Render ─────────────────────────────────────────────────────────────────
  return (
    <div class="flex flex-col h-screen w-screen overflow-hidden text-textMain bg-bgDark font-sans">
      {/* Header - Fixed at top */}
      <Header />

      {/* Main content area - Sidebar + Content + BottomBar */}
      <div class="flex flex-1 overflow-hidden relative">
        <Sidebar
          onSelectModList={handleSelectModList}
          onSwitchAccount={handleSwitchAccount}
        />

        {/* Content + BottomBar column */}
        <div class="flex flex-col flex-1 min-w-0">
          <ModListEditor
            onAddMod={() => setAddModModalOpen(true)}
            onDeleteSelected={handleDeleteSelected}
            onReorder={orderedIds => void handleReorderRules(orderedIds)}
          />
          <LaunchPanel onLaunch={handleLaunch} onSwitchAccount={handleSwitchAccount} />
        </div>
      </div>

      {/* Modals */}
      <AddModDialog onAddModrinth={handleAddModrinth} onUploadLocal={handleUploadLocal} />
      <CreateModlistModal onCreate={handleCreateModlist} />
      <SettingsModal onSave={handleSaveSettings} />
      <AccountsModal onSwitchAccount={handleSwitchAccount} />
      <FunctionalGroupModal />
      <InstancePresentationModal onSave={handleSavePresentation} />
      <RenameRuleModal onRename={handleRenameRule} />
      <IncompatibilitiesModal onSave={handleSaveIncompatibilities} />
      <AlternativesPanel onSave={handleSaveAlternativeOrder} onAddAlternative={handleAddAlternative} onRemoveAlternative={handleRemoveAlternative} />
      <ErrorCenter />
      <ExportModal onExport={handleExport} />
    </div>
  );
}
