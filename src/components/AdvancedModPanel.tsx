import { For, Show, createSignal } from "solid-js";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  advancedPanelMod, advancedPanelModId, setAdvancedPanelModId,
  versionRules, addVersionRule, removeVersionRule, updateVersionRule,
  customConfigs, addCustomConfig, removeCustomConfig, updateCustomConfig,
  functionalGroups, functionalGroupsByBlockId, addModToFunctionalGroup, removeFunctionalGroupMember,
  createFunctionalGroupForMod, functionalGroupTagClass,
  savedLinks, setSavedLinks,
  linksByModId, rowMap,
  parentIdByChildId,
} from "../store";
import { MOD_LOADERS } from "../lib/types";
import { minecraftVersions } from "../store";
import { MaterialIcon, XIcon } from "./icons";

const isTauri = () => "__TAURI_INTERNALS__" in window;

// ── helpers ───────────────────────────────────────────────────────────────────
const ALL_LOADERS = ["any", ...MOD_LOADERS];

function SectionHeader(props: { title: string }) {
  return (
    <div class="px-5 py-2 bg-muted/30 border-b border-border">
      <h3 class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{props.title}</h3>
    </div>
  );
}

// ── Main component ─────────────────────────────────────────────────────────────
export function AdvancedModPanel() {
  const row   = () => advancedPanelMod();
  const modId = () => advancedPanelModId()!;

  // ── Version Rule add form ──────────────────────────────────────────────────
  const [addingRule, setAddingRule]         = createSignal(false);
  const [draftKind, setDraftKind]           = createSignal<'exclude' | 'only'>('exclude');
  const [draftVersions, setDraftVersions]   = createSignal<string[]>([]);
  const [draftLoader, setDraftLoader]       = createSignal('any');

  const commitRule = () => {
    if (draftVersions().length === 0) return;
    addVersionRule({ modId: modId(), kind: draftKind(), mcVersions: draftVersions(), loader: draftLoader() });
    setAddingRule(false); setDraftVersions([]); setDraftLoader('any'); setDraftKind('exclude');
  };

  // ── Link add form ──────────────────────────────────────────────────────────
  const [addingLink, setAddingLink]         = createSignal(false);
  const [newLinkPartnerId, setNewLinkPartnerId] = createSignal('');
  const [newLinkDir, setNewLinkDir]         = createSignal<'a-to-b' | 'mutual' | 'b-to-a'>('a-to-b');

  const availableLinkPartners = () => {
    const me = modId();
    const linked = new Set((linksByModId().get(me) ?? []).map(l => l.partnerId));
    const all = [...rowMap().entries()].filter(([id]) => id !== me && !linked.has(id));
    return all.map(([id, r]) => ({ id, name: r.name }));
  };

  const commitLink = () => {
    const partner = newLinkPartnerId();
    if (!partner) return;
    const me = modId();
    setSavedLinks(cur => {
      const without = cur.filter(l => !((l.fromId === me && l.toId === partner) || (l.fromId === partner && l.toId === me)));
      if (newLinkDir() === 'a-to-b') return [...without, { fromId: me, toId: partner }];
      if (newLinkDir() === 'b-to-a') return [...without, { fromId: partner, toId: me }];
      return [...without, { fromId: me, toId: partner }, { fromId: partner, toId: me }];
    });
    setAddingLink(false); setNewLinkPartnerId(''); setNewLinkDir('a-to-b');
  };

  // ── Tags ───────────────────────────────────────────────────────────────────
  const myFGroups       = () => functionalGroupsByBlockId().get(modId()) ?? [];
  const unassignedFGroups = () => functionalGroups().filter(g => !g.modIds.includes(modId()));
  const [addingTag, setAddingTag] = createSignal(false);
  const [newTagName, setNewTagName] = createSignal("");

  // ── Links (current) ────────────────────────────────────────────────────────
  const myLinks = () => linksByModId().get(modId()) ?? [];

  const setLinkDirection = (partner: string, dir: 'a-to-b' | 'mutual' | 'b-to-a' | 'none') => {
    const me = modId();
    setSavedLinks(cur => {
      const without = cur.filter(l => !((l.fromId === me && l.toId === partner) || (l.fromId === partner && l.toId === me)));
      if (dir === 'none') return without;
      if (dir === 'a-to-b') return [...without, { fromId: me, toId: partner }];
      if (dir === 'b-to-a') return [...without, { fromId: partner, toId: me }];
      return [...without, { fromId: me, toId: partner }, { fromId: partner, toId: me }];
    });
  };

  const currentLinkDir = (partnerId: string): 'a-to-b' | 'mutual' | 'b-to-a' | 'none' => {
    const me = modId();
    const links = savedLinks();
    const ab = links.some(l => l.fromId === me && l.toId === partnerId);
    const ba = links.some(l => l.fromId === partnerId && l.toId === me);
    if (ab && ba) return 'mutual';
    if (ab) return 'a-to-b';
    if (ba) return 'b-to-a';
    return 'none';
  };

  const toggleLinkDir = (partnerId: string, target: 'a-to-b' | 'mutual' | 'b-to-a') => {
    setLinkDirection(partnerId, currentLinkDir(partnerId) === target ? 'none' : target);
  };

  const dirBtnClass = (active: boolean) =>
    `rounded px-1.5 py-0.5 text-xs font-bold transition-colors ${active ? 'bg-primary/20 text-primary ring-1 ring-primary/30' : 'text-muted-foreground hover:bg-muted'}`;

  // ── Relationships ──────────────────────────────────────────────────────────
  const parentId  = () => parentIdByChildId().get(modId());
  const parentRow = () => { const pid = parentId(); return pid ? rowMap().get(pid) : undefined; };
  const childRows = () => row()?.alternatives ?? [];

  // ── Custom Configs ─────────────────────────────────────────────────────────
  const myConfigs = () => customConfigs().filter(c => c.modId === modId());

  const pickFiles = async (configId: string) => {
    if (!isTauri()) return;
    try {
      const result = await openFileDialog({ multiple: true, directory: false });
      if (!result) return;
      const paths = Array.isArray(result) ? result : [result];
      const cfg = customConfigs().find(c => c.id === configId);
      if (cfg) updateCustomConfig(configId, { files: [...cfg.files, ...paths] });
    } catch { /* dialog cancelled */ }
  };

  return (
    <Show when={row()}>
      {/* Backdrop */}
      <div
        class="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-black/60 px-4 py-8 backdrop-blur-sm"
        onClick={e => { if (e.target === e.currentTarget) setAdvancedPanelModId(null); }}
      >
        <div class="flex w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xl">

          {/* Header */}
          <div class="flex items-center justify-between border-b border-border px-6 py-4 shrink-0">
            <div>
              <h2 class="text-lg font-semibold text-foreground">Advanced</h2>
              <p class="text-sm text-muted-foreground truncate max-w-md">{row()!.name}</p>
            </div>
            <button
              onClick={() => setAdvancedPanelModId(null)}
              class="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              <XIcon class="h-4 w-4" />
            </button>
          </div>

          {/* Body */}
          <div class="flex-1 overflow-y-auto max-h-[70vh] divide-y divide-border">

            {/* ── Version Rules ───────────────────────────────────────── */}
            <div>
              <SectionHeader title="Version Rules" />
              <div class="p-4 space-y-2">
                <For each={versionRules().filter(r => r.modId === modId())}>
                  {rule => (
                    <div class="flex flex-wrap items-center gap-2 rounded-md border border-border bg-background p-2">
                      {/* Kind toggle */}
                      <select
                        value={rule.kind}
                        onChange={e => updateVersionRule(rule.id, { kind: e.currentTarget.value as 'exclude' | 'only' })}
                        class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                      >
                        <option value="exclude">Exclude when</option>
                        <option value="only">Only when</option>
                      </select>
                      {/* Versions */}
                      <select
                        value={rule.mcVersions[0] ?? ""}
                        onChange={e => updateVersionRule(rule.id, { mcVersions: e.currentTarget.value ? [e.currentTarget.value] : [] })}
                        class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                      >
                        <option value="">Any version</option>
                        <For each={minecraftVersions()}>
                          {v => <option value={v}>{v}</option>}
                        </For>
                      </select>
                      {/* Loader */}
                      <select
                        value={rule.loader}
                        onChange={e => updateVersionRule(rule.id, { loader: e.currentTarget.value })}
                        class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                      >
                        <For each={ALL_LOADERS}>{l => <option value={l}>{l === 'any' ? 'Any loader' : l}</option>}</For>
                      </select>
                      {/* Delete */}
                      <button
                        onClick={() => removeVersionRule(rule.id)}
                        class="ml-auto flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition-colors"
                      >
                        <XIcon class="h-3.5 w-3.5" />
                      </button>
                    </div>
                  )}
                </For>

                {/* Add rule form */}
                <Show when={addingRule()}>
                  <div class="rounded-md border border-primary/30 bg-primary/5 p-3 space-y-2">
                    <div class="flex flex-wrap gap-2 items-center">
                      <select
                        value={draftKind()}
                        onChange={e => setDraftKind(e.currentTarget.value as 'exclude' | 'only')}
                        class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                      >
                        <option value="exclude">Exclude when</option>
                        <option value="only">Only when</option>
                      </select>
                      <select
                        value={draftVersions()[0] ?? ""}
                        onChange={e => setDraftVersions(e.currentTarget.value ? [e.currentTarget.value] : [])}
                        class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                      >
                        <option value="">Select version…</option>
                        <For each={minecraftVersions()}>
                          {v => <option value={v}>{v}</option>}
                        </For>
                      </select>
                      <select
                        value={draftLoader()}
                        onChange={e => setDraftLoader(e.currentTarget.value)}
                        class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                      >
                        <For each={ALL_LOADERS}>{l => <option value={l}>{l === 'any' ? 'Any loader' : l}</option>}</For>
                      </select>
                    </div>
                    <div class="flex gap-2">
                      <button
                        onClick={commitRule}
                        disabled={draftVersions().length === 0}
                        class="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                      >
                        Add
                      </button>
                      <button
                        onClick={() => { setAddingRule(false); setDraftVersions([]); }}
                        class="rounded-md bg-secondary px-3 py-1 text-xs text-secondary-foreground hover:bg-secondary/80"
                      >
                        Cancel
                      </button>
                    </div>
                  </div>
                </Show>

                <Show when={!addingRule()}>
                  <button
                    onClick={() => setAddingRule(true)}
                    class="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                  >
                    <MaterialIcon name="add" size="sm" />
                    Add Rule
                  </button>
                </Show>
              </div>
            </div>

            {/* ── Tags ────────────────────────────────────────────────── */}
            <div>
              <SectionHeader title="Tags" />
              <div class="p-4 space-y-2">
                <div class="flex flex-wrap gap-1.5">
                  <For each={myFGroups()}>
                    {g => (
                      <span class={functionalGroupTagClass(g.tone)}>
                        {g.name}
                        <button
                          onClick={() => removeFunctionalGroupMember(g.id, modId())}
                          class="ml-0.5 opacity-50 hover:opacity-100 transition-opacity"
                          title={`Remove from "${g.name}"`}
                        >
                          <XIcon class="h-2.5 w-2.5" />
                        </button>
                      </span>
                    )}
                  </For>
                  <Show when={myFGroups().length === 0}>
                    <span class="text-xs text-muted-foreground">No tags assigned.</span>
                  </Show>
                </div>

                {/* Add tag */}
                <div class="relative inline-block">
                  <button
                    onClick={() => { setAddingTag(o => !o); setNewTagName(""); }}
                    class="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                  >
                    <MaterialIcon name="add" size="sm" />
                    Add Tag
                    <MaterialIcon name={addingTag() ? "expand_less" : "expand_more"} size="sm" />
                  </button>
                  <Show when={addingTag()}>
                    <div class="fixed inset-0 z-10" onClick={() => setAddingTag(false)} />
                    <div class="absolute left-0 top-full mt-1 z-20 min-w-[160px] rounded-lg border border-border bg-card shadow-lg overflow-hidden">
                      <For each={unassignedFGroups()}>
                        {g => (
                          <button
                            onClick={() => { addModToFunctionalGroup(g.id, modId()); setAddingTag(false); }}
                            class="w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-muted/30 transition-colors"
                          >
                            <span class={functionalGroupTagClass(g.tone)}>{g.name}</span>
                          </button>
                        )}
                      </For>
                      <Show when={unassignedFGroups().length > 0}>
                        <div class="border-t border-border" />
                      </Show>
                      <div class="px-2 py-2 flex items-center gap-1">
                        <input
                          type="text"
                          value={newTagName()}
                          onInput={e => setNewTagName(e.currentTarget.value)}
                          onKeyDown={e => {
                            if (e.key === "Enter" && newTagName().trim()) {
                              createFunctionalGroupForMod(newTagName(), modId());
                              setAddingTag(false);
                              setNewTagName("");
                            }
                          }}
                          placeholder="New tag…"
                          class="flex-1 min-w-0 rounded border border-border bg-input px-2 py-1 text-xs text-foreground placeholder-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                          onClick={e => e.stopPropagation()}
                        />
                        <button
                          onClick={e => {
                            e.stopPropagation();
                            if (!newTagName().trim()) return;
                            createFunctionalGroupForMod(newTagName(), modId());
                            setAddingTag(false);
                            setNewTagName("");
                          }}
                          disabled={!newTagName().trim()}
                          class="rounded bg-primary px-2 py-1 text-xs text-primary-foreground hover:bg-primary/90 disabled:opacity-40"
                        >
                          Add
                        </button>
                      </div>
                    </div>
                  </Show>
                </div>
              </div>
            </div>

            {/* ── Links ───────────────────────────────────────────────── */}
            <div>
              <SectionHeader title="Links" />
              <div class="p-4 space-y-2">
                <For each={myLinks()}>
                  {link => {
                    const partnerName = () => rowMap().get(link.partnerId)?.name ?? link.partnerId;
                    const dir = () => currentLinkDir(link.partnerId);
                    return (
                      <div class="flex items-center gap-2 rounded-md border border-border bg-background p-2">
                        <span class="text-sm font-medium text-foreground truncate min-w-0 flex-1 text-right">{row()!.name}</span>
                        <div class="flex shrink-0 items-center gap-0.5">
                          <button onClick={() => toggleLinkDir(link.partnerId, 'a-to-b')} class={dirBtnClass(dir() === 'a-to-b')} title={`${row()!.name} requires ${partnerName()}`}>→</button>
                          <button onClick={() => toggleLinkDir(link.partnerId, 'mutual')} class={dirBtnClass(dir() === 'mutual')} title="Mutual dependency">↔</button>
                          <button onClick={() => toggleLinkDir(link.partnerId, 'b-to-a')} class={dirBtnClass(dir() === 'b-to-a')} title={`${partnerName()} requires ${row()!.name}`}>←</button>
                        </div>
                        <span class="text-sm font-medium text-foreground truncate min-w-0 flex-1">{partnerName()}</span>
                        <button
                          onClick={() => setLinkDirection(link.partnerId, 'none')}
                          class="shrink-0 flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition-colors"
                        >
                          <XIcon class="h-3.5 w-3.5" />
                        </button>
                      </div>
                    );
                  }}
                </For>
                <Show when={myLinks().length === 0 && !addingLink()}>
                  <span class="text-xs text-muted-foreground">No links defined.</span>
                </Show>

                {/* Add link form */}
                <Show when={addingLink()}>
                  <div class="rounded-md border border-primary/30 bg-primary/5 p-3 space-y-2">
                    <div class="flex flex-wrap items-center gap-2">
                      <span class="text-sm font-medium text-foreground">{row()!.name}</span>
                      <div class="flex items-center gap-0.5">
                        <button onClick={() => setNewLinkDir('a-to-b')} class={dirBtnClass(newLinkDir() === 'a-to-b')}>→</button>
                        <button onClick={() => setNewLinkDir('mutual')} class={dirBtnClass(newLinkDir() === 'mutual')}>↔</button>
                        <button onClick={() => setNewLinkDir('b-to-a')} class={dirBtnClass(newLinkDir() === 'b-to-a')}>←</button>
                      </div>
                      <select
                        value={newLinkPartnerId()}
                        onChange={e => setNewLinkPartnerId(e.currentTarget.value)}
                        class="flex-1 rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                      >
                        <option value="">Select mod…</option>
                        <For each={availableLinkPartners()}>
                          {p => <option value={p.id}>{p.name}</option>}
                        </For>
                      </select>
                    </div>
                    <div class="flex gap-2">
                      <button onClick={commitLink} disabled={!newLinkPartnerId()} class="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50">Add</button>
                      <button onClick={() => { setAddingLink(false); setNewLinkPartnerId(''); }} class="rounded-md bg-secondary px-3 py-1 text-xs text-secondary-foreground hover:bg-secondary/80">Cancel</button>
                    </div>
                  </div>
                </Show>

                <Show when={!addingLink()}>
                  <button onClick={() => setAddingLink(true)} class="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors">
                    <MaterialIcon name="add" size="sm" />
                    Add Link
                  </button>
                </Show>
              </div>
            </div>

            {/* ── Relationships ────────────────────────────────────────── */}
            <div>
              <SectionHeader title="Relationships" />
              <div class="p-4 space-y-2">
                <Show when={parentRow()}>
                  <div class="flex items-center gap-2">
                    <span class="text-xs text-muted-foreground shrink-0">Parent:</span>
                    <button
                      onClick={() => setAdvancedPanelModId(parentId()!)}
                      class="text-sm font-medium text-primary hover:underline truncate"
                    >
                      {parentRow()!.name}
                    </button>
                  </div>
                </Show>
                <Show when={childRows().length > 0}>
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="text-xs text-muted-foreground shrink-0">Children:</span>
                    <For each={childRows()}>
                      {child => (
                        <button
                          onClick={() => setAdvancedPanelModId(child.id)}
                          class="text-sm font-medium text-primary hover:underline"
                        >
                          {child.name}
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
                <Show when={!parentRow() && childRows().length === 0}>
                  <span class="text-xs text-muted-foreground">No parent or child mods.</span>
                </Show>
              </div>
            </div>

          </div>{/* end scrollable body */}

          {/* ── Custom Configs footer ────────────────────────────────────── */}
          <div class="border-t border-border p-4 space-y-3 shrink-0 max-h-64 overflow-y-auto">
            <For each={myConfigs()}>
              {cfg => (
                <div class="rounded-md border border-border bg-background p-3 space-y-2">
                  <div class="flex items-center justify-between">
                    <span class="text-xs font-semibold text-muted-foreground uppercase tracking-wide">Config</span>
                    <button
                      onClick={() => removeCustomConfig(cfg.id)}
                      class="flex items-center gap-1 rounded px-2 py-0.5 text-xs text-destructive hover:bg-destructive/10 transition-colors"
                    >
                      <XIcon class="h-3 w-3" /> Delete
                    </button>
                  </div>

                  {/* Versions */}
                  <div class="flex items-center gap-2">
                    <label class="text-xs text-muted-foreground shrink-0">Versions:</label>
                    <select
                      value={cfg.mcVersions[0] ?? ""}
                      onChange={e => updateCustomConfig(cfg.id, { mcVersions: e.currentTarget.value ? [e.currentTarget.value] : [] })}
                      class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground flex-1"
                    >
                      <option value="">Any version</option>
                      <For each={minecraftVersions()}>
                        {v => <option value={v}>{v}</option>}
                      </For>
                    </select>
                  </div>

                  {/* Loader */}
                  <div class="flex items-center gap-2">
                    <label class="text-xs text-muted-foreground shrink-0">Loader:</label>
                    <select
                      value={cfg.loader}
                      onChange={e => updateCustomConfig(cfg.id, { loader: e.currentTarget.value })}
                      class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
                    >
                      <For each={ALL_LOADERS}>{l => <option value={l}>{l === 'any' ? 'Any loader' : l}</option>}</For>
                    </select>
                  </div>

                  {/* Target path */}
                  <div class="flex items-center gap-2">
                    <label class="text-xs text-muted-foreground shrink-0">Path:</label>
                    <input
                      type="text"
                      value={cfg.targetPath}
                      onInput={e => updateCustomConfig(cfg.id, { targetPath: e.currentTarget.value })}
                      placeholder="e.g. config/sodium.json"
                      class="flex-1 rounded border border-border bg-input px-2 py-1 text-xs text-foreground placeholder-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                    />
                  </div>

                  {/* Files */}
                  <div>
                    <label class="text-xs text-muted-foreground mb-1 block">Config files:</label>
                    <div class="space-y-1">
                      <For each={cfg.files}>
                        {(file, idx) => (
                          <div class="flex items-center gap-2 rounded bg-muted/30 px-2 py-1">
                            <span class="flex-1 truncate text-xs text-foreground" title={file}>{file.split(/[\\/]/).pop()}</span>
                            <button
                              onClick={() => updateCustomConfig(cfg.id, { files: cfg.files.filter((_, i) => i !== idx()) })}
                              class="shrink-0 text-muted-foreground hover:text-destructive transition-colors"
                            >
                              <XIcon class="h-3 w-3" />
                            </button>
                          </div>
                        )}
                      </For>
                    </div>
                    <button
                      onClick={() => void pickFiles(cfg.id)}
                      class="mt-1.5 flex items-center gap-1 rounded-md border border-dashed border-border px-3 py-1.5 text-xs text-muted-foreground hover:border-primary/50 hover:text-foreground transition-colors w-full justify-center"
                    >
                      <MaterialIcon name="upload_file" size="sm" />
                      Add Files…
                    </button>
                  </div>
                </div>
              )}
            </For>

            <button
              onClick={() => addCustomConfig(modId())}
              class="flex w-full items-center justify-center gap-1.5 rounded-md border border-dashed border-border px-4 py-2 text-sm text-muted-foreground hover:border-primary/50 hover:text-foreground transition-colors"
            >
              <MaterialIcon name="add" size="md" />
              Add Custom Config
            </button>
          </div>

        </div>
      </div>
    </Show>
  );
}
