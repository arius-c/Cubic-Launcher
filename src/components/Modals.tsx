import { For, Show, createSignal, createEffect } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { ModRow } from "../lib/types";
import { appendDebugTrace } from "../lib/debugTrace";
import { useDragEngine, type DragItem } from "../lib/dragEngine";
import { GripVerticalIcon } from "./icons";
import {
  /* Settings */
  settingsModalOpen, setSettingsModalOpen, settingsTab, setSettingsTab,
  globalSettings, setGlobalSettings, modlistOverrides, setModlistOverrides,
  /* Accounts */
  accountsModalOpen, setAccountsModalOpen, accounts, setAccounts, activeAccountId, setActiveAccountId,
  toggleActiveAccountConnection,
  /* Presentation */
  instancePresentationOpen, setInstancePresentationOpen,
  instancePresentation, setInstancePresentation,
  /* Create Modlist */
  createModlistModalOpen, setCreateModlistModalOpen,
  createModlistName, setCreateModlistName,
  createModlistDescription, setCreateModlistDescription,
  createModlistBusy,
  /* Functional Groups */
  functionalGroupModalOpen, setFunctionalGroupModalOpen,
  newFunctionalGroupName, setNewFunctionalGroupName,
  functionalGroupTone, setFunctionalGroupTone,
  createFunctionalGroup, selectedModListName, toneToHue, huePreviewColor,
  aestheticGroups, setAestheticGroups, nextAestheticGroupName,
  /* Incompatibilities */
  incompatibilityModalOpen, setIncompatibilityModalOpen,
  focusedIncompatibilityMod, draftIncompatibilities,
  rowMap, priorityParadoxDetected,
  setPairConflictEnabled, setPairWinner,
  incompatibilityFocusId,
  /* Links */
  linkModalOpen, setLinkModalOpen, linkModalModIds,
  draftLinks, setDraftLinks, saveDraftLinks,
  savedLinks, setSavedLinks,
  linksOverviewOpen, setLinksOverviewOpen,
  /* Rename rule */
  renameRuleModalOpen, setRenameRuleModalOpen,
  renameRuleDraft, setRenameRuleDraft,
  /* Alternatives panel */
  alternativesPanelParent, setAlternativesPanelParentId,
  /* Error center */
  errorCenterOpen, setErrorCenterOpen, launcherErrors, dismissError,
  /* Export */
  exportModalOpen, setExportModalOpen, exportOptions, setExportOptions,
} from "../store";
import { XIcon, AlertTriangleIcon } from "./icons";

// ── Sortable alternative row (for drag-and-drop in the alternatives panel) ────
function DraggableAltRow(props: {
  alt: ModRow;
  priority: number;
  removing: boolean;
  selected: boolean;
  isDragging: boolean;
  isDropTarget: boolean;
  translateY: number;
  anyDragging: boolean;
  onToggleSelected: () => void;
  onRemove: () => void;
  onOpenAlts: () => void;
  onStartDrag: (e: PointerEvent) => void;
}) {
  return (
    <div
      data-draggable-id={props.alt.id}
      data-draggable-mid-id={props.alt.id}
      style={{
        transform:  props.anyDragging ? `translateY(${props.isDragging ? 0 : props.translateY}px)` : "none",
        transition: props.anyDragging ? "transform 150ms ease" : "none",
        position:   "relative",
        "z-index":  props.isDragging ? "0" : "1",
      }}
      class={`flex items-center gap-3 rounded-md border px-3 py-2.5 ${props.selected ? "border-primary/40 bg-primary/5" : "border-border bg-background"} ${props.isDragging ? "opacity-0 pointer-events-none" : ""} ${props.isDropTarget ? "ring-1 ring-primary/40" : ""}`}
    >
      {/* Drag handle */}
      <div
        class="shrink-0 cursor-grab touch-none text-muted-foreground/50 hover:text-muted-foreground"
        onPointerDown={props.onStartDrag}
        title="Drag to reorder"
      >
        <GripVerticalIcon class="h-4 w-4" />
      </div>

      <span class="w-5 shrink-0 text-center text-sm font-mono text-muted-foreground">
        {props.priority}
      </span>

      <input
        type="checkbox"
        checked={props.selected}
        onChange={() => props.onToggleSelected()}
        class="h-4 w-4 shrink-0 rounded text-primary"
      />

      <div class="min-w-0 flex-1">
        <p class="truncate text-sm font-medium text-foreground">{props.alt.name}</p>
        <Show when={props.alt.kind === "local"}>
          <p class="text-xs text-warning">Local JAR — verify dependencies</p>
        </Show>
        <Show when={props.alt.modrinth_id}>
          <p class="text-xs text-muted-foreground/60">{props.alt.modrinth_id}</p>
        </Show>
        <Show when={(props.alt.alternatives?.length ?? 0) > 0}>
          <p class="text-xs text-primary/70">{props.alt.alternatives!.length} sub-alt{props.alt.alternatives!.length !== 1 ? "s" : ""}</p>
        </Show>
      </div>

      {/* Open this alternative's own alternatives panel */}
      <button
        onClick={props.onOpenAlts}
        class="flex h-7 items-center gap-1 rounded-md px-2 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        title="Manage alternatives of this mod"
      >
        Alts
      </button>

      {/* Remove from alternatives */}
      <button
        onClick={props.onRemove}
        disabled={props.removing}
        class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-50"
        title="Remove — restore as top-level rule"
      >
        <XIcon class="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

/** Reusable modal wrapper */
function Modal(props: { children: any; onClose: () => void; maxWidth?: string }) {
  return (
    <div
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 px-4 py-6 backdrop-blur-sm"
      onClick={e => { if (e.target === e.currentTarget) props.onClose(); }}
    >
      <div class={`flex max-h-[90vh] w-full flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xl ${props.maxWidth ?? "max-w-2xl"}`}>
        {props.children}
      </div>
    </div>
  );
}

function ModalHeader(props: { title: string; description?: string; onClose: () => void }) {
  return (
    <div class="flex items-center justify-between border-b border-border px-6 py-4">
      <div>
        <h2 class="text-lg font-semibold text-foreground">{props.title}</h2>
        <Show when={props.description}>
          <p class="text-sm text-muted-foreground">{props.description}</p>
        </Show>
      </div>
      <button onClick={props.onClose} class="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground">
        <XIcon class="h-4 w-4" />
      </button>
    </div>
  );
}

// ── Create Modlist ────────────────────────────────────────────────────────────
export function CreateModlistModal(props: { onCreate: () => Promise<void> }) {
  return (
    <Show when={createModlistModalOpen()}>
      <Modal onClose={() => setCreateModlistModalOpen(false)}>
        <ModalHeader title="Create New Mod List" description="Create a version-agnostic mod list that works across all Minecraft versions." onClose={() => setCreateModlistModalOpen(false)} />
        <div class="space-y-4 p-6">
          <div>
            <label class="mb-1.5 block text-sm font-medium text-foreground">Name</label>
            <input
              type="text"
              value={createModlistName()}
              onInput={e => setCreateModlistName(e.currentTarget.value)}
              placeholder="My Awesome Pack"
              class="w-full rounded-md border border-input bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              autofocus
            />
          </div>
          <div>
            <label class="mb-1.5 block text-sm font-medium text-foreground">Description</label>
            <textarea
              rows={3}
              value={createModlistDescription()}
              onInput={e => setCreateModlistDescription(e.currentTarget.value)}
              placeholder="A brief description of your mod list…"
              class="w-full resize-none rounded-md border border-input bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
            />
          </div>
        </div>
        <div class="flex justify-end gap-2 border-t border-border px-6 py-4">
          <button onClick={() => setCreateModlistModalOpen(false)} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Cancel</button>
          <button
            onClick={() => void props.onCreate()}
            disabled={!createModlistName().trim() || createModlistBusy()}
            class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          >
            Create
          </button>
        </div>
      </Modal>
    </Show>
  );
}

// ── Settings ──────────────────────────────────────────────────────────────────
export function SettingsModal(props: { onSave: () => Promise<void> }) {
  const field = (label: string, content: any) => (
    <div class="rounded-md border border-border bg-background p-4">
      <p class="mb-2 text-sm font-medium text-foreground">{label}</p>
      {content}
    </div>
  );

  return (
    <Show when={settingsModalOpen()}>
      <Modal onClose={() => setSettingsModalOpen(false)} maxWidth="max-w-3xl">
        <ModalHeader title="Settings" description="Global defaults and Mod-list overrides" onClose={() => setSettingsModalOpen(false)} />
        <div class="flex flex-1 gap-0 overflow-hidden">
          {/* Tab nav */}
          <div class="w-44 shrink-0 border-r border-border p-3 space-y-1">
            {(["global", "modlist"] as const).map(tab => (
              <button
                onClick={() => setSettingsTab(tab)}
                class={`block w-full rounded-md px-3 py-2 text-left text-sm font-medium transition-colors ${settingsTab() === tab ? "bg-primary/10 text-primary" : "text-muted-foreground hover:bg-muted hover:text-foreground"}`}
              >
                {tab === "global" ? "Global Settings" : "Mod-list Overrides"}
              </button>
            ))}
          </div>

          {/* Tab content */}
          <div class="flex-1 overflow-y-auto p-6 space-y-4">
            <Show when={settingsTab() === "global"} fallback={
              /* Modlist overrides */
              <div class="space-y-4">
                <p class="text-sm text-muted-foreground">Checked values override the global defaults for <strong class="text-foreground">{selectedModListName()}</strong>.</p>
                {[
                  { key: "minRam", label: "Min RAM (MB)", enabled: "minRamEnabled", value: "minRamMb" },
                  { key: "maxRam", label: "Max RAM (MB)", enabled: "maxRamEnabled", value: "maxRamMb" },
                ].map(({ label, enabled, value }) => field(label,
                  <div class="flex gap-3">
                    <input type="checkbox" checked={(modlistOverrides() as any)[enabled]} onChange={e => setModlistOverrides(c => ({ ...c, [enabled]: e.currentTarget.checked }))} class="mt-1 h-4 w-4 rounded text-primary" />
                    <input type="number" value={(modlistOverrides() as any)[value]} disabled={!(modlistOverrides() as any)[enabled]} onInput={e => setModlistOverrides(c => ({ ...c, [value]: Number(e.currentTarget.value) }))} class="flex-1 rounded-md border border-input bg-input px-3 py-1.5 text-sm text-foreground disabled:opacity-40 focus:outline-none focus:ring-1 focus:ring-ring" />
                  </div>
                ))}
                {field("Custom JVM Args",
                  <div class="flex gap-3">
                    <input type="checkbox" checked={modlistOverrides().customArgsEnabled} onChange={e => setModlistOverrides(c => ({ ...c, customArgsEnabled: e.currentTarget.checked }))} class="mt-1 h-4 w-4 rounded text-primary" />
                    <textarea rows={3} value={modlistOverrides().customJvmArgs} disabled={!modlistOverrides().customArgsEnabled} onInput={e => setModlistOverrides(c => ({ ...c, customJvmArgs: e.currentTarget.value }))} class="flex-1 resize-none rounded-md border border-input bg-input px-3 py-1.5 text-sm text-foreground disabled:opacity-40 focus:outline-none" />
                  </div>
                )}
              </div>
            }>
              {/* Global settings */}
              <div class="space-y-4">
                <div class="grid grid-cols-2 gap-4">
                  {field("Min RAM (MB)", <input type="number" value={globalSettings().minRamMb} onInput={e => setGlobalSettings(c => ({ ...c, minRamMb: Number(e.currentTarget.value) }))} class="w-full rounded-md border border-input bg-input px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />)}
                  {field("Max RAM (MB)", <input type="number" value={globalSettings().maxRamMb} onInput={e => setGlobalSettings(c => ({ ...c, maxRamMb: Number(e.currentTarget.value) }))} class="w-full rounded-md border border-input bg-input px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />)}
                </div>
                {field("Custom JVM Args", <textarea rows={3} value={globalSettings().customJvmArgs} onInput={e => setGlobalSettings(c => ({ ...c, customJvmArgs: e.currentTarget.value }))} class="w-full resize-none rounded-md border border-input bg-input px-3 py-1.5 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring" />)}
                {field("Java Path Override", <input type="text" value={globalSettings().javaPathOverride} onInput={e => setGlobalSettings(c => ({ ...c, javaPathOverride: e.currentTarget.value }))} placeholder="Optional explicit Java binary path" class="w-full rounded-md border border-input bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring" />)}
                {field("Wrapper Command (Linux)", <input type="text" value={globalSettings().wrapperCommand} onInput={e => setGlobalSettings(c => ({ ...c, wrapperCommand: e.currentTarget.value }))} placeholder="gamemoderun mangohud" class="w-full rounded-md border border-input bg-input px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring" />)}
                <label class="flex items-center gap-3 text-sm">
                  <input type="checkbox" checked={globalSettings().profilerEnabled} onChange={e => setGlobalSettings(c => ({ ...c, profilerEnabled: e.currentTarget.checked }))} class="h-4 w-4 rounded text-primary" />
                  <span class="text-foreground">Enable profiler globally</span>
                </label>
              </div>
            </Show>
          </div>
        </div>
        <div class="flex justify-end gap-2 border-t border-border px-6 py-4">
          <button onClick={() => setSettingsModalOpen(false)} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Cancel</button>
          <button onClick={() => void props.onSave()} class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90">Save Settings</button>
        </div>
      </Modal>
    </Show>
  );
}

// ── Accounts ──────────────────────────────────────────────────────────────────
export function AccountsModal(props: { onSwitchAccount: (id: string) => Promise<void> }) {
  const [loggingIn, setLoggingIn] = createSignal(false);
  const [loginError, setLoginError] = createSignal<string | null>(null);
  return (
    <Show when={accountsModalOpen()}>
      <Modal onClose={() => setAccountsModalOpen(false)}>
        <ModalHeader title="Accounts" description="Microsoft login, account switching and offline mode" onClose={() => setAccountsModalOpen(false)} />
        <div class="flex-1 overflow-y-auto p-6 space-y-3">
          {/* Microsoft login notice */}
          <div class="mb-4 rounded-md border border-border bg-muted/30 p-4">
            <p class="mb-2 text-sm font-medium text-foreground">Add Microsoft Account</p>
            <p class="mb-3 text-xs text-muted-foreground">
              The launcher opens your system browser for secure Microsoft login (OAuth 2.0 + PKCE).
              Your credentials are never seen by Cubic Launcher.
            </p>
            <button
              class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
              disabled={loggingIn()}
              onClick={async () => {
                setLoggingIn(true);
                try {
                  const gamertag: string = await invoke("microsoft_login_command");
                  setAccountsModalOpen(false);
                  // Reload accounts
                  const snap: any = await invoke("load_shell_snapshot_command", { preferredModlistName: null });
                  if (snap.active_account) {
                    const a = snap.active_account;
                    const gtag = a.xbox_gamertag?.trim() || a.microsoft_id;
                    setAccounts(cur => {
                      const rest = cur.filter((x: any) => x.id !== a.microsoft_id);
                      return [{ id: a.microsoft_id, gamertag: gtag, email: a.microsoft_id, avatarUrl: a.avatar_url, status: "online" as const, lastMode: "microsoft" as const }, ...rest];
                    });
                    setActiveAccountId(a.microsoft_id);
                  }
                } catch (err) {
                  setLoginError(String(err));
                } finally {
                  setLoggingIn(false);
                }
              }}
            >
              {loggingIn() ? "Logging in…" : "Login with Microsoft"}
            </button>
            <Show when={loginError()}>
              <p class="mt-2 text-xs text-destructive break-all">{loginError()}</p>
            </Show>
          </div>
          <p class="text-xs text-muted-foreground">Saved accounts — click to switch</p>
          <For each={accounts()}>
            {acc => (
              <div class={`rounded-md border p-4 transition-colors ${acc.id === activeAccountId() ? "border-primary/40 bg-primary/10" : "border-border hover:bg-muted/30"}`}>
                <div class="flex items-start justify-between">
                  <button
                    onClick={() => void props.onSwitchAccount(acc.id)}
                    class="flex items-start gap-3 text-left flex-1 min-w-0"
                  >
                    <Show when={acc.avatarUrl} fallback={
                      <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-primary/20 text-sm font-semibold text-primary">
                        {acc.gamertag.slice(0, 2).toUpperCase()}
                      </div>
                    }>
                      <img src={acc.avatarUrl} alt="" class="h-10 w-10 shrink-0 rounded-md" loading="lazy" />
                    </Show>
                    <div class="min-w-0">
                      <p class="font-medium text-foreground truncate">{acc.gamertag}</p>
                      <p class="text-xs text-muted-foreground truncate">{acc.email}</p>
                    </div>
                  </button>
                  <div class="flex flex-col items-end gap-1 shrink-0 ml-2">
                    <span class={`rounded-full px-2 py-0.5 text-[10px] ${acc.status === "online" ? "bg-success/15 text-success" : "bg-warning/15 text-warning"}`}>
                      {acc.status}
                    </span>
                    <Show when={acc.id === activeAccountId()}>
                      <span class="rounded-full bg-primary/15 px-2 py-0.5 text-[10px] text-primary">Active</span>
                    </Show>
                    <button
                      onClick={async (e) => {
                        e.stopPropagation();
                        try {
                          await invoke("delete_account_command", { microsoftId: acc.id });
                          setAccounts(cur => cur.filter(a => a.id !== acc.id));
                          if (activeAccountId() === acc.id) setActiveAccountId("");
                        } catch (err) {
                          setLoginError(String(err));
                        }
                      }}
                      class="rounded-full px-2 py-0.5 text-[10px] text-destructive hover:bg-destructive/15 transition-colors"
                    >
                      Remove
                    </button>
                  </div>
                </div>
              </div>
            )}
          </For>
        </div>
        <div class="flex justify-end border-t border-border px-6 py-4">
          <button onClick={toggleActiveAccountConnection} class="mr-auto rounded-md bg-secondary px-3 py-1.5 text-sm text-secondary-foreground hover:bg-secondary/80">Toggle Online/Offline</button>
          <button onClick={() => setAccountsModalOpen(false)} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Close</button>
        </div>
      </Modal>
    </Show>
  );
}

// ── Settings (formerly Icon & Notes) ──────────────────────────────────────────
const isTauriEnv = () => "__TAURI_INTERNALS__" in window;

export function InstancePresentationModal(props: { onSave: () => Promise<void>; onDelete: () => Promise<void> }) {
  // Local draft — initialised from global state whenever the modal opens
  const [draft, setDraft] = createSignal({ ...instancePresentation() });
  const [saving, setSaving] = createSignal(false);
  const [confirmDelete, setConfirmDelete] = createSignal(false);
  const [deleting, setDeleting] = createSignal(false);

  // Reset draft each time the modal opens
  createEffect(() => {
    if (instancePresentationOpen()) {
      setDraft({ ...instancePresentation() });
      setConfirmDelete(false);
    }
  });

  const close = () => {
    setInstancePresentationOpen(false);
    setConfirmDelete(false);
  };

  const handleSave = async () => {
    if (saving()) return;
    setSaving(true);
    try {
      // Commit draft to global state, then persist
      setInstancePresentation(draft());
      await props.onSave();
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (deleting()) return;
    setDeleting(true);
    try {
      await props.onDelete();
    } finally {
      setDeleting(false);
      setConfirmDelete(false);
    }
  };

  const pickIcon = async () => {
    if (!isTauriEnv()) return;
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const { invoke: inv } = await import("@tauri-apps/api/core");
      const selected = await open({
        multiple: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
      });
      if (typeof selected === "string") {
        const dataUrl: string = await inv("read_image_as_data_url_command", { path: selected });
        setDraft(cur => ({ ...cur, iconImage: dataUrl }));
      }
    } catch (_) {
      // user cancelled or no dialog support
    }
  };

  return (
    <Show when={instancePresentationOpen()}>
      <Modal onClose={close}>
        <ModalHeader title="Settings" description="Customize the mod-list card identity and save private notes for this pack." onClose={close} />
        <div class="grid gap-6 p-6 md:grid-cols-[220px,1fr]">
          <div class="rounded-lg border border-border bg-background p-4">
            <p class="mb-3 text-xs font-medium uppercase tracking-wider text-muted-foreground">Preview</p>
            <div class="flex flex-col items-center rounded-lg border border-border bg-card px-4 py-6 text-center">
              <button
                onClick={() => void pickIcon()}
                class="relative flex h-20 w-20 items-center justify-center rounded-2xl bg-primary/15 text-2xl font-semibold text-primary shadow-inner overflow-hidden hover:ring-2 hover:ring-primary transition-all"
                title="Click to set a custom icon image"
              >
                <Show when={draft().iconImage} fallback={
                  <span>{(draft().iconLabel.trim() || "ML").toUpperCase()}</span>
                }>
                  <img src={draft().iconImage} class="absolute inset-0 h-full w-full object-cover" alt="icon" />
                </Show>
                <span class="absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 hover:opacity-100 transition-opacity text-xs text-white font-medium">Edit</span>
              </button>
              <Show when={draft().iconAccent.trim()}>
                <p class="mt-3 text-xs uppercase tracking-[0.18em] text-primary/80">
                  {draft().iconAccent}
                </p>
              </Show>
              <Show when={draft().notes.trim()}>
                <p class="mt-3 text-xs text-muted-foreground whitespace-pre-wrap break-words text-left w-full line-clamp-4">
                  {draft().notes}
                </p>
              </Show>
            </div>
          </div>

          <div class="space-y-4">
            <div>
              <label class="mb-1.5 block text-sm font-medium text-foreground">Name</label>
              <input
                type="text"
                value={draft().iconLabel}
                onInput={e => setDraft(cur => ({ ...cur, iconLabel: e.currentTarget.value }))}
                placeholder="My Mod List"
                class="w-full rounded-md border border-input bg-input px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                autofocus
              />
            </div>
            <div>
              <label class="mb-1.5 block text-sm font-medium text-foreground">Created by</label>
              <input
                type="text"
                value={draft().iconAccent}
                onInput={e => setDraft(cur => ({ ...cur, iconAccent: e.currentTarget.value }))}
                placeholder="Author name"
                class="w-full rounded-md border border-input bg-input px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring"
              />
            </div>
            <div>
              <label class="mb-1.5 block text-sm font-medium text-foreground">Notes</label>
              <textarea
                rows={8}
                value={draft().notes}
                onInput={e => setDraft(cur => ({ ...cur, notes: e.currentTarget.value }))}
                placeholder="Remember shader requirements, server notes, or version-specific caveats..."
                class="w-full resize-none rounded-md border border-input bg-input px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring whitespace-pre-wrap break-words"
              />
            </div>
          </div>
        </div>
        <div class="flex items-center justify-between gap-2 border-t border-border px-6 py-4">
          <Show when={confirmDelete()} fallback={
            <button
              onClick={() => setConfirmDelete(true)}
              class="rounded-md bg-red-900/40 px-4 py-2 text-sm text-red-300 hover:bg-red-900/70 border border-red-700/40 transition-colors"
            >
              Delete Mod-list
            </button>
          }>
            <div class="flex items-center gap-2">
              <span class="text-sm text-red-400">Are you sure? This cannot be undone.</span>
              <button
                onClick={() => void handleDelete()}
                disabled={deleting()}
                class="rounded-md bg-red-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
              >
                {deleting() ? "Deleting..." : "Yes, delete"}
              </button>
              <button
                onClick={() => setConfirmDelete(false)}
                class="rounded-md bg-secondary px-3 py-1.5 text-sm text-secondary-foreground hover:bg-secondary/80"
              >
                Cancel
              </button>
            </div>
          </Show>
          <div class="flex gap-2 ml-auto">
            <button onClick={close} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Cancel</button>
            <button onClick={() => void handleSave()} disabled={saving()} class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50">{saving() ? "Saving..." : "Save Settings"}</button>
          </div>
        </div>
      </Modal>
    </Show>
  );
}

// ── Functional Group ──────────────────────────────────────────────────────────
export function FunctionalGroupModal() {
  const currentHue = () => toneToHue(functionalGroupTone());
  return (
    <Show when={functionalGroupModalOpen()}>
      <Modal onClose={() => setFunctionalGroupModalOpen(false)}>
        <ModalHeader title="Create Tag" description="Tags let you label and filter mods by category." onClose={() => setFunctionalGroupModalOpen(false)} />
        <div class="space-y-4 p-6">
          <div>
            <label class="mb-1.5 block text-sm font-medium text-foreground">Tag Name</label>
            <input type="text" value={newFunctionalGroupName()} onInput={e => setNewFunctionalGroupName(e.currentTarget.value)} placeholder="Performance Core" class="w-full rounded-md border border-input bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring" autofocus />
          </div>
          <div>
            <label class="mb-1.5 block text-sm font-medium text-foreground">Tag Color</label>
            <div class="flex items-center gap-3">
              <div class="h-7 w-7 rounded-full shrink-0 border border-border" style={`background-color: ${huePreviewColor(currentHue())}`} />
              <input
                type="range"
                min="0"
                max="360"
                value={currentHue()}
                onInput={e => setFunctionalGroupTone(e.currentTarget.value)}
                class="flex-1 h-3 appearance-none rounded-full cursor-pointer"
                style="background: linear-gradient(to right, hsl(0,70%,55%), hsl(30,70%,55%), hsl(60,70%,55%), hsl(90,70%,55%), hsl(120,70%,55%), hsl(150,70%,55%), hsl(180,70%,55%), hsl(210,70%,55%), hsl(240,70%,55%), hsl(270,70%,55%), hsl(300,70%,55%), hsl(330,70%,55%), hsl(360,70%,55%));"
              />
            </div>
          </div>
        </div>
        <div class="flex justify-end gap-2 border-t border-border px-6 py-4">
          <button onClick={() => setFunctionalGroupModalOpen(false)} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Cancel</button>
          <button onClick={createFunctionalGroup} disabled={!newFunctionalGroupName().trim()} class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50">Create</button>
        </div>
      </Modal>
    </Show>
  );
}

// ── Link Modal ────────────────────────────────────────────────────────────────
export function LinkModal() {
  const pairs = () => {
    const ids = linkModalModIds();
    const result: Array<[string, string]> = [];
    for (let i = 0; i < ids.length; i++) {
      for (let j = i + 1; j < ids.length; j++) {
        result.push([ids[i], ids[j]]);
      }
    }
    return result;
  };

  const hasLink = (from: string, to: string) =>
    draftLinks().some(l => l.fromId === from && l.toId === to);

  const setDirection = (a: string, b: string, dir: 'a-to-b' | 'mutual' | 'b-to-a' | 'none') => {
    setDraftLinks(cur => {
      const without = cur.filter(l => !(
        (l.fromId === a && l.toId === b) || (l.fromId === b && l.toId === a)
      ));
      if (dir === 'none') return without;
      if (dir === 'a-to-b') return [...without, { fromId: a, toId: b }];
      if (dir === 'b-to-a') return [...without, { fromId: b, toId: a }];
      return [...without, { fromId: a, toId: b }, { fromId: b, toId: a }];
    });
  };

  const currentDir = (a: string, b: string): 'a-to-b' | 'mutual' | 'b-to-a' | 'none' => {
    const ab = hasLink(a, b);
    const ba = hasLink(b, a);
    if (ab && ba) return 'mutual';
    if (ab) return 'a-to-b';
    if (ba) return 'b-to-a';
    return 'none';
  };

  const toggleDir = (a: string, b: string, target: 'a-to-b' | 'mutual' | 'b-to-a') => {
    const cur = currentDir(a, b);
    setDirection(a, b, cur === target ? 'none' : target);
  };

  const dirBtnClass = (active: boolean) =>
    active
      ? "bg-primary/20 text-primary ring-1 ring-primary/30"
      : "text-muted-foreground hover:bg-muted";

  return (
    <Show when={linkModalOpen()}>
      <Modal onClose={() => setLinkModalOpen(false)}>
        <ModalHeader title="Link Mods" description="Define dependency relationships between selected mods." onClose={() => setLinkModalOpen(false)} />
        <div class="flex-1 overflow-y-auto p-6 space-y-3">
          <For each={pairs()}>
            {([a, b]) => {
              const nameA = () => rowMap().get(a)?.name ?? a;
              const nameB = () => rowMap().get(b)?.name ?? b;
              return (
                <div class="flex items-center gap-3 rounded-md border border-border bg-background p-3">
                  <span class="min-w-0 flex-1 truncate text-sm font-medium text-foreground text-right">{nameA()}</span>
                  <div class="flex shrink-0 items-center gap-1">
                    <button
                      onClick={() => toggleDir(a, b, 'a-to-b')}
                      class={`rounded-md px-2 py-1 text-xs font-medium transition-colors ${dirBtnClass(currentDir(a, b) === 'a-to-b')}`}
                      title={`${nameA()} requires ${nameB()}`}
                    >
                      &rarr;
                    </button>
                    <button
                      onClick={() => toggleDir(a, b, 'mutual')}
                      class={`rounded-md px-2 py-1 text-xs font-medium transition-colors ${dirBtnClass(currentDir(a, b) === 'mutual')}`}
                      title={`${nameA()} and ${nameB()} require each other`}
                    >
                      &harr;
                    </button>
                    <button
                      onClick={() => toggleDir(a, b, 'b-to-a')}
                      class={`rounded-md px-2 py-1 text-xs font-medium transition-colors ${dirBtnClass(currentDir(a, b) === 'b-to-a')}`}
                      title={`${nameB()} requires ${nameA()}`}
                    >
                      &larr;
                    </button>
                  </div>
                  <span class="min-w-0 flex-1 truncate text-sm font-medium text-foreground">{nameB()}</span>
                </div>
              );
            }}
          </For>
        </div>
        <div class="flex justify-end gap-2 border-t border-border px-6 py-4">
          <button onClick={() => setLinkModalOpen(false)} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Cancel</button>
          <button onClick={saveDraftLinks} class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90">Save</button>
        </div>
      </Modal>
    </Show>
  );
}

// ── Links Overview ────────────────────────────────────────────────────────────
export function LinksOverviewModal() {
  /** Deduplicated pairs derived from savedLinks. */
  const pairs = () => {
    const links = savedLinks();
    const seen = new Set<string>();
    const result: Array<{ a: string; b: string }> = [];
    for (const link of links) {
      const key = [link.fromId, link.toId].sort().join("|");
      if (!seen.has(key)) {
        seen.add(key);
        const [a, b] = [link.fromId, link.toId].sort();
        result.push({ a, b });
      }
    }
    return result;
  };

  const nameOf = (id: string) => rowMap().get(id)?.name ?? id;

  const hasLink = (from: string, to: string) =>
    savedLinks().some(l => l.fromId === from && l.toId === to);

  const currentDir = (a: string, b: string): 'a-to-b' | 'mutual' | 'b-to-a' | 'none' => {
    const ab = hasLink(a, b);
    const ba = hasLink(b, a);
    if (ab && ba) return 'mutual';
    if (ab) return 'a-to-b';
    if (ba) return 'b-to-a';
    return 'none';
  };

  const setDirection = (a: string, b: string, dir: 'a-to-b' | 'mutual' | 'b-to-a' | 'none') => {
    setSavedLinks(cur => {
      const without = cur.filter(l =>
        !((l.fromId === a && l.toId === b) || (l.fromId === b && l.toId === a))
      );
      if (dir === 'none') return without;
      if (dir === 'a-to-b') return [...without, { fromId: a, toId: b }];
      if (dir === 'b-to-a') return [...without, { fromId: b, toId: a }];
      return [...without, { fromId: a, toId: b }, { fromId: b, toId: a }];
    });
  };

  const toggleDir = (a: string, b: string, target: 'a-to-b' | 'mutual' | 'b-to-a') => {
    const cur = currentDir(a, b);
    setDirection(a, b, cur === target ? 'none' : target);
  };

  const dirBtnClass = (active: boolean) =>
    active
      ? "bg-primary/20 text-primary ring-1 ring-primary/30"
      : "text-muted-foreground hover:bg-muted";

  return (
    <Show when={linksOverviewOpen()}>
      <Modal onClose={() => setLinksOverviewOpen(false)} maxWidth="max-w-lg">
        <ModalHeader
          title="Link Relations"
          description="All dependency links defined across your mod list."
          onClose={() => setLinksOverviewOpen(false)}
        />
        <div class="flex-1 overflow-y-auto p-4 space-y-2 max-h-96">
          <Show
            when={pairs().length > 0}
            fallback={<p class="text-center text-sm text-muted-foreground py-6">No links defined.</p>}
          >
            <For each={pairs()}>
              {({ a, b }) => (
                <div class="flex items-center gap-3 rounded-md border border-border bg-background p-3">
                  <span class="min-w-0 flex-1 truncate text-sm font-medium text-foreground text-right">{nameOf(a)}</span>
                  <div class="flex shrink-0 items-center gap-1">
                    <button
                      onClick={() => toggleDir(a, b, 'a-to-b')}
                      class={`rounded-md px-2 py-1 text-xs font-medium transition-colors ${dirBtnClass(currentDir(a, b) === 'a-to-b')}`}
                      title={`${nameOf(a)} requires ${nameOf(b)}`}
                    >
                      &rarr;
                    </button>
                    <button
                      onClick={() => toggleDir(a, b, 'mutual')}
                      class={`rounded-md px-2 py-1 text-xs font-medium transition-colors ${dirBtnClass(currentDir(a, b) === 'mutual')}`}
                      title={`${nameOf(a)} and ${nameOf(b)} require each other`}
                    >
                      &harr;
                    </button>
                    <button
                      onClick={() => toggleDir(a, b, 'b-to-a')}
                      class={`rounded-md px-2 py-1 text-xs font-medium transition-colors ${dirBtnClass(currentDir(a, b) === 'b-to-a')}`}
                      title={`${nameOf(b)} requires ${nameOf(a)}`}
                    >
                      &larr;
                    </button>
                  </div>
                  <span class="min-w-0 flex-1 truncate text-sm font-medium text-foreground">{nameOf(b)}</span>
                  <button
                    onClick={() => setDirection(a, b, 'none')}
                    class="shrink-0 flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                    title="Remove link"
                  >
                    <XIcon class="h-3.5 w-3.5" />
                  </button>
                </div>
              )}
            </For>
          </Show>
        </div>
        <div class="flex justify-end border-t border-border px-4 py-3">
          <button
            onClick={() => setLinksOverviewOpen(false)}
            class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80"
          >
            Close
          </button>
        </div>
      </Modal>
    </Show>
  );
}

// ── Rename Rule ───────────────────────────────────────────────────────────────
export function RenameRuleModal(props: { onRename: () => Promise<void> }) {
  return (
    <Show when={renameRuleModalOpen()}>
      <Modal onClose={() => setRenameRuleModalOpen(false)}>
        <ModalHeader title="Rename Rule" onClose={() => setRenameRuleModalOpen(false)} />
        <div class="p-6">
          <input type="text" value={renameRuleDraft()} onInput={e => setRenameRuleDraft(e.currentTarget.value)} onKeyDown={e => e.key === "Enter" && void props.onRename()} class="w-full rounded-md border border-input bg-input px-3 py-2 text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-ring" autofocus />
        </div>
        <div class="flex justify-end gap-2 border-t border-border px-6 py-4">
          <button onClick={() => setRenameRuleModalOpen(false)} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Cancel</button>
          <button onClick={() => void props.onRename()} disabled={!renameRuleDraft().trim()} class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50">Rename</button>
        </div>
      </Modal>
    </Show>
  );
}

// ── Incompatibilities ─────────────────────────────────────────────────────────
interface IncompatibilitiesModalProps {
  onSave: () => Promise<void>;
}

export function IncompatibilitiesModal(props: IncompatibilitiesModalProps) {
  const [saving, setSaving] = createSignal(false);

  const handleSave = async () => {
    if (saving() || priorityParadoxDetected()) return;
    setSaving(true);
    try {
      await props.onSave();
    } finally {
      setSaving(false);
    }
  };

  // Build the set of IDs that cannot be selected as incompatibility partners.
  // Rule: no incompatibility between any two mods that share an ancestor-descendant
  // relationship at any depth — self, all ancestors, and all descendants are excluded.
  const incompatibilityExcluded = () => {
    const focusId = incompatibilityFocusId();
    const excluded = new Set<string>();
    if (!focusId) return excluded;
    excluded.add(focusId);

    // Build parent map: child ID → direct parent ID (from all rows at all depths).
    const parentMap = new Map<string, string>();
    for (const row of rowMap().values()) {
      for (const alt of (row.alternatives ?? [])) {
        parentMap.set(alt.id, row.id);
      }
    }

    // Walk up the chain to collect all ancestors.
    let cur = focusId;
    while (parentMap.has(cur)) {
      const pid = parentMap.get(cur)!;
      excluded.add(pid);
      cur = pid;
    }

    // Recursively collect all descendants.
    const collectDesc = (row: ModRow) => {
      for (const alt of (row.alternatives ?? [])) {
        excluded.add(alt.id);
        collectDesc(alt);
      }
    };
    const focusedMod = rowMap().get(focusId);
    if (focusedMod) collectDesc(focusedMod);

    return excluded;
  };

  return (
    <Show when={incompatibilityModalOpen()}>
      <Modal onClose={() => setIncompatibilityModalOpen(false)}>
        <ModalHeader title="Incompatibility Rules" description={`Define which mods conflict with "${focusedIncompatibilityMod()?.name ?? "selected"}"`} onClose={() => setIncompatibilityModalOpen(false)} />
        <div class="flex-1 overflow-y-auto p-6 space-y-3">
          <Show when={priorityParadoxDetected()}>
            <div class="flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">
              <AlertTriangleIcon class="h-4 w-4 shrink-0" />
              Attention, you have created a priority paradox! Remove the conflicting rule to continue.
            </div>
          </Show>
          <For each={[...rowMap().values()].filter(r => !incompatibilityExcluded().has(r.id))}>
            {other => {
              const pair = () => draftIncompatibilities().find(r =>
                (r.winnerId === incompatibilityFocusId() && r.loserId === other.id) ||
                (r.winnerId === other.id && r.loserId === incompatibilityFocusId())
              );
              const enabled = () => !!pair();
              const focusWins = () => pair()?.winnerId === incompatibilityFocusId();

              return (
                <div class={`rounded-md border p-3 transition-colors ${enabled() ? "border-border bg-background" : "border-border/50 bg-background/50"}`}>
                  <div class="flex items-center gap-3">
                    <input
                      type="checkbox"
                      checked={enabled()}
                      onChange={e => setPairConflictEnabled(incompatibilityFocusId()!, other.id, e.currentTarget.checked)}
                      class="h-4 w-4 shrink-0 rounded text-primary"
                    />
                    <Show
                      when={enabled()}
                      fallback={
                        <span class="text-sm text-muted-foreground">{other.name}</span>
                      }
                    >
                      <div class="flex flex-1 flex-wrap items-center gap-2">
                        {/* Focused mod badge */}
                        <button
                          onClick={() => setPairWinner(incompatibilityFocusId()!, other.id, incompatibilityFocusId()!)}
                          title={focusWins() ? "Currently wins — click to make it lose" : "Currently loses — click to make it win"}
                          class={`rounded-md px-2.5 py-0.5 text-sm font-medium transition-colors ${
                            focusWins()
                              ? "bg-green-500/15 text-green-500 ring-1 ring-green-500/30"
                              : "bg-red-500/15 text-red-500 ring-1 ring-red-500/30"
                          }`}
                        >
                          {focusedIncompatibilityMod()?.name}
                        </button>

                        <span class="text-xs text-muted-foreground">vs</span>

                        {/* Other mod badge */}
                        <button
                          onClick={() => setPairWinner(incompatibilityFocusId()!, other.id, other.id)}
                          title={!focusWins() ? "Currently wins — click to make it lose" : "Currently loses — click to make it win"}
                          class={`rounded-md px-2.5 py-0.5 text-sm font-medium transition-colors ${
                            !focusWins()
                              ? "bg-green-500/15 text-green-500 ring-1 ring-green-500/30"
                              : "bg-red-500/15 text-red-500 ring-1 ring-red-500/30"
                          }`}
                        >
                          {other.name}
                        </button>
                      </div>
                    </Show>
                  </div>
                </div>
              );
            }}
          </For>
        </div>
        <div class="flex justify-end gap-2 border-t border-border px-6 py-4">
          <button onClick={() => setIncompatibilityModalOpen(false)} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Cancel</button>
          <button onClick={() => void handleSave()} disabled={priorityParadoxDetected() || saving()} class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50">{saving() ? "Saving..." : "Save Rules"}</button>
        </div>
      </Modal>
    </Show>
  );
}

// ── Alternatives Panel ────────────────────────────────────────────────────────
interface AlternativesPanelProps {
  onSave: (parentId: string, orderedAltIds: string[]) => Promise<void>;
  onAddAlternative: (parentId: string, altRowId: string) => Promise<void>;
  onRemoveAlternative: (altRowId: string) => Promise<void>;
}

export function AlternativesPanel(props: AlternativesPanelProps) {
  const [ordered, setOrdered]     = createSignal<ModRow[]>([]);
  const [saving, setSaving]       = createSignal(false);
  const [adding, setAdding]       = createSignal(false);
  const [removing, setRemoving]   = createSignal<string | null>(null);
  const [selectedAlternativeIds, setSelectedAlternativeIds] = createSignal<string[]>([]);

  createEffect(() => {
    const parent = alternativesPanelParent();
    setOrdered(parent?.alternatives ? [...parent.alternatives] : []);
    setSelectedAlternativeIds([]);
    appendDebugTrace("alts.panel.frontend", {
      parentId: parent?.id ?? null,
      parentName: parent?.name ?? null,
      orderedIds: (parent?.alternatives ?? []).map(alt => alt.id),
    });
  });

  const scopedGroups = () => {
    const parentId = alternativesPanelParent()?.id ?? null;
    if (!parentId) return [];
    return aestheticGroups()
      .filter(group => group.scopeRowId === parentId)
      .map(group => {
        const blockIdSet = new Set(group.blockIds);
        return {
          ...group,
          blocks: ordered().filter(alt => blockIdSet.has(alt.id)),
        };
      })
      .filter(group => group.blocks.length > 0);
  };

  const ungroupedAlternatives = () => {
    const groupedIds = new Set(scopedGroups().flatMap(group => group.blockIds));
    return ordered().filter(alt => !groupedIds.has(alt.id));
  };

  const toggleAlternativeSelection = (altId: string) => {
    setSelectedAlternativeIds(current => current.includes(altId) ? current.filter(id => id !== altId) : [...current, altId]);
  };

  const handleCreateAlternativeGroup = () => {
    const parent = alternativesPanelParent();
    const selectedIds = selectedAlternativeIds().filter(id => ordered().some(alt => alt.id === id));
    if (!parent || selectedIds.length === 0) return;

    const id = `ag-${Date.now()}`;
    const name = nextAestheticGroupName(parent.id);
    setAestheticGroups(current => {
      const withoutSelected = current.map(group => ({
        ...group,
        blockIds: group.blockIds.filter(blockId => !selectedIds.includes(blockId)),
      }));
      return [...withoutSelected, { id, name, collapsed: false, blockIds: selectedIds, scopeRowId: parent.id }];
    });
    setSelectedAlternativeIds([]);
  };

  let altPanelContainerRef: HTMLDivElement | undefined;

  const altPanelEngine = useDragEngine({
    containerRef: () => altPanelContainerRef,
    getItems: () => ordered().map((r): DragItem => ({ kind: "row", id: r.id })),
    onCommit: (fromId, dropId) => {
      // Resolve the target index from the drop ID
      let toId: string;
      if (dropId.startsWith("before:")) {
        toId = dropId.slice("before:".length);
      } else if (dropId.startsWith("after:")) {
        toId = dropId.slice("after:".length);
      } else {
        return;
      }
      if (fromId === toId) return;

      appendDebugTrace("alts.drag.frontend", {
        phase: "start",
        parentId: alternativesPanelParent()?.id ?? null,
        fromId,
        toId,
        orderedIds: ordered().map(alt => alt.id),
      });

      setOrdered(cur => {
        const arr = [...cur];
        const fromIdx = arr.findIndex(r => r.id === fromId);
        const toIdx   = arr.findIndex(r => r.id === toId);
        if (fromIdx === -1 || toIdx === -1) return cur;
        // For "after:" we insert after toIdx, for "before:" we insert at toIdx
        const [item] = arr.splice(fromIdx, 1);
        const adjustedToIdx = dropId.startsWith("after:")
          ? (toIdx > fromIdx ? toIdx : toIdx + 1)
          : (toIdx > fromIdx ? toIdx - 1 : toIdx);
        arr.splice(Math.max(0, Math.min(adjustedToIdx, arr.length)), 0, item);
        appendDebugTrace("alts.drag.frontend", {
          phase: "end",
          parentId: alternativesPanelParent()?.id ?? null,
          fromId,
          toId,
          orderedIds: arr.map(alt => alt.id),
        });
        return arr;
      });
    },
  });

  const handleSave = async () => {
    const parent = alternativesPanelParent();
    if (!parent || ordered().length === 0) return;
    appendDebugTrace("alts.save.frontend", {
      parentId: parent.id,
      orderedIds: ordered().map(alt => alt.id),
    });
    setSaving(true);
    await props.onSave(parent.id, ordered().map(r => r.id));
    setSaving(false);
    setAlternativesPanelParentId(null);
  };

  const handleAddAlt = async (altRow: ModRow) => {
    const parent = alternativesPanelParent();
    if (!parent) return;
    appendDebugTrace("alts.add.panel.frontend", {
      parentId: parent.id,
      altRowId: altRow.id,
      altRowName: altRow.name,
    });
    setAdding(true);
    await props.onAddAlternative(parent.id, altRow.id);
    setAdding(false);
  };

  const handleRemoveAlt = async (altRow: ModRow) => {
    appendDebugTrace("alts.remove.panel.frontend", {
      parentId: alternativesPanelParent()?.id ?? null,
      altRowId: altRow.id,
      altRowName: altRow.name,
    });
    setRemoving(altRow.id);
    await props.onRemoveAlternative(altRow.id);
    setRemoving(null);
  };

  const availableToAdd = () => {
    const parent = alternativesPanelParent();
    if (!parent) return [];
    const existingAltIds = new Set(ordered().map(r => r.id));

    // Build exclusion set: self + all ancestors + all descendants.
    const excluded = new Set<string>();
    excluded.add(parent.id);

    // Ancestors: walk up via parent map.
    const parentMap = new Map<string, string>();
    for (const row of rowMap().values()) {
      for (const alt of (row.alternatives ?? [])) {
        parentMap.set(alt.id, row.id);
      }
    }
    let cur = parent.id;
    while (parentMap.has(cur)) {
      const pid = parentMap.get(cur)!;
      excluded.add(pid);
      cur = pid;
    }

    // Descendants: all alternatives of parent, recursively.
    const collectDesc = (row: ModRow) => {
      for (const alt of (row.alternatives ?? [])) {
        excluded.add(alt.id);
        collectDesc(alt);
      }
    };
    collectDesc(parent);

    // Return all rows at any depth except excluded and already-added.
    return [...rowMap().values()].filter(r => !existingAltIds.has(r.id) && !excluded.has(r.id));
  };

  return (
    <Show when={alternativesPanelParent()}>
      {parent => (
        <Modal onClose={() => setAlternativesPanelParentId(null)} maxWidth="max-w-lg">
          <ModalHeader
            title={`Fallback Order — ${parent().name}`}
            description="Drag to reorder. The launcher tries options top to bottom."
            onClose={() => setAlternativesPanelParentId(null)}
          />

          {/* Primary — fixed at Priority 1 */}
          <div class="border-b border-border px-6 py-3">
            <p class="mb-1.5 text-xs font-medium uppercase tracking-wider text-muted-foreground">Primary (Priority 1 — fixed)</p>
            <div class="flex items-center gap-3 rounded-md border border-primary/30 bg-primary/5 px-3 py-2">
              <div class="h-4 w-4 shrink-0" /> {/* grip spacer */}
              <span class="w-5 shrink-0 text-center text-sm font-semibold text-primary">1</span>
              <span class="flex-1 text-sm font-medium text-foreground">{parent().name}</span>
              <span class="text-xs text-muted-foreground">Primary</span>
            </div>
          </div>

          {/* Drag-and-drop reorderable alternatives */}
          <div class="flex-1 overflow-y-auto p-6 space-y-2">
            <div class="mb-2 flex items-center justify-between gap-3">
              <p class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                Fallback order — drag to reorder
              </p>
              <button
                onClick={handleCreateAlternativeGroup}
                disabled={selectedAlternativeIds().length === 0}
                class="rounded-md bg-secondary px-2.5 py-1 text-xs font-medium text-secondary-foreground hover:bg-secondary/80 disabled:opacity-50"
              >
                Create Group
              </button>
            </div>
            <Show
              when={ordered().length > 0}
              fallback={
                <div class="rounded-md border border-dashed border-border py-6 text-center">
                  <p class="text-sm text-muted-foreground">No fallbacks yet.</p>
                  <p class="mt-1 text-xs text-muted-foreground/60">Add mods from the list below.</p>
                </div>
              }
            >
              {/* Drag ghost */}
              <Show when={altPanelEngine.draggingId() && altPanelEngine.dragPointer()}>
                {(() => {
                  const alt = () => ordered().find(r => r.id === altPanelEngine.draggingId());
                  return (
                    <div
                      class="pointer-events-none fixed z-50 flex cursor-grabbing items-center gap-3 rounded-md border border-primary/40 bg-card px-3 py-2.5 shadow-2xl ring-1 ring-primary/20"
                      style={{ left: `${altPanelEngine.dragPointer()!.x + 12}px`, top: `${altPanelEngine.dragPointer()!.y - 16}px`, "min-width": "220px" }}
                    >
                      <GripVerticalIcon class="h-4 w-4 shrink-0 text-primary" />
                      <span class="truncate text-sm font-medium text-foreground">{alt()?.name ?? "..."}</span>
                    </div>
                  );
                })()}
              </Show>

              <div class="space-y-3" ref={altPanelContainerRef}>
                <For each={scopedGroups()}>
                  {group => (
                    <div class="rounded-md border border-border bg-muted/20 p-2">
                      <div class="mb-2 flex items-center justify-between px-1">
                        <span class="text-xs font-medium uppercase tracking-wider text-muted-foreground">{group.name}</span>
                        <span class="text-[10px] text-muted-foreground">{group.blocks.length} mods</span>
                      </div>
                      <div class="space-y-1.5">
                        <For each={group.blocks}>
                          {(alt) => (
                            <DraggableAltRow
                              alt={alt}
                              priority={ordered().findIndex(candidate => candidate.id === alt.id) + 2}
                              removing={removing() === alt.id}
                              selected={selectedAlternativeIds().includes(alt.id)}
                              isDragging={altPanelEngine.draggingId() === alt.id}
                              isDropTarget={!!(altPanelEngine.hoveredDropId()?.endsWith(alt.id))}
                              translateY={altPanelEngine.previewTranslates().get(alt.id) ?? 0}
                              anyDragging={altPanelEngine.anyDragging()}
                              onToggleSelected={() => toggleAlternativeSelection(alt.id)}
                              onRemove={() => void handleRemoveAlt(alt)}
                              onOpenAlts={() => {
                                setAlternativesPanelParentId(alt.id);
                              }}
                              onStartDrag={(e) => altPanelEngine.startDrag(alt.id, "row", e)}
                            />
                          )}
                        </For>
                      </div>
                    </div>
                  )}
                </For>

                <For each={ungroupedAlternatives()}>
                  {(alt) => (
                    <DraggableAltRow
                      alt={alt}
                      priority={ordered().findIndex(candidate => candidate.id === alt.id) + 2}
                      removing={removing() === alt.id}
                      selected={selectedAlternativeIds().includes(alt.id)}
                      isDragging={altPanelEngine.draggingId() === alt.id}
                      isDropTarget={!!(altPanelEngine.hoveredDropId()?.endsWith(alt.id))}
                      translateY={altPanelEngine.previewTranslates().get(alt.id) ?? 0}
                      anyDragging={altPanelEngine.anyDragging()}
                      onToggleSelected={() => toggleAlternativeSelection(alt.id)}
                      onRemove={() => void handleRemoveAlt(alt)}
                      onOpenAlts={() => {
                        setAlternativesPanelParentId(alt.id);
                      }}
                      onStartDrag={(e) => altPanelEngine.startDrag(alt.id, "row", e)}
                    />
                  )}
                </For>
              </div>
            </Show>

            {/* Add from list */}
            <Show when={availableToAdd().length > 0}>
              <div class="mt-4 border-t border-border pt-4">
                <p class="mb-2 text-xs font-medium uppercase tracking-wider text-muted-foreground">
                  Add fallback from your mod list
                </p>
                <div class="space-y-1.5">
                  <For each={availableToAdd()}>
                    {(row) => (
                      <div class="flex items-center gap-3 rounded-md border border-border bg-muted/20 px-3 py-2">
                        <span class="flex-1 truncate text-sm text-foreground">{row.name}</span>
                        <Show when={row.kind === "local"}>
                          <span class="text-[10px] text-warning">Local</span>
                        </Show>
                        <button
                          onClick={() => void handleAddAlt(row)}
                          disabled={adding()}
                          class="rounded-md bg-secondary px-2.5 py-1 text-xs font-medium text-secondary-foreground transition-colors hover:bg-secondary/80 disabled:opacity-50"
                        >
                          Add
                        </button>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>
          </div>

          <div class="flex justify-end gap-2 border-t border-border px-6 py-4">
            <button
              onClick={() => setAlternativesPanelParentId(null)}
              class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80"
            >
              Close
            </button>
            <Show when={ordered().length > 0}>
              <button
                onClick={() => void handleSave()}
                disabled={saving()}
                class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-60"
              >
                {saving() ? "Saving…" : "Save Order"}
              </button>
            </Show>
          </div>
        </Modal>
      )}
    </Show>
  );
}

// ── Error Center ──────────────────────────────────────────────────────────────
export function ErrorCenter() {
  return (
    <Show when={errorCenterOpen()}>
      <Modal onClose={() => setErrorCenterOpen(false)}>
        <ModalHeader title="Error Center" description="All launcher warnings and errors" onClose={() => setErrorCenterOpen(false)} />
        <div class="flex-1 overflow-y-auto p-6 space-y-3">
          <Show when={launcherErrors().length === 0}>
            <p class="text-center text-sm text-muted-foreground py-8">No errors or warnings.</p>
          </Show>
          <For each={launcherErrors()}>
            {err => (
              <div class={`rounded-md border p-4 ${err.severity === "error" ? "border-destructive/40 bg-destructive/10" : "border-warning/40 bg-warning/10"}`}>
                <div class="flex items-start justify-between gap-3">
                  <div class="flex items-start gap-2">
                    <AlertTriangleIcon class={`mt-0.5 h-4 w-4 shrink-0 ${err.severity === "error" ? "text-destructive" : "text-warning"}`} />
                    <div>
                      <p class={`font-medium text-sm ${err.severity === "error" ? "text-destructive" : "text-warning"}`}>{err.title}</p>
                      <p class="mt-1 text-sm text-foreground">{err.message}</p>
                      <p class="mt-1 text-xs text-muted-foreground">{err.detail}</p>
                    </div>
                  </div>
                  <button onClick={() => dismissError(err.id)} class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground">
                    <XIcon class="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            )}
          </For>
        </div>
        <div class="flex justify-end border-t border-border px-6 py-4">
          <button onClick={() => setErrorCenterOpen(false)} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Close</button>
        </div>
      </Modal>
    </Show>
  );
}

// ── Export Modal ──────────────────────────────────────────────────────────────
export function ExportModal(props: { onExport: () => Promise<void> }) {
  const [saving, setSaving] = createSignal(false);

  const handleExport = async () => {
    if (saving()) return;
    setSaving(true);
    try {
      await props.onExport();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Show when={exportModalOpen()}>
      <Modal onClose={() => setExportModalOpen(false)}>
        <ModalHeader title="Export Mod List" description="Choose what to include in the .zip archive" onClose={() => setExportModalOpen(false)} />
        <div class="p-6 space-y-3">
          {([ ["rulesJson","Mod-list definition (rules.json)"], ["modJars","Mod JAR files from cache"], ["configFiles","Config files from cache"], ["resourcePacks","Resource packs"], ["otherFiles","Other files"] ] as const).map(([key, label]) => (
            <label class="flex items-center gap-3 text-sm">
              <input type="checkbox" checked={(exportOptions() as any)[key]} onChange={e => setExportOptions(o => ({ ...o, [key]: e.currentTarget.checked }))} class="h-4 w-4 rounded text-primary" />
              <span class="text-foreground">{label}</span>
            </label>
          ))}
          <p class="pt-2 text-xs text-muted-foreground">
            {exportOptions().rulesJson && !exportOptions().modJars
              ? "Rules-only export is tiny — the recipient's Cubic Launcher will download dependencies automatically."
              : "Expanded export bundles more content for portable sharing."}
          </p>
        </div>
        <div class="flex justify-end gap-2 border-t border-border px-6 py-4">
          <button onClick={() => setExportModalOpen(false)} class="rounded-md bg-secondary px-4 py-2 text-sm text-secondary-foreground hover:bg-secondary/80">Cancel</button>
          <button onClick={() => void handleExport()} disabled={saving() || !exportOptions().rulesJson} class="rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50">{saving() ? "Exporting..." : "Export"}</button>
        </div>
      </Modal>
    </Show>
  );
}
