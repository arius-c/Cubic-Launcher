import { createEffect, createSignal, onMount, onCleanup, batch } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { appendDebugTrace } from "./lib/debugTrace";
import { logger } from "./lib/logger";
import { updateAltsDeep } from "./lib/dragUtils";

import {
  modListCards, setModListCards, selectedModListName, setSelectedModListName,
  modRowsState, setModRowsState, setAccounts, setActiveAccountId,
  setLaunchState, setLaunchProgress, setLaunchStageLabel, setLaunchStageDetail,
  setLaunchLogs,
  globalSettings, modlistOverrides, selectedMcVersion, setSelectedMcVersion, selectedModLoader, setSelectedModLoader,
  createModlistName, createModlistDescription, setCreateModlistModalOpen,
  setCreateModlistBusy, createModlistBusy,
  renameRuleDraft, renameRuleTargetId, setRenameRuleModalOpen,
  selectedIds, setSelectedIds, expandedRows, setExpandedRows, activeAccount, setAddModModalOpen,
  setSettingsModalOpen, localJarRuleName, setLocalJarRuleName,
  setGlobalSettings, setModlistOverrides,
  pushUiError, resetLaunchUiState,
  aestheticGroups, functionalGroups, setAestheticGroups, setFunctionalGroups,
  savedIncompatibilities, setSavedIncompatibilities,
  savedLinks, setSavedLinks,
  draftIncompatibilities, setIncompatibilityModalOpen,
  instancePresentation, setInstancePresentation,
  exportOptions, setExportModalOpen, setInstancePresentationOpen,
  alternativesPanelParent, setAlternativesPanelParentId,
  setAdvancedPanelModId, rowMap, setOnToggleEnabled,
  launchState, selectedModList, setAppLoading, setModIcons, modIcons,
  minecraftVersions, setMinecraftVersions, setMcWithSnapshots,
  LAUNCH_STAGES, wait,
  versionRules, setVersionRules, customConfigs, setCustomConfigs,
  setResolvedModIds,
} from "./store";
import type { AestheticGroup, FunctionalGroup, LinkRule, ModRow, VersionRule, CustomConfig } from "./lib/types";

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
      cur.enabled    === nextRow.enabled &&
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

function serializeGroupsLayout(fGroups: FunctionalGroup[]) {
  return JSON.stringify({
    tags: fGroups.map(group => ({
      id: group.id,
      name: group.name,
      tone: group.tone,
      modIds: group.modIds,
    })),
  });
}

/** Serialize aesthetic group structure for change-detection in save_rule_groups_command. */
function serializeRuleGroups(aGroups: AestheticGroup[]): string {
  return JSON.stringify(
    aGroups.map(g => ({ id: g.id, name: g.name, collapsed: g.collapsed, rowIds: g.blockIds, scopeRowId: g.scopeRowId ?? null }))
  );
}

function serializeExpandedRows(ids: string[]): string {
  return JSON.stringify([...ids].sort());
}

function serializeLinks(links: Array<{ fromId: string; toId: string }>): string {
  return JSON.stringify([...links].sort((a, b) => `${a.fromId}|${a.toId}`.localeCompare(`${b.fromId}|${b.toId}`)));
}

function serializeIncompatibilities(rules: Array<{ winnerId: string; loserId: string }>) {
  return JSON.stringify([...rules].sort((a, b) => `${a.winnerId}|${a.loserId}`.localeCompare(`${b.winnerId}|${b.loserId}`)));
}

import { Header }            from "./components/Header";
import { Sidebar }           from "./components/Sidebar";
import { ModListEditor }     from "./components/ModListEditor";
import { LaunchPanel }       from "./components/LaunchPanel";
import { AddModDialog }      from "./components/AddModDialog";
import {
  CreateModlistModal, SettingsModal, AccountsModal,
  FunctionalGroupModal, LinkModal, LinksOverviewModal, InstancePresentationModal, RenameRuleModal, IncompatibilitiesModal,
  AlternativesPanel, ErrorCenter, ExportModal,
} from "./components/Modals";
import { AdvancedModPanel } from "./components/AdvancedModPanel";
import { DebugPanel } from "./components/DebugPanel";

// ─────────────────────────────────────────────────────────────────────────────

const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

// ── Resolution helper ────────────────────────────────────────────────────────

let resolutionSeq = 0;

async function runResolution(modlistName?: string, mcVersion?: string, modLoader?: string) {
  const name = modlistName ?? selectedModListName();
  const ver  = mcVersion  ?? selectedMcVersion();
  const ldr  = modLoader  ?? selectedModLoader();
  if (!isTauri() || !name) return;
  const seq = ++resolutionSeq;
  try {
    const activeIds: string[] = await invoke("resolve_modlist_command", {
      modlistName: name,
      mcVersion: ver,
      modLoader: ldr,
    });
    if (seq !== resolutionSeq) return; // stale result, discard
    setResolvedModIds(new Set(activeIds));
  } catch (e) {
    if (seq !== resolutionSeq) return; // stale error, discard
    logger.warn("App", "resolution failed", { error: e });
    setResolvedModIds(null);
  }
}

// ── Per-modlist version/loader cache (prevents bleed when switching modlists) ──
const modlistVersionLoaderCache = new Map<string, { version: string; loader: string }>();

// ── Backend data loaders ──────────────────────────────────────────────────────

async function loadShellSnapshot(preferredName?: string | null) {
  try {
    const snap: any = await invoke("load_shell_snapshot_command", {
      selectedModlistName: preferredName ?? null,
    });

    // Always update mod list cards — preserve any cached icon data already loaded
    const existing = modListCards();
    const cards = (snap.modlists ?? []).map((m: any) => {
      const prev = existing.find(c => c.name === m.name);
      return {
        name: m.name,
        status: "Ready" as const,
        accent: "from-primary/30 via-primary/10 to-transparent",
        description: m.description || (m.rule_count > 0 ? `${m.rule_count} rules` : "Empty mod list"),
        iconImage: prev?.iconImage,
        iconLabel: prev?.iconLabel,
        iconAccent: prev?.iconAccent,
        displayName: prev?.displayName,
        mcVersion: m.minecraft_version ?? undefined,
        modLoader: m.mod_loader ?? undefined,
      };
    });
    setModListCards(cards);

    // Load icon images for any cards that don't have them yet
    const needLoad = cards.filter((c: { iconLabel?: string }) => !c.iconLabel).map((c: { name: string }) => c.name);
    if (needLoad.length > 0) void loadAllCardIcons(needLoad);

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
        return [{ id: a.microsoft_id, gamertag: gtag, email: a.microsoft_id, avatarUrl: a.avatar_url, status: a.status, lastMode: a.last_mode }, ...rest];
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
      const resolvedVersion = ov.minecraft_version ?? minecraftVersions()[0] ?? "1.21.1";
      const resolvedLoader = ov.mod_loader ?? "Fabric";
      setSelectedMcVersion(resolvedVersion);
      setSelectedModLoader(resolvedLoader);
      if (preferredName) modlistVersionLoaderCache.set(preferredName, { version: resolvedVersion, loader: resolvedLoader });
    }

    return snap;
  } catch (err) {
    logger.error("App", "loadShellSnapshot failed", err);
    pushUiError({ title: "Could not load launcher state", message: "The backend returned an error on startup.", detail: String(err), severity: "error", scope: "launch" });
    return null;
  }
}

function mapEditorRow(row: any, ruleIndex: number, path: string): ModRow {
  const nameId = normalizeIdentifier(row.name);
  const id = `rule-${ruleIndex}${path}-${nameId}`;
  const alternatives = (row.alternatives ?? []).map((alt: any, altIdx: number) =>
    mapEditorRow(alt, ruleIndex, `${path}-alternative-${altIdx + 1}`)
  );
  return {
    id,
    name: row.name as string,
    modrinth_id: row.source === "modrinth" ? (row.modId as string) : undefined,
    primaryModId: row.modId as string,
    kind: row.source as "modrinth" | "local",
    enabled: row.enabled !== false,
    area: "Rule",
    note: alternatives.length > 0 ? `Primary option with ${alternatives.length + 1} mods.` : "Primary option with 1 mod.",
    tags: [],
    alternatives,
    links: (row.requires ?? []) as string[],
    altGroups: [],
  };
}

function mapEditorRowsToModRows(rows: any[]): ModRow[] {
  return rows.map((row: any, index: number) => mapEditorRow(row, index, ""));
}

async function loadEditorSnapshot(modlistName: string, resetGroups = false) {
  if (!modlistName) return null;
  try {
    const snap: any = await invoke("load_modlist_editor_command", { selectedModlistName: modlistName });
    const mappedRows: ModRow[] = mapEditorRowsToModRows(snap.rows ?? []);

    // Smart merge: preserve unchanged row references so SolidJS <For> doesn't
    // destroy and recreate every component on every reload.
    setModRowsState(cur => smartSetModRows(cur, mappedRows));

    // Immediately re-apply cached Modrinth display names so rows don't briefly
    // (or permanently) show raw slugs after a reload.
    patchModNames();

    // NOTE: Aesthetic groups are loaded exclusively by loadModlistGroups (from
    // the groups file).  Setting them here would race with the save effect and
    // persist an empty array before loadModlistGroups can read the real data.

    // Reconstruct savedLinks from each row's links field (primary mod IDs → row IDs).
    // Build a primaryModId → rowId map covering all rows at every depth.
    const primaryModToRowId = new Map<string, string>();
    const collectPrimaryMods = (rows: any[]) => {
      for (const row of rows) {
        if (row.primaryModId) primaryModToRowId.set(row.primaryModId as string, row.id as string);
        if (row.alternatives?.length) collectPrimaryMods(row.alternatives);
      }
    };
    collectPrimaryMods(mappedRows);

    const restoredLinks: LinkRule[] = [];
    const collectLinks = (rows: any[]) => {
      for (const row of rows) {
        for (const primaryModId of (row.links ?? []) as string[]) {
          const toId = primaryModToRowId.get(primaryModId);
          if (toId) restoredLinks.push({ fromId: row.id as string, toId });
        }
        if (row.alternatives?.length) collectLinks(row.alternatives);
      }
    };
    collectLinks(mappedRows);
    setSavedLinks(restoredLinks);

    // Wipe functional groups when switching to a different list; they're loaded by loadModlistGroups.
    if (resetGroups) {
      setFunctionalGroups([]);
    }
    // Convert mod IDs → row IDs so the UI can match them against rowMap entries.
    const incompat: Array<{ winnerId: string; loserId: string }> = snap.incompatibilities ?? [];
    setSavedIncompatibilities(incompat.map(r => ({
      winnerId: primaryModToRowId.get(r.winnerId) ?? r.winnerId,
      loserId: primaryModToRowId.get(r.loserId) ?? r.loserId,
    })));

    // Extract versionRules and customConfigs from the flat backend rows.
    // Use row IDs (not mod IDs) so they match the IDs used by the AdvancedModPanel.
    const extractedVR: VersionRule[] = [];
    const extractedCC: CustomConfig[] = [];
    const collectAdvanced = (rows: any[]) => {
      for (const row of rows) {
        const backendModId = row.modId as string;
        const rowId = primaryModToRowId.get(backendModId) ?? backendModId;
        for (const [i, vr] of (row.versionRules ?? []).entries()) {
          extractedVR.push({ id: `vr-${backendModId}-${i}`, modId: rowId, kind: vr.kind as 'exclude' | 'only', mcVersions: vr.mcVersions ?? [], loader: vr.loader ?? 'any' });
        }
        for (const [i, cc] of (row.customConfigs ?? []).entries()) {
          extractedCC.push({ id: `cc-${backendModId}-${i}`, modId: rowId, mcVersions: cc.mcVersions ?? [], loader: cc.loader ?? 'any', targetPath: cc.targetPath ?? '', files: cc.files ?? [] });
        }
        if (row.alternatives?.length) collectAdvanced(row.alternatives);
      }
    };
    collectAdvanced(snap.rows ?? []);
    setVersionRules(extractedVR);
    setCustomConfigs(extractedCC);

    // Pre-populate Modrinth availability cache in the background, then resolve.
    // The backfill ensures the DB has results for all mods before resolution reads them.
    void invoke("backfill_availability_command", {
      modlistName,
      mcVersion: selectedMcVersion(),
      modLoader: selectedModLoader(),
    }).catch(() => {}).then(() => runResolution(modlistName));

    return snap;
  } catch (err) {
    logger.error("App", "loadEditorSnapshot failed", err);
    pushUiError({ title: "Could not load mod list", message: `Failed to read rules for '${modlistName}'.`, detail: String(err), severity: "error", scope: "launch" });
    setModRowsState([]);
    return null;
  }
}

async function loadAllCardIcons(names: string[]) {
  if (!isTauri() || names.length === 0) { logger.warn("App", "loadAllCardIcons skipped — no backend"); return; }
  await Promise.all(names.map(async name => {
    try {
      const p: any = await invoke("load_modlist_presentation_command", { modlistName: name });
      const iconImage: string = p.iconImage ?? "";
      const iconLabel: string = p.iconLabel ?? "";
      const iconAccent: string = p.iconAccent ?? "";
      const displayName: string = p.displayName ?? name;
      setModListCards(cards => cards.map(c => c.name === name ? { ...c, iconImage, iconLabel, iconAccent, displayName } : c));
    } catch (_) { /* silently skip */ }
  }));
}

async function loadModlistPresentation(modlistName: string) {
  if (!modlistName) {
    setInstancePresentation({ displayName: "", iconLabel: "ML", iconAccent: "", notes: "", iconImage: "" });
    return null;
  }

  if (!isTauri()) {
    logger.warn("App", "loadModlistPresentation skipped — no backend");
    setInstancePresentation({ displayName: modlistName, iconLabel: buildDefaultIconLabel(modlistName), iconAccent: "", notes: "", iconImage: "" });
    return null;
  }

  try {
    const presentation: any = await invoke("load_modlist_presentation_command", { modlistName });
    const iconImage = presentation.iconImage ?? "";
    const iconLabel = presentation.iconLabel ?? buildDefaultIconLabel(modlistName);
    const iconAccent = presentation.iconAccent ?? "";
    const displayName = presentation.displayName ?? modlistName;
    setInstancePresentation({
      displayName,
      iconLabel,
      iconAccent,
      notes: presentation.notes ?? "",
      iconImage,
    });
    setModListCards(cards => cards.map(c => c.name === modlistName ? { ...c, iconImage, iconLabel, iconAccent, displayName } : c));
    return presentation;
  } catch (err) {
    setInstancePresentation({ displayName: modlistName, iconLabel: buildDefaultIconLabel(modlistName), iconAccent: "", notes: "", iconImage: "" });
    pushUiError({ title: "Could not load notes", message: `Saved notes for '${modlistName}' could not be loaded.`, detail: String(err), severity: "error", scope: "launch" });
    return null;
  }
}

async function loadModlistGroups(modlistName: string, rows: ModRow[]) {
  if (!modlistName) {
    setFunctionalGroups([]);
    return null;
  }

  if (!isTauri()) {
    logger.warn("App", "loadModlistGroups skipped — no backend");
    setFunctionalGroups([]);
    return null;
  }

  try {
    const layout: any = await invoke("load_modlist_groups_command", { modlistName });
    const availableIds = new Set<string>();
    // Build modId → rowId map for translating persisted mod IDs back to current row IDs.
    const modIdToRowId = new Map<string, string>();
    const collect = (items: ModRow[]) => {
      for (const row of items) {
        availableIds.add(row.id);
        if (row.primaryModId) modIdToRowId.set(row.primaryModId, row.id);
        if (row.alternatives?.length) collect(row.alternatives);
      }
    };
    collect(rows);

    // Resolve a persisted ID (could be a mod_id or an old-format row ID) to a current row ID.
    const resolveId = (id: string): string | null => {
      if (availableIds.has(id)) return id;               // already a valid row ID
      const mapped = modIdToRowId.get(id);                // try as mod_id
      return mapped && availableIds.has(mapped) ? mapped : null;
    };

    // Read tag definitions with their mod_ids directly from the layout file.
    const tagDefs = layout.tags ?? layout.functionalGroups ?? [];
    const nextFunctionalGroups = tagDefs.map((group: any) => ({
      id: group.id,
      name: group.name,
      tone: group.tone,
      modIds: (group.modIds ?? group.mod_ids ?? []).map((id: string) => resolveId(id)).filter((id: string | null): id is string => id !== null),
    }));
    setFunctionalGroups(nextFunctionalGroups.filter((g: FunctionalGroup) => g.modIds.length > 0));

    // Read aesthetic groups from the layout file.
    const persistedAesthetic: any[] = layout.aestheticGroups ?? [];
    setAestheticGroups(persistedAesthetic.map((g: any) => ({
      id: g.id as string,
      name: g.name as string,
      collapsed: g.collapsed as boolean,
      blockIds: (g.blockIds ?? []).map((id: string) => resolveId(id)).filter((id: string | null): id is string => id !== null),
      scopeRowId: (() => { const s = g.scopeRowId as string | null; return s ? (resolveId(s) ?? null) : null; })(),
    })));

    // Restore expanded alt panels from persisted state.
    const persistedExpanded: string[] = (layout.collapsedAlts ?? []).map((id: string) => resolveId(id)).filter((id: string | null): id is string => id !== null);
    setExpandedRows(persistedExpanded);

    return layout;
  } catch (err) {
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

/** Cache of slug → display name, persisted across reloads so names survive editor refreshes. */
const modNameCache = new Map<string, string>();

async function fetchModMetadata(rows: ModRow[]) {
  const ids = [...collectModrinthIds(rows)];
  if (ids.length === 0) return;

  // Always patch names from cache first (instant, no network).
  patchModNames();

  // Fetch metadata for any IDs we haven't seen yet.
  const missing = ids.filter(id => !modIcons().has(id));
  if (missing.length === 0) return;

  try {
    const param = encodeURIComponent(JSON.stringify(missing));
    const res = await fetch(
      `https://api.modrinth.com/v2/projects?ids=${param}`,
      { headers: { "User-Agent": "CubicLauncher/0.1.0" } }
    );
    if (!res.ok) return;
    const projects: Array<{ id: string; slug: string; title: string; icon_url?: string | null }> = await res.json();
    const updatedIcons = new Map(modIcons());
    for (const p of projects) {
      // Store under both project ID and slug so lookups work regardless of
      // which identifier the mod was originally added with.
      if (p.icon_url) {
        if (p.id) updatedIcons.set(p.id, p.icon_url);
        if (p.slug) updatedIcons.set(p.slug, p.icon_url);
      }
      if (p.title) {
        if (p.id) modNameCache.set(p.id, p.title);
        if (p.slug) modNameCache.set(p.slug, p.title);
      }
    }
    setModIcons(updatedIcons);
    patchModNames();
  } catch {
    // Metadata is best-effort — swallow errors silently
  }
}

/** Apply cached display names to any row still showing a slug/id as its name. */
function patchModNames() {
  if (modNameCache.size === 0) return;
  setModRowsState(cur => cur.map(function patch(r: ModRow): ModRow {
    const realName = r.modrinth_id ? modNameCache.get(r.modrinth_id) : undefined;
    // Patch if the name still looks like a raw identifier (matches the modrinth_id).
    const needsPatch = realName && (r.name === r.modrinth_id || r.name === r.primaryModId);
    const nameFixed = needsPatch ? { ...r, name: realName } : r;
    if (nameFixed.alternatives?.length) {
      const patchedAlts = nameFixed.alternatives.map(patch);
      return { ...nameFixed, alternatives: patchedAlts };
    }
    return nameFixed;
  }));
}

// ─────────────────────────────────────────────────────────────────────────────

export default function App() {
  const [groupLayoutReady, setGroupLayoutReady] = createSignal(false);
  const [lastSavedGroupLayout, setLastSavedGroupLayout] = createSignal("");
  const [lastSavedRuleMetaGroups, setLastSavedRuleMetaGroups] = createSignal("");
  const [lastSavedCollapsedAlts, setLastSavedCollapsedAlts] = createSignal("");
  const [incompatReady, setIncompatReady] = createSignal(false);
  const [lastSavedIncompat, setLastSavedIncompat] = createSignal("");
  const [linksReady, setLinksReady] = createSignal(false);
  const [lastSavedLinks, setLastSavedLinks] = createSignal("");

  // Helper: convert a row ID to a stable mod ID for persistence.
  const rowIdToModId = (id: string): string => rowMap().get(id)?.primaryModId ?? id;

  // Helper: build the full groups command payload (reused by all three group save effects).
  // Persists mod IDs (stable) instead of position-based row IDs so groups survive rule reordering.
  const buildGroupsPayload = (modlistName: string) => ({
    modlistName,
    tags: functionalGroups().map(g => ({ id: g.id, name: g.name, tone: g.tone, modIds: g.modIds.map(rowIdToModId) })),
    aestheticGroups: aestheticGroups().map(g => ({ id: g.id, name: g.name, collapsed: g.collapsed, blockIds: g.blockIds.map(rowIdToModId), scopeRowId: g.scopeRowId ? rowIdToModId(g.scopeRowId) : null })),
    collapsedAlts: [...expandedRows()].map(rowIdToModId),
  });

  // Save functional group (tag) definitions + membership to modlist-editor-groups.json.
  createEffect(() => {
    const modlistName = selectedModListName();
    const ready = groupLayoutReady();
    const serialized = serializeGroupsLayout(functionalGroups());

    if (!ready || !modlistName || !isTauri() || serialized === lastSavedGroupLayout()) return;

    setLastSavedGroupLayout(serialized);
    void invoke("save_modlist_groups_command", { input: buildGroupsPayload(modlistName) }).catch(err => {
      setLastSavedGroupLayout("");
      pushUiError({ title: "Could not save groups", message: "The mod-list group layout could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
    });
  });

  // Save aesthetic group changes to the same modlist-editor-groups.json file.
  createEffect(() => {
    const modlistName = selectedModListName();
    const ready = groupLayoutReady();
    const serialized = serializeRuleGroups(aestheticGroups());

    if (!ready || !modlistName || !isTauri() || serialized === lastSavedRuleMetaGroups()) return;

    setLastSavedRuleMetaGroups(serialized);
    void invoke("save_modlist_groups_command", { input: buildGroupsPayload(modlistName) }).catch(err => {
      setLastSavedRuleMetaGroups("");
      pushUiError({ title: "Could not save groups", message: "The aesthetic group layout could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
    });
  });

  // Save expanded alt panel state to the same modlist-editor-groups.json file.
  createEffect(() => {
    const modlistName = selectedModListName();
    const ready = groupLayoutReady();
    const serialized = serializeExpandedRows(expandedRows());

    if (!ready || !modlistName || !isTauri() || serialized === lastSavedCollapsedAlts()) return;

    setLastSavedCollapsedAlts(serialized);
    void invoke("save_modlist_groups_command", { input: buildGroupsPayload(modlistName) }).catch(err => {
      setLastSavedCollapsedAlts("");
      pushUiError({ title: "Could not save groups", message: "The alt panel state could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
    });
  });

  createEffect(() => {
    const modlistName = selectedModListName();
    const ready = incompatReady();
    const serialized = serializeIncompatibilities(savedIncompatibilities());

    if (!ready || !modlistName || !isTauri() || serialized === lastSavedIncompat()) return;

    setLastSavedIncompat(serialized);
    const rules = savedIncompatibilities().map(r => ({
      winnerId: rowMap().get(r.winnerId)?.primaryModId ?? r.winnerId,
      loserId: rowMap().get(r.loserId)?.primaryModId ?? r.loserId,
    }));
    void invoke("save_incompatibilities_command", {
      input: { modlistName, rules },
    }).then(() => runResolution()).catch(err => {
      setLastSavedIncompat("");
      pushUiError({ title: "Could not save incompatibilities", message: "The incompatibility rules could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
    });
  });

  // Combined effect: save links + versionRules + customConfigs via save_advanced_batch_command.
  // Uses a single batch command that clears all requires/vr/cc then sets new values atomically.
  const [advancedReady, setAdvancedReady] = createSignal(false);
  const [lastSavedAdvanced, setLastSavedAdvanced] = createSignal("");

  createEffect(() => {
    const modlistName = selectedModListName();
    const readyLinks = linksReady();
    const readyAdv = advancedReady();
    const links = savedLinks();
    const vr = versionRules();
    const cc = customConfigs();
    const serialized = JSON.stringify({ links: serializeLinks(links), vr, cc });

    if (!readyLinks || !readyAdv || !modlistName || !isTauri() || serialized === lastSavedAdvanced()) return;

    setLastSavedAdvanced(serialized);
    // Also keep lastSavedLinks in sync so the linksReady gate works correctly on modlist switch.
    setLastSavedLinks(serializeLinks(links));

    // Build requires entries from links (fromModId → [toModId, ...]).
    const requiresByModId = new Map<string, string[]>();
    for (const l of links) {
      const fromModId = rowMap().get(l.fromId)?.primaryModId ?? l.fromId;
      const toModId = rowMap().get(l.toId)?.primaryModId ?? l.toId;
      const arr = requiresByModId.get(fromModId) ?? [];
      arr.push(toModId);
      requiresByModId.set(fromModId, arr);
    }

    const requiresEntries = [...requiresByModId.entries()].map(([modId, requires]) => ({ modId, requires }));

    // Group version rules by mod ID (convert row IDs → mod IDs for the backend).
    const vrByMod = new Map<string, typeof vr>();
    for (const r of vr) {
      const realModId = rowMap().get(r.modId)?.primaryModId ?? r.modId;
      const arr = vrByMod.get(realModId) ?? [];
      arr.push(r);
      vrByMod.set(realModId, arr);
    }
    const versionRulesEntries = [...vrByMod.entries()].map(([modId, rules]) => ({
      modId,
      versionRules: rules.map(r => ({ kind: r.kind, mcVersions: r.mcVersions, loader: r.loader })),
    }));

    // Group custom configs by mod ID (convert row IDs → mod IDs for the backend).
    const ccByMod = new Map<string, typeof cc>();
    for (const c of cc) {
      const realModId = rowMap().get(c.modId)?.primaryModId ?? c.modId;
      const arr = ccByMod.get(realModId) ?? [];
      arr.push(c);
      ccByMod.set(realModId, arr);
    }
    const customConfigsEntries = [...ccByMod.entries()].map(([modId, configs]) => ({
      modId,
      customConfigs: configs.map(c => ({ mcVersions: c.mcVersions, loader: c.loader, targetPath: c.targetPath, files: c.files })),
    }));

    void invoke("save_advanced_batch_command", {
      input: { modlistName, requiresEntries, versionRulesEntries, customConfigsEntries },
    }).then(() => runResolution()).catch(err => {
      setLastSavedAdvanced("");
      pushUiError({ title: "Could not save rule config", message: "Failed to persist advanced config.", detail: String(err), severity: "error", scope: "launch" });
    });
  });

  // ── Startup ────────────────────────────────────────────────────────────────
  onMount(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    const boot = async () => {
      logger.info("App", "boot started");

      // Fetch MC versions first so the fallback in loadShellSnapshot is correct.
      {
        const payload: { releases: string[]; withSnapshots: string[] } = await invoke("fetch_minecraft_versions_command");
        logger.debug("App", "fetched minecraft versions", { releases: payload.releases.length, withSnapshots: payload.withSnapshots.length });
        if (payload.releases.length > 0) setMinecraftVersions(payload.releases);
        if (payload.withSnapshots.length > 0) setMcWithSnapshots(payload.withSnapshots);
      }

      // Load shell + editor from backend (stubs return empty defaults in browser mode)
      const snap = await loadShellSnapshot(null);
      setAppLoading(false);
      logger.debug("App", "shell snapshot loaded", { modlists: (snap?.modlists ?? []).map((m: any) => m.name) });

      // Load editor data + icons for the selected mod list
      const firstList = snap?.modlists?.[0]?.name ?? "";
      if (firstList) {
        const editorSnapshot = await loadEditorSnapshot(firstList);
        await loadModlistPresentation(firstList);
        await loadModlistGroups(firstList, modRowsState());
        setLastSavedGroupLayout(serializeGroupsLayout(functionalGroups()));
        setLastSavedRuleMetaGroups(serializeRuleGroups(aestheticGroups()));
        setLastSavedCollapsedAlts(serializeExpandedRows(expandedRows()));
        setGroupLayoutReady(true);
        setLastSavedIncompat(serializeIncompatibilities(savedIncompatibilities()));
        setIncompatReady(true);
        setLastSavedLinks(serializeLinks(savedLinks()));
        setLinksReady(true);
        setLastSavedAdvanced(JSON.stringify({ links: serializeLinks(savedLinks()), vr: versionRules(), cc: customConfigs() }));
        setAdvancedReady(true);
        logger.debug("App", "boot completed", { firstList, rowCount: editorSnapshot?.rows?.length ?? modRowsState().length });
        void fetchModMetadata(modRowsState());
        void runResolution(firstList, selectedMcVersion(), selectedModLoader());
      } else {
        logger.debug("App", "boot completed — no mod lists found");
      }

      // ── Register Tauri event listeners ──────────────────────────────────
      if (isTauri()) {
        unlisteners.push(await listen<{ state: string; progress: number; stage: string; detail: string }>("launch-progress", (event) => {
          const { state, progress, stage, detail } = event.payload;
          setLaunchState(state as "idle" | "resolving" | "ready" | "running");
          setLaunchProgress(progress);
          setLaunchStageLabel(stage);
          setLaunchStageDetail(detail);
        }));

        unlisteners.push(await listen<{ stream: string; line: string }>("minecraft-log", (event) => {
          setLaunchLogs(cur => [...cur, event.payload.line]);
        }));

        unlisteners.push(await listen<{ title: string; message: string; detail: string }>("launcher-error", (event) => {
          pushUiError({ title: event.payload.title, message: event.payload.message, detail: event.payload.detail, severity: "error", scope: "launch" });
        }));

        unlisteners.push(await listen<{ success: boolean; exitCode: number | null }>("minecraft-exit", (event) => {
          setLaunchState("idle");
          setLaunchProgress(0);
          setLaunchStageLabel("Ready");
          setLaunchStageDetail(event.payload.success ? "Minecraft exited normally." : `Minecraft exited with code ${event.payload.exitCode ?? "unknown"}.`);
        }));
      }

      if (disposed) unlisteners.forEach(u => u());
    };

    void boot();
    onCleanup(() => { disposed = true; unlisteners.forEach(u => u()); });
  });

  // ── Handlers ──────────────────────────────────────────────────────────────

  const handleSelectModList = async (name: string) => {
    logger.info("App", "handleSelectModList started", { name });
    appendDebugTrace("modlist.select", { name });
    setSelectedModListName(name);
    setSelectedIds([]);
    setGroupLayoutReady(false);
    setIncompatReady(false);
    setLinksReady(false);
    setAdvancedReady(false);
    // Immediately show cached presentation data so there's no flicker
    const cached = modListCards().find(c => c.name === name);
    if (cached?.iconLabel !== undefined) {
      setInstancePresentation(cur => ({
        ...cur,
        displayName: cached.displayName ?? cur.displayName,
        iconLabel: cached.iconLabel ?? cur.iconLabel,
        iconAccent: cached.iconAccent ?? cur.iconAccent,
        iconImage: cached.iconImage ?? cur.iconImage,
      }));
    }
    // Restore cached version/loader immediately — prevents bleed from previous modlist
    const cachedVL = modlistVersionLoaderCache.get(name);
    if (cachedVL) { setSelectedMcVersion(cachedVL.version); setSelectedModLoader(cachedVL.loader); }
    if (!isTauri()) { logger.warn("App", "handleSelectModList skipped — no backend"); return; }
    await loadShellSnapshot(name);
    const editorSnapshot = await loadEditorSnapshot(name, true);
    await loadModlistPresentation(name);
    await loadModlistGroups(name, modRowsState());
    setLastSavedGroupLayout(serializeGroupsLayout(functionalGroups()));
    setLastSavedRuleMetaGroups(serializeRuleGroups(aestheticGroups()));
    setLastSavedCollapsedAlts(serializeExpandedRows(expandedRows()));
    setGroupLayoutReady(true);
    setLastSavedIncompat(serializeIncompatibilities(savedIncompatibilities()));
    setIncompatReady(true);
    setLastSavedLinks(serializeLinks(savedLinks()));
    setLinksReady(true);
    setLastSavedAdvanced(JSON.stringify({ links: serializeLinks(savedLinks()), vr: versionRules(), cc: customConfigs() }));
    setAdvancedReady(true);
    void fetchModMetadata(modRowsState());
    void runResolution(name, selectedMcVersion(), selectedModLoader());
  };

  const handleReorderRules = async (orderedIds: string[]) => {
    logger.info("App", "handleReorderRules started", { count: orderedIds.length });
    if (!isTauri() || !selectedModListName()) { logger.warn("App", "handleReorderRules skipped — no backend"); return; }
    try {
      await invoke("reorder_rules_command", {
        input: { modlistName: selectedModListName(), orderedModIds: orderedIds.map(id => rowMap().get(id)?.primaryModId ?? id) },
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
        setSavedLinks(links => links.map(l => ({
          fromId: idRemap.get(l.fromId) ?? l.fromId,
          toId:   idRemap.get(l.toId)   ?? l.toId,
        })).filter(l => validIds.has(l.fromId) && validIds.has(l.toId)));
        setSelectedIds(ids => remapIds(ids, idRemap, validIds));
        setExpandedRows(ids => remapIds(ids, idRemap, validIds));
        setAlternativesPanelParentId(id => (id && idRemap.has(id) ? idRemap.get(id)! : id));
        setAdvancedPanelModId(id => (id && idRemap.has(id) ? idRemap.get(id)! : id));
      });
      void runResolution();
    } catch (err) {
      logger.error("App", "handleReorderRules failed", err);
      pushUiError({ title: "Reorder failed", message: "The new rule order could not be saved.", detail: String(err), severity: "error", scope: "launch" });
      await loadEditorSnapshot(selectedModListName()); // restore original order on failure
    }
  };

  const handleSaveAlternativeOrder = async (parentId: string, orderedAltIds: string[]) => {
    if (!selectedModListName()) return;
    appendDebugTrace("alts.reorder.frontend", { parentId, orderedAltIds, modlistName: selectedModListName() });

    // Optimistic: reorder alternatives in local state immediately, at any depth.
    setModRowsState(cur => reorderAlternativesInRows(cur, parentId, orderedAltIds));

    if (!isTauri()) { logger.warn("App", "handleSaveAlternativeOrder skipped — no backend"); return; }
    try {
      await invoke("save_alternative_order_command", {
        input: { modlistName: selectedModListName(), parentModId: rowMap().get(parentId)?.primaryModId ?? parentId, orderedAltIds: orderedAltIds.map(id => rowMap().get(id)?.primaryModId ?? id) },
      });
      void runResolution();
      appendDebugTrace("alts.reorder.frontend", { parentId, status: "saved" });
    } catch (err) {
      appendDebugTrace("alts.reorder.frontend", { parentId, status: "error", detail: String(err) });
      pushUiError({ title: "Could not save alternative order", message: "The fallback order could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
      await loadEditorSnapshot(selectedModListName()); // restore on failure
    }
  };

  const handleAddAlternative = async (parentId: string, altRowId: string) => {
    if (!isTauri()) {
      const parentRow = rowMap().get(parentId);
      const altRow = rowMap().get(altRowId);
      if (!parentRow || !altRow) return;

      // Remove altRow from wherever it currently lives (top-level or nested)
      const withoutAlt = modRowsState()
        .filter(r => r.id !== altRowId)
        .map(r => ({
          ...r,
          alternatives: (r.alternatives ?? []).filter(a => a.id !== altRowId),
        }));

      // Add a shallow copy as alternative of parent (breaks the shared reference)
      setModRowsState(
        updateAltsDeep(withoutAlt, parentId, [
          ...(parentRow.alternatives ?? []),
          { ...altRow, alternatives: [] },
        ])
      );
      return;
    }
    if (!selectedModListName()) { logger.warn("App", "handleAddAlternative skipped — no modlist"); return; }
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
    const altModId = rowMap().get(altRowId)?.primaryModId ?? altRowId;
    const parentModId = rowMap().get(parentId)?.primaryModId ?? parentId;
    const altSource = rowMap().get(altRowId)?.kind ?? "modrinth";
    try {
      // Move the mod to be an alternative of the parent. The backend extracts
      // the rule from its current position (preserving all data) and re-inserts
      // it as an alternative, so no separate delete is needed.
      if (isNested) {
        await invoke("add_nested_alternative_command", {
          input: { modlistName: selectedModListName(), parentModId, modId: altModId, source: altSource },
        });
      } else {
        await invoke("add_alternative_command", {
          input: { modlistName: selectedModListName(), parentModId, modId: altModId, source: altSource },
        });
      }
      await loadEditorSnapshot(selectedModListName());
      await loadModlistGroups(selectedModListName(), modRowsState());
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
      void fetchModMetadata(modRowsState());
    } catch (err) {
      appendDebugTrace("alts.add.frontend", { status: "error", parentId, altRowId, detail: String(err) });
      pushUiError({ title: "Could not add alternative", message: "The mod could not be added as a fallback.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleRemoveAlternative = async (altRowId: string) => {
    if (!isTauri()) {
      setModRowsState(rows => {
        // Find and extract the alt, then promote it to its parent's level.
        let extracted: ModRow | null = null;
        let parentId: string | null = null;
        const withoutAlt = rows.map(function removeDeep(r: ModRow): ModRow {
          if (!r.alternatives?.length) return r;
          const altIdx = r.alternatives.findIndex(a => a.id === altRowId);
          if (altIdx !== -1) {
            extracted = r.alternatives[altIdx];
            parentId = r.id;
            const filtered = [...r.alternatives.slice(0, altIdx), ...r.alternatives.slice(altIdx + 1)].map(removeDeep);
            return { ...r, alternatives: filtered };
          }
          return { ...r, alternatives: r.alternatives.map(removeDeep) };
        });
        if (!extracted) return withoutAlt;
        // Insert after parent at the same level
        const topIdx = withoutAlt.findIndex(r => r.id === parentId);
        if (topIdx !== -1) {
          return [...withoutAlt.slice(0, topIdx + 1), extracted, ...withoutAlt.slice(topIdx + 1)];
        }
        // Parent is nested — insert as sibling alt (simplified: append to top for now)
        return [...withoutAlt, extracted];
      });
      return;
    }
    if (!selectedModListName()) { logger.warn("App", "handleRemoveAlternative skipped — no modlist"); return; }
    // Save the panel parent's stable mod_id so we can restore it after reload.
    const panelParentModId = alternativesPanelParent()?.primaryModId ?? null;
    appendDebugTrace("alts.remove.frontend", {
      modlistName: selectedModListName(),
      altRowId,
      activePanelModId: panelParentModId,
    });
    try {
      const altModId = rowMap().get(altRowId)?.primaryModId ?? altRowId;
      const findParent = (rows: ModRow[]): ModRow | undefined => {
        for (const r of rows) {
          if (r.alternatives?.some(a => a.id === altRowId)) return r;
          if (r.alternatives?.length) { const found = findParent(r.alternatives); if (found) return found; }
        }
        return undefined;
      };
      const parentRow = findParent(modRowsState());
      const parentModId = parentRow?.primaryModId ?? "";
      await invoke("remove_alternative_command", {
        input: { modlistName: selectedModListName(), parentModId, altModId },
      });
      await loadEditorSnapshot(selectedModListName());
      await loadModlistGroups(selectedModListName(), modRowsState());
      // Restore the alternatives panel using the stable mod_id.
      if (panelParentModId) {
        const nextParent = [...rowMap().values()].find(r => r.primaryModId === panelParentModId);
        if (nextParent) setAlternativesPanelParentId(nextParent.id);
        else setAlternativesPanelParentId(null);
        appendDebugTrace("alts.remove.frontend", {
          status: "reloaded",
          altRowId,
          nextParentId: nextParent?.id ?? null,
          panelParentModId,
        });
      }
    } catch (err) {
      appendDebugTrace("alts.remove.frontend", { status: "error", altRowId, detail: String(err) });
      pushUiError({ title: "Could not remove alternative", message: "The mod could not be detached as a fallback.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleSwitchAccount = async (id: string) => {
    setActiveAccountId(id);
    if (!isTauri()) { logger.warn("App", "handleSwitchAccount skipped — no backend"); return; }
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
    logger.info("App", "handleCreateModlist started", { name });
    setCreateModlistBusy(true);

    if (!isTauri()) {
      logger.warn("App", "handleCreateModlist skipped — no backend");
      // Browser mode: add to the list locally so the UI is usable
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
      await loadModlistGroups(name, modRowsState());
      setLastSavedGroupLayout(serializeGroupsLayout(functionalGroups()));
      setLastSavedRuleMetaGroups(serializeRuleGroups(aestheticGroups()));
      setLastSavedCollapsedAlts(serializeExpandedRows(expandedRows()));
      setGroupLayoutReady(true);
      setLastSavedIncompat(serializeIncompatibilities(savedIncompatibilities()));
      setIncompatReady(true);
      setLastSavedLinks(serializeLinks(savedLinks()));
      setLinksReady(true);
      setLastSavedAdvanced(JSON.stringify({ links: serializeLinks(savedLinks()), vr: versionRules(), cc: customConfigs() }));
      setAdvancedReady(true);
      logger.debug("App", "handleCreateModlist completed", { name });
    } catch (err) {
      logger.error("App", "handleCreateModlist failed", err);
      pushUiError({ title: "Failed to create mod list", message: `'${name}' could not be created.`, detail: String(err), severity: "error", scope: "launch" });
    } finally {
      setCreateModlistBusy(false);
    }
  };

  const handleAddModrinth = async (modId: string, displayName: string) => {
    // Must have a mod list selected before we can add anything
    if (!selectedModListName()) {
      pushUiError({ title: "No mod list selected", message: "Create or select a mod list in the sidebar before adding mods.", detail: "", severity: "warning", scope: "launch" });
      return;
    }
    logger.info("App", "handleAddModrinth started", { modId, displayName });
    // Optimistic insert: append a placeholder row immediately so the list
    // never fully re-renders.  The backend normalises the rule name the same
    // way we compute the ID here, so the reference is stable after the reload.
    const tempId = `rule-${modRowsState().length}-${modId}`;
    const tempRow: ModRow = {
      id: tempId,
      name: displayName,
      modrinth_id: modId,
      kind: "modrinth",
      enabled: true,
      area: "Rule",
      note: "Primary option with 1 mod.",
      tags: [],
      alternatives: [],
    };
    setModRowsState(c => [...c, tempRow]);
    void fetchModMetadata([tempRow]);

    if (!isTauri()) { logger.warn("App", "handleAddModrinth skipped — no backend"); return; }
    try {
      await invoke("add_mod_rule_command", {
        input: { modlistName: selectedModListName(), modId, source: "modrinth", fileName: null },
      });
      // Smart-merge: only the new row changes (or its ID gets corrected); existing rows keep references.
      await loadEditorSnapshot(selectedModListName());
      logger.debug("App", "handleAddModrinth completed", { modId, displayName });
    } catch (err) {
      logger.error("App", "handleAddModrinth failed", err);
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
    logger.info("App", "handleDeleteSelected started", { ids });
    const idSet = new Set(ids);

    // Capture real mod IDs BEFORE the optimistic removal clears them from rowMap.
    const backendModIds = ids.map(id => rowMap().get(id)?.primaryModId ?? id);

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
    if (!isTauri() || !selectedModListName()) { logger.warn("App", "handleDeleteSelected skipped — no backend"); return; }
    try {
      await invoke("delete_rules_command", { input: { modlistName: selectedModListName(), modIds: backendModIds } });
      const currentRows = modRowsState();
      const rebuiltRows = rebuildRowIds(currentRows);
      const idRemap = buildIdRemap(currentRows, rebuiltRows);
      const validIds = collectRowIds(rebuiltRows);
      batch(() => {
        setModRowsState(rebuiltRows);
        setAestheticGroups(groups => remapAestheticGroups(groups, idRemap, validIds));
        setFunctionalGroups(groups => remapFunctionalGroups(groups, idRemap, validIds));
        setAdvancedPanelModId(id => (id && idRemap.has(id) ? idRemap.get(id)! : (id && validIds.has(id) ? id : null)));
      });
      void runResolution();
      logger.debug("App", "handleDeleteSelected completed");
    } catch (err) {
      logger.error("App", "handleDeleteSelected failed", err);
      pushUiError({ title: "Failed to delete rules", message: "The selected rules could not be removed.", detail: String(err), severity: "error", scope: "launch" });
      await loadEditorSnapshot(selectedModListName()); // restore on failure
    }
  };

  const handleRenameRule = async () => {
    const id = renameRuleTargetId();
    const name = renameRuleDraft().trim();
    if (!id || !name) return;
    logger.info("App", "handleRenameRule started", { id, name });
    const nextId = renameRowId(id, name);
    const idRemap = new Map([[id, nextId]]);
    const currentRows = modRowsState().map(r => r.id === id ? { ...r, id: nextId, name } : r);
    const validIds = collectRowIds(currentRows);
    batch(() => {
      setModRowsState(currentRows);
      setAestheticGroups(groups => remapAestheticGroups(groups, idRemap, validIds));
      setFunctionalGroups(groups => remapFunctionalGroups(groups, idRemap, validIds));
      setSelectedIds(ids => remapIds(ids, idRemap, validIds));
      setAdvancedPanelModId(aid => (aid && idRemap.has(aid) ? idRemap.get(aid)! : aid));
    });
    setRenameRuleModalOpen(false);
    if (!isTauri() || !selectedModListName()) { logger.warn("App", "handleRenameRule skipped — no backend"); return; }
    try {
      await invoke("rename_rule_command", { input: { modlistName: selectedModListName(), modId: rowMap().get(id)?.primaryModId ?? id, newModId: name } });
      void runResolution();
      logger.debug("App", "handleRenameRule completed");
    } catch (err) {
      logger.error("App", "handleRenameRule failed", err);
      pushUiError({ title: "Rename failed", message: "The rule name could not be saved.", detail: String(err), severity: "error", scope: "launch" });
      await loadEditorSnapshot(selectedModListName()); // restore on failure
    }
  };

  const handleSaveSettings = async () => {
    logger.info("App", "handleSaveSettings started");
    if (!isTauri()) { logger.warn("App", "handleSaveSettings skipped — no backend"); setSettingsModalOpen(false); return; }
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
            minecraftVersion: selectedMcVersion() || null,
            modLoader: selectedModLoader() || null,
          },
        });
      }
      setSettingsModalOpen(false);
      await loadShellSnapshot(selectedModListName() || null);
      logger.debug("App", "handleSaveSettings completed");
    } catch (err) {
      logger.error("App", "handleSaveSettings failed", err);
      pushUiError({ title: "Settings not saved", message: "The settings could not be written to disk.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const saveVersionLoader = async (modlistName: string, version: string, loader: string) => {
    if (!isTauri() || !modlistName) { logger.warn("App", "saveVersionLoader skipped — no backend"); return; }
    const ov = modlistOverrides();
    await invoke("save_modlist_overrides_command", {
      overrides: {
        modlistName,
        minRamMb: ov.minRamEnabled ? ov.minRamMb : null,
        maxRamMb: ov.maxRamEnabled ? ov.maxRamMb : null,
        customJvmArgs: ov.customArgsEnabled ? ov.customJvmArgs : null,
        profilerEnabled: ov.profilerEnabled ? ov.profilerActive : null,
        wrapperCommand: ov.wrapperEnabled ? ov.wrapperCommand : null,
        minecraftVersion: version || null,
        modLoader: loader || null,
      },
    }).catch(() => {});
  };

  const handleVersionChange = (version: string) => {
    const modlistName = selectedModListName();
    setSelectedMcVersion(version);
    modlistVersionLoaderCache.set(modlistName, { version, loader: selectedModLoader() });
    setModListCards(cards => cards.map(c => c.name === modlistName ? { ...c, mcVersion: version } : c));
    void saveVersionLoader(modlistName, version, selectedModLoader());
    void runResolution(modlistName, version, selectedModLoader());
  };

  const handleLoaderChange = (loader: string) => {
    const modlistName = selectedModListName();
    setSelectedModLoader(loader);
    modlistVersionLoaderCache.set(modlistName, { version: selectedMcVersion(), loader });
    setModListCards(cards => cards.map(c => c.name === modlistName ? { ...c, modLoader: loader } : c));
    void saveVersionLoader(modlistName, selectedMcVersion(), loader);
    void runResolution(modlistName, selectedMcVersion(), loader);
  };

  const handleSaveIncompatibilities = async () => {
    // Keep synthetic IDs for the frontend signals (UI uses them)
    const rules = draftIncompatibilities().map(rule => ({
      winnerId: rule.winnerId,
      loserId: rule.loserId,
    }));
    // Map to real mod IDs for the backend
    const backendRules = rules.map(rule => ({
      winnerId: rowMap().get(rule.winnerId)?.primaryModId ?? rule.winnerId,
      loserId: rowMap().get(rule.loserId)?.primaryModId ?? rule.loserId,
    }));

    if (!isTauri() || !selectedModListName()) {
      setSavedIncompatibilities(rules.map(rule => ({ ...rule })));
      setLastSavedIncompat(serializeIncompatibilities(rules));
      setIncompatibilityModalOpen(false);
      return;
    }

    try {
      await invoke("save_incompatibilities_command", {
        input: {
          modlistName: selectedModListName(),
          rules: backendRules,
        },
      });
      setSavedIncompatibilities(rules.map(rule => ({ ...rule })));
      setLastSavedIncompat(serializeIncompatibilities(rules));
      setIncompatibilityModalOpen(false);
      await loadEditorSnapshot(selectedModListName());
    } catch (err) {
      pushUiError({ title: "Could not save incompatibilities", message: "The incompatibility rules could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleToggleEnabled = async (rowId: string | string[], enabled: boolean) => {
    const ids = Array.isArray(rowId) ? rowId : [rowId];
    const modIds = ids.map(id => rowMap().get(id)?.primaryModId ?? id);
    if (!isTauri() || !selectedModListName()) return;

    // Lock scroll position across all re-renders (optimistic + resolution).
    const scrollEl = document.querySelector<HTMLElement>(".flex-1.p-4");
    const savedScroll = scrollEl?.scrollTop ?? 0;
    let scrollGuardId = 0;
    const guardScroll = () => { if (scrollEl) scrollEl.scrollTop = savedScroll; scrollGuardId = requestAnimationFrame(guardScroll); };
    scrollGuardId = requestAnimationFrame(guardScroll);

    // Optimistic update — toggle all target IDs and their alternatives.
    const idSet = new Set(ids);
    const setEnabledDeep = (r: ModRow, en: boolean): ModRow => {
      const next = r.enabled === en ? r : { ...r, enabled: en };
      if (!next.alternatives?.length) return next;
      const alts = next.alternatives.map(a => setEnabledDeep(a, en));
      return alts === next.alternatives ? next : { ...next, alternatives: alts };
    };
    setModRowsState(cur => cur.map(r =>
      idSet.has(r.id) ? setEnabledDeep(r, enabled) : r
    ));
    try {
      await Promise.all(modIds.map(modId =>
        invoke("toggle_rule_enabled_command", { input: { modlistName: selectedModListName(), modId, enabled } })
      ));
      await runResolution();
    } catch (err) {
      pushUiError({ title: "Toggle failed", message: `Could not ${enabled ? "enable" : "disable"} the mod(s).`, detail: String(err), severity: "error", scope: "launch" });
      await loadEditorSnapshot(selectedModListName());
    } finally {
      cancelAnimationFrame(scrollGuardId);
      if (scrollEl) scrollEl.scrollTop = savedScroll;
    }
  };

  setOnToggleEnabled(() => handleToggleEnabled);

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
          displayName: presentation.displayName || null,
          iconLabel: presentation.iconLabel,
          iconAccent: presentation.iconAccent,
          notes: presentation.notes,
          iconImage: presentation.iconImage || null,
        },
      });
      setInstancePresentationOpen(false);
      await loadModlistPresentation(selectedModListName());
    } catch (err) {
      pushUiError({ title: "Could not save settings", message: "The mod-list settings could not be persisted.", detail: String(err), severity: "error", scope: "launch" });
    }
  };

  const handleDeleteModList = async () => {
    const name = selectedModListName();
    if (!name) return;

    if (!isTauri()) {
      setInstancePresentationOpen(false);
      return;
    }

    try {
      await invoke("delete_modlist_command", { modlistName: name });
      setInstancePresentationOpen(false);
      // Reload shell to refresh mod-list sidebar
      const snap: any = await invoke("load_shell_snapshot_command");
      const cards = (snap.modlists ?? []).map((m: any) => ({
        name: m.name,
        status: "Ready" as const,
        accent: "from-primary/30 via-primary/10 to-transparent",
        description: m.description || (m.rule_count > 0 ? `${m.rule_count} rules` : "Empty mod list"),
      }));
      setModListCards(cards);
      void loadAllCardIcons(cards.map((c: { name: string }) => c.name));
      const next = cards[0]?.name ?? null;
      if (next) {
        setSelectedModListName(next);
        const editorSnapshot = await loadEditorSnapshot(next, true);
        await loadModlistPresentation(next);
        await loadModlistGroups(next, modRowsState());
        setLastSavedGroupLayout(serializeGroupsLayout(functionalGroups()));
        setLastSavedRuleMetaGroups(serializeRuleGroups(aestheticGroups()));
        setLastSavedCollapsedAlts(serializeExpandedRows(expandedRows()));
        setGroupLayoutReady(true);
        setLastSavedIncompat(serializeIncompatibilities(savedIncompatibilities()));
        setIncompatReady(true);
        setLastSavedLinks(serializeLinks(savedLinks()));
        setLinksReady(true);
        setLastSavedAdvanced(JSON.stringify({ links: serializeLinks(savedLinks()), vr: versionRules(), cc: customConfigs() }));
        setAdvancedReady(true);
      } else {
        setSelectedModListName("");
        setInstancePresentation({ displayName: "", iconLabel: "ML", iconAccent: "", notes: "", iconImage: "" });
        setModRowsState([]);
      }
    } catch (err) {
      pushUiError({ title: "Could not delete mod-list", message: "The mod-list could not be deleted.", detail: String(err), severity: "error", scope: "launch" });
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
          dataPacks: options.dataPacks,
          shaders: options.shaders,
          otherFiles: options.otherFiles,
          selectedOtherPaths: options.selectedOtherPaths,
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
            onReorderAlts={(parentId, orderedIds) => void handleSaveAlternativeOrder(parentId, orderedIds)}
          />
          <LaunchPanel onLaunch={handleLaunch} onSwitchAccount={handleSwitchAccount} onVersionChange={handleVersionChange} onLoaderChange={handleLoaderChange} />
        </div>
      </div>

      {/* Modals */}
      <AddModDialog onAddModrinth={handleAddModrinth} onAddContent={async (contentType, id, _name) => {
        if (!selectedModListName()) return;
        try {
          await invoke("add_content_command", { input: { modlistName: selectedModListName(), contentType, id, source: "modrinth" } });
          const { bumpContentVersion } = await import("./components/ModListEditor");
          bumpContentVersion();
        } catch (err) {
          pushUiError({ title: "Failed to add content", message: `Could not add '${id}'.`, detail: String(err), severity: "error", scope: "launch" });
        }
      }} onUploadLocal={handleUploadLocal} onDropJar={async (path) => {
        if (!selectedModListName()) return;
        try {
          await invoke("copy_local_jar_command", {
            input: { sourcePath: path, ruleName: localJarRuleName().trim(), modlistName: selectedModListName() },
          });
          setLocalJarRuleName("");
          setAddModModalOpen(false);
          await loadEditorSnapshot(selectedModListName());
        } catch (err) {
          pushUiError({ title: "Failed to upload JAR", message: "The dropped file could not be added.", detail: String(err), severity: "error", scope: "launch" });
        }
      }} />
      <CreateModlistModal onCreate={handleCreateModlist} />
      <SettingsModal onSave={handleSaveSettings} />
      <AccountsModal onSwitchAccount={handleSwitchAccount} />
      <FunctionalGroupModal />
      <LinkModal />
      <LinksOverviewModal />
      <AdvancedModPanel onDelete={id => { setSelectedIds([id]); void handleDeleteSelected(); }} />
      <InstancePresentationModal onSave={handleSavePresentation} onDelete={handleDeleteModList} />
      <RenameRuleModal onRename={handleRenameRule} />
      <IncompatibilitiesModal onSave={handleSaveIncompatibilities} />
      <AlternativesPanel onSave={handleSaveAlternativeOrder} onAddAlternative={handleAddAlternative} onRemoveAlternative={handleRemoveAlternative} />
      <ErrorCenter />
      <ExportModal onExport={handleExport} />
    </div>
  );
}
