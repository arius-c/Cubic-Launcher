import { For, Show, createSignal, createEffect } from "solid-js";
import {
  addModModalOpen, setAddModModalOpen,
  addModSearch, setAddModSearch,
  addModMode, setAddModMode,
  localJarRuleName, setLocalJarRuleName,
  MOCK_MODRINTH,
} from "../store";
import { SearchIcon, XIcon, UploadIcon, PackageIcon, CheckIcon, Loader2Icon } from "./icons";
import type { ModrinthResult } from "../lib/types";

interface AddModDialogProps {
  onAddModrinth: (id: string, name: string) => Promise<void>;
  onUploadLocal: () => Promise<void>;
}

function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

// ── Modrinth search ────────────────────────────────────────────────────────────

interface ModrinthHit {
  slug: string;
  project_id: string;
  title: string;
  description: string;
  author: string;
  categories: string[];
  icon_url?: string;
  downloads: number;
}

async function searchModrinth(query: string): Promise<ModrinthResult[]> {
  if (!query.trim()) return MOCK_MODRINTH;
  try {
    const facets = encodeURIComponent(JSON.stringify([["project_type:mod"]]));
    const url = `https://api.modrinth.com/v2/search?query=${encodeURIComponent(query)}&limit=20&facets=${facets}`;
    const res = await fetch(url, { headers: { "User-Agent": "CubicLauncher/0.1.0" } });
    if (!res.ok) throw new Error(`Modrinth returned HTTP ${res.status}`);
    const data: { hits: ModrinthHit[] } = await res.json();
    return data.hits.map(h => ({
      id: h.slug || h.project_id,
      name: h.title,
      author: h.author,
      description: h.description,
      categories: h.categories.slice(0, 3),
      iconUrl: h.icon_url,
      downloads: h.downloads,
    }));
  } catch {
    // Fall back to local mock on network failure or CORS
    return MOCK_MODRINTH.filter(m =>
      m.name.toLowerCase().includes(query.toLowerCase()) ||
      m.author.toLowerCase().includes(query.toLowerCase())
    );
  }
}

// ── Component ─────────────────────────────────────────────────────────────────

export function AddModDialog(props: AddModDialogProps) {
  const [searchResults, setSearchResults] = createSignal<ModrinthResult[]>(MOCK_MODRINTH);
  const [searching, setSearching] = createSignal(false);
  const [addingIds, setAddingIds] = createSignal<Set<string>>(new Set());
  const [addedIds,  setAddedIds]  = createSignal<Set<string>>(new Set());

  // Debounced search: fires 350 ms after the user stops typing
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const q = addModSearch();
    clearTimeout(debounceTimer);
    if (addModMode() !== "modrinth") return;

    if (!q.trim()) {
      setSearchResults(MOCK_MODRINTH);
      return;
    }

    setSearching(true);
    debounceTimer = setTimeout(async () => {
      const results = await searchModrinth(q);
      setSearchResults(results);
      setSearching(false);
    }, 350);
  });

  // Reset added/adding state when dialog opens
  createEffect(() => {
    if (addModModalOpen()) {
      setAddedIds(new Set<string>());
      setAddingIds(new Set<string>());
      if (!addModSearch()) setSearchResults(MOCK_MODRINTH);
    }
  });

  const handleAdd = async (id: string, name: string) => {
    setAddingIds(s => new Set([...s, id]));
    await props.onAddModrinth(id, name);
    setAddingIds(s => { const n = new Set(s); n.delete(id); return n; });
    setAddedIds(s => new Set([...s, id]));
  };

  const close = () => {
    setAddModModalOpen(false);
    setAddModSearch("");
    setAddedIds(new Set<string>());
  };

  return (
    <Show when={addModModalOpen()}>
      {/* Backdrop */}
      <div
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 px-4 py-6 backdrop-blur-sm"
        onClick={e => { if (e.target === e.currentTarget) close(); }}
      >
        {/* Dialog */}
        <div class="flex max-h-[90vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xl">

          {/* Header */}
          <div class="flex items-center justify-between border-b border-border px-6 py-4">
            <div>
              <h2 class="text-lg font-semibold text-foreground">Add Mod</h2>
              <p class="text-sm text-muted-foreground">
                Search Modrinth or upload a local JAR file.
              </p>
            </div>
            <button
              onClick={close}
              class="flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            >
              <XIcon class="h-4 w-4" />
            </button>
          </div>

          {/* Tabs */}
          <div class="flex border-b border-border">
            <button
              onClick={() => { setAddModMode("modrinth"); }}
              class={`flex items-center gap-2 px-6 py-3 text-sm font-medium transition-colors ${
                addModMode() === "modrinth"
                  ? "border-b-2 border-primary text-primary"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <SearchIcon class="h-4 w-4" />
              Search Modrinth
            </button>
            <button
              onClick={() => setAddModMode("local")}
              class={`flex items-center gap-2 px-6 py-3 text-sm font-medium transition-colors ${
                addModMode() === "local"
                  ? "border-b-2 border-primary text-primary"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              <UploadIcon class="h-4 w-4" />
              Upload Local
            </button>
          </div>

          {/* Body */}
          <div class="flex-1 overflow-y-auto p-6">
            <Show
              when={addModMode() === "modrinth"}
              fallback={
                /* ── Local JAR tab ── */
                <div class="space-y-4">
                  <div class="flex flex-col items-center justify-center rounded-lg border-2 border-dashed border-border bg-muted/20 py-14">
                    <UploadIcon class="mb-4 h-12 w-12 text-muted-foreground/50" />
                    <h4 class="mb-1 font-medium text-foreground">Upload JAR File</h4>
                    <p class="mb-4 max-w-xs text-center text-sm text-muted-foreground">
                      Pick a <code>.jar</code> file. It will be copied to <code>cache/mods/</code>
                      and added as a local rule to the current mod list.
                    </p>
                    <div class="flex w-full max-w-xs flex-col gap-3">
                      <input
                        type="text"
                        placeholder="Rule name (optional — defaults to filename)"
                        value={localJarRuleName()}
                        onInput={e => setLocalJarRuleName(e.currentTarget.value)}
                        class="rounded-md border border-input bg-input px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                      />
                      <button
                        onClick={() => void props.onUploadLocal()}
                        class="rounded-md bg-secondary px-4 py-2 text-sm font-medium text-secondary-foreground transition-colors hover:bg-secondary/80"
                      >
                        Browse Files
                      </button>
                    </div>
                    <p class="mt-4 max-w-xs text-center text-xs text-warning">
                      Local mods carry a dependency warning — you must manually verify and add required library mods.
                    </p>
                  </div>
                </div>
              }
            >
              {/* ── Modrinth search tab ── */}
              <div>
                <div class="relative mb-4">
                  <SearchIcon class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
                  <Show when={searching()}>
                    <Loader2Icon class="absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 animate-spin text-muted-foreground" />
                  </Show>
                  <Show when={addModSearch()}>
                    <button
                      onClick={() => setAddModSearch("")}
                      class="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                    >
                      <XIcon class="h-4 w-4" />
                    </button>
                  </Show>
                  <input
                    type="text"
                    placeholder="Search mods..."
                    value={addModSearch()}
                    onInput={e => setAddModSearch(e.currentTarget.value)}
                    class="h-11 w-full rounded-xl border border-input bg-input pl-10 pr-10 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                    autofocus
                  />
                </div>

                <Show when={!addModSearch() && !searching()}>
                  <p class="mb-3 text-xs text-muted-foreground">
                    Popular mods — type to search all of Modrinth
                  </p>
                </Show>

                <div class="space-y-1.5">
                  <For each={searchResults()}>
                    {mod => {
                      const isAdded  = () => addedIds().has(mod.id);
                      const isAdding = () => addingIds().has(mod.id);

                      return (
                        <div class="flex items-center gap-3 rounded-lg p-2.5 transition-colors hover:bg-muted/40 cursor-pointer group" onClick={() => { if (!isAdded() && !isAdding()) void handleAdd(mod.id, mod.name); }}>
                          <Show when={mod.iconUrl} fallback={
                            <div class="flex h-11 w-11 shrink-0 items-center justify-center rounded-lg bg-muted">
                              <PackageIcon class="h-5 w-5 text-muted-foreground" />
                            </div>
                          }>
                            <img src={mod.iconUrl} alt="" class="h-11 w-11 shrink-0 rounded-lg object-cover bg-muted" loading="lazy" />
                          </Show>
                          <div class="min-w-0 flex-1">
                            <div class="flex items-center gap-2">
                              <span class="font-semibold text-sm text-foreground truncate">{mod.name}</span>
                              <span class="text-xs text-muted-foreground shrink-0">by {mod.author}</span>
                            </div>
                            <p class="line-clamp-1 text-xs text-muted-foreground mt-0.5">{mod.description}</p>
                            <div class="flex items-center gap-2 mt-0.5">
                              <Show when={mod.downloads != null}>
                                <span class="text-[10px] text-muted-foreground/70">{formatDownloads(mod.downloads!)} downloads</span>
                              </Show>
                              <For each={mod.categories.slice(0, 3)}>
                                {cat => (
                                  <span class="rounded px-1 py-px text-[10px] text-muted-foreground/70 bg-muted">
                                    {cat}
                                  </span>
                                )}
                              </For>
                            </div>
                          </div>
                          <div class="shrink-0">
                            <Show when={isAdded()}>
                              <span class="flex h-8 items-center gap-1 rounded-lg px-3 text-xs font-medium bg-green-500/15 text-green-500">
                                <CheckIcon class="h-3.5 w-3.5" />
                                Added
                              </span>
                            </Show>
                            <Show when={isAdding()}>
                              <span class="flex h-8 items-center rounded-lg px-3">
                                <Loader2Icon class="h-4 w-4 animate-spin text-muted-foreground" />
                              </span>
                            </Show>
                            <Show when={!isAdded() && !isAdding()}>
                              <button
                                class="flex h-8 items-center rounded-lg px-3 text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 opacity-0 group-hover:opacity-100 transition-opacity"
                              >
                                Install
                              </button>
                            </Show>
                          </div>
                        </div>
                      );
                    }}
                  </For>
                  <Show when={searchResults().length === 0 && !searching()}>
                    <div class="flex flex-col items-center justify-center py-12 text-center">
                      <PackageIcon class="mb-4 h-12 w-12 text-muted-foreground/40" />
                      <p class="text-muted-foreground">
                        No mods found for "{addModSearch()}"
                      </p>
                      <p class="mt-1 text-sm text-muted-foreground/60">
                        Try a different search term
                      </p>
                    </div>
                  </Show>
                </div>
              </div>
            </Show>
          </div>

          {/* Footer */}
          <div class="flex justify-end border-t border-border px-6 py-4">
            <button
              onClick={close}
              class="rounded-md bg-secondary px-4 py-2 text-sm font-medium text-secondary-foreground transition-colors hover:bg-secondary/80"
            >
              Done
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
