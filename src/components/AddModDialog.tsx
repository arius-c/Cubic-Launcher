import { For, Show, createSignal, createEffect, onMount, onCleanup } from "solid-js";
import {
  addModModalOpen, setAddModModalOpen,
  addModSearch, setAddModSearch,
  addModMode, setAddModMode,
  localJarRuleName, setLocalJarRuleName,
  MOCK_MODRINTH, modRowsState,
} from "../store";
import { SearchIcon, XIcon, UploadIcon, PackageIcon, CheckIcon, Loader2Icon } from "./icons";
import type { ModrinthResult } from "../lib/types";

interface AddModDialogProps {
  onAddModrinth: (id: string, name: string) => Promise<void>;
  onUploadLocal: () => Promise<void>;
  onDropJar?: (path: string) => Promise<void>;
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

interface SearchFilters {
  categories: string[];
  loaders: string[];
  versions: string[];
  environments: string[];
  sortBy: string;
}

const MOD_CATEGORIES = [
  "adventure", "decoration", "economy", "equipment", "food", "game-mechanics",
  "library", "magic", "management", "minigame", "mobs", "optimization",
  "social", "storage", "technology", "transportation", "utility", "worldgen",
];

const MOD_LOADERS_SEARCH = ["fabric", "forge", "neoforge", "quilt"];
const SORT_OPTIONS = [
  { value: "relevance", label: "Relevance" },
  { value: "downloads", label: "Downloads" },
  { value: "follows", label: "Follows" },
  { value: "updated", label: "Updated" },
  { value: "newest", label: "Newest" },
];

async function searchModrinth(query: string, filters: SearchFilters, offset = 0): Promise<{ results: ModrinthResult[]; totalHits: number }> {
  try {
    const facetGroups: string[][] = [["project_type:mod"]];
    if (filters.categories.length > 0) facetGroups.push(filters.categories.map(c => `categories:${c}`));
    if (filters.loaders.length > 0) facetGroups.push(filters.loaders.map(l => `categories:${l}`));
    if (filters.versions.length > 0) facetGroups.push(filters.versions.map(v => `versions:${v}`));
    if (filters.environments.length > 0) {
      for (const env of filters.environments) {
        facetGroups.push([`${env}_side:required`, `${env}_side:optional`]);
      }
    }

    const params = new URLSearchParams();
    if (query.trim()) params.set("query", query);
    params.set("limit", "20");
    if (offset > 0) params.set("offset", String(offset));
    params.set("facets", JSON.stringify(facetGroups));
    params.set("index", (!query.trim() && filters.sortBy === "relevance") ? "downloads" : (filters.sortBy || "relevance"));

    const url = `https://api.modrinth.com/v2/search?${params}`;
    const res = await fetch(url, { headers: { "User-Agent": "CubicLauncher/0.1.0" } });
    if (!res.ok) throw new Error(`Modrinth returned HTTP ${res.status}`);
    const data: { hits: ModrinthHit[]; total_hits: number } = await res.json();
    return {
      totalHits: data.total_hits,
      results: data.hits.map(h => ({
        id: h.slug || h.project_id,
        name: h.title,
        author: h.author,
        description: h.description,
        categories: h.categories.slice(0, 3),
        iconUrl: h.icon_url,
        downloads: h.downloads,
      })),
    };
  } catch {
    const filtered = MOCK_MODRINTH.filter(m =>
      m.name.toLowerCase().includes(query.toLowerCase()) ||
      m.author.toLowerCase().includes(query.toLowerCase())
    );
    return { results: filtered, totalHits: filtered.length };
  }
}

// ── Local JAR drop zone ───────────────────────────────────────────────────────

function LocalJarTab(props: { onUploadLocal: () => Promise<void>; onDropJar?: (path: string) => Promise<void> }) {
  const [dragging, setDragging] = createSignal(false);

  onMount(async () => {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const win = getCurrentWindow();
      const unlisten = await win.onDragDropEvent(async (event) => {
        if (event.payload.type === "over" || event.payload.type === "enter") {
          setDragging(true);
        } else if (event.payload.type === "leave") {
          setDragging(false);
        } else if (event.payload.type === "drop") {
          setDragging(false);
          const paths: string[] = event.payload.paths ?? [];
          const jarPath = paths.find(p => p.endsWith(".jar"));
          if (jarPath && props.onDropJar) {
            await props.onDropJar(jarPath);
          }
        }
      });
      onCleanup(() => unlisten());
    } catch { /* not in Tauri */ }
  });

  return (
    <div class="space-y-4">
      <div
        class={`flex flex-col items-center justify-center rounded-lg border-2 border-dashed py-14 transition-colors ${
          dragging() ? "border-primary bg-primary/10" : "border-border bg-muted/20"
        }`}
      >
        <UploadIcon class={`mb-4 h-12 w-12 transition-colors ${dragging() ? "text-primary" : "text-muted-foreground/50"}`} />
        <h4 class="mb-1 font-medium text-foreground">
          {dragging() ? "Drop JAR file here" : "Upload JAR File"}
        </h4>
        <p class="mb-4 max-w-xs text-center text-sm text-muted-foreground">
          Drag & drop a <code>.jar</code> file here, or click Browse Files below.
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
  );
}

// ── Component ─────────────────────────────────────────────────────────────────

export function AddModDialog(props: AddModDialogProps) {
  const [searchResults, setSearchResults] = createSignal<ModrinthResult[]>([]);
  const [totalHits, setTotalHits] = createSignal(0);
  const [page, setPage] = createSignal(0);
  const [searching, setSearching] = createSignal(false);
  const [addingIds, setAddingIds] = createSignal<Set<string>>(new Set());
  const [addedIds,  setAddedIds]  = createSignal<Set<string>>(new Set());
  const [selCategories, setSelCategories] = createSignal<Set<string>>(new Set());
  const [selLoaders, setSelLoaders] = createSignal<Set<string>>(new Set());
  const [selVersions, setSelVersions] = createSignal<Set<string>>(new Set());
  const [selEnvs, setSelEnvs] = createSignal<Set<string>>(new Set());
  const [sortBy, setSortBy] = createSignal("relevance");
  const [showMoreCats, setShowMoreCats] = createSignal(false);

  const PAGE_SIZE = 20;
  const totalPages = () => Math.max(1, Math.ceil(totalHits() / PAGE_SIZE));

  const toggleSet = (setter: (fn: (s: Set<string>) => Set<string>) => void, val: string) => {
    setter(cur => { const next = new Set(cur); if (next.has(val)) next.delete(val); else next.add(val); return next; });
  };

  const filters = (): SearchFilters => ({
    categories: [...selCategories()],
    loaders: [...selLoaders()],
    versions: [...selVersions()],
    environments: [...selEnvs()],
    sortBy: sortBy(),
  });

  const hasFilters = () => selCategories().size > 0 || selLoaders().size > 0 || selVersions().size > 0 || selEnvs().size > 0;

  const runSearch = async (pg?: number) => {
    const p = pg ?? page();
    setSearching(true);
    try {
      const { results, totalHits: total } = await searchModrinth(addModSearch(), filters(), p * PAGE_SIZE);
      setSearchResults(results);
      setTotalHits(total);
    } catch { /* */ }
    setSearching(false);
  };

  // Debounced search: fires 350 ms after the user stops typing or filters change
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const q = addModSearch();
    const f = filters(); // track filter changes
    clearTimeout(debounceTimer);
    if (addModMode() !== "modrinth") return;

    setPage(0); // reset to page 0 on query/filter change
    debounceTimer = setTimeout(() => void runSearch(0), 350);
  });

  // Reset state and fetch popular mods when dialog opens
  createEffect(() => {
    if (addModModalOpen()) {
      setAddedIds(new Set<string>());
      setAddingIds(new Set<string>());
      setPage(0);
      void runSearch(0);
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
        <div class="flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-lg border border-border bg-card shadow-xl">

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
          <div class="flex-1 min-h-0 flex flex-col overflow-hidden">
            <Show
              when={addModMode() === "modrinth"}
              fallback={
                <div class="flex-1 overflow-y-auto p-6"><LocalJarTab onUploadLocal={props.onUploadLocal} onDropJar={props.onDropJar} /></div>
              }
            >
              {/* ── Modrinth search tab ── */}
              <div class="flex flex-1 min-h-0">

                {/* Sidebar filters */}
                <div class="w-48 shrink-0 border-r border-border overflow-y-auto p-3 space-y-3">
                  {/* Sort */}
                  <div>
                    <h4 class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground mb-1.5">Sort by</h4>
                    <select value={sortBy()} onChange={e => setSortBy(e.currentTarget.value)} class="w-full rounded border border-border bg-input px-2 py-1 text-xs text-foreground">
                      <For each={SORT_OPTIONS}>{o => <option value={o.value}>{o.label}</option>}</For>
                    </select>
                  </div>

                  {/* Loaders */}
                  <div>
                    <h4 class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground mb-1.5">Loaders</h4>
                    <div class="space-y-1">
                      <For each={MOD_LOADERS_SEARCH}>
                        {l => (
                          <label class="flex items-center gap-1.5 cursor-pointer text-xs text-foreground hover:text-primary transition-colors">
                            <input type="checkbox" checked={selLoaders().has(l)} onChange={() => toggleSet(setSelLoaders, l)} class="accent-accentColor w-3 h-3" />
                            {l.charAt(0).toUpperCase() + l.slice(1)}
                          </label>
                        )}
                      </For>
                    </div>
                  </div>

                  {/* Environments */}
                  <div>
                    <h4 class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground mb-1.5">Environment</h4>
                    <div class="space-y-1">
                      <For each={["client", "server"]}>
                        {env => (
                          <label class="flex items-center gap-1.5 cursor-pointer text-xs text-foreground hover:text-primary transition-colors">
                            <input type="checkbox" checked={selEnvs().has(env)} onChange={() => toggleSet(setSelEnvs, env)} class="accent-accentColor w-3 h-3" />
                            {env.charAt(0).toUpperCase() + env.slice(1)}
                          </label>
                        )}
                      </For>
                    </div>
                  </div>

                  {/* Categories */}
                  <div>
                    <h4 class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground mb-1.5">Categories</h4>
                    <div class="space-y-1">
                      <For each={showMoreCats() ? MOD_CATEGORIES : MOD_CATEGORIES.slice(0, 8)}>
                        {cat => (
                          <label class="flex items-center gap-1.5 cursor-pointer text-xs text-foreground hover:text-primary transition-colors">
                            <input type="checkbox" checked={selCategories().has(cat)} onChange={() => toggleSet(setSelCategories, cat)} class="accent-accentColor w-3 h-3" />
                            {cat.replace("-", " ").replace(/\b\w/g, c => c.toUpperCase())}
                          </label>
                        )}
                      </For>
                      <button onClick={() => setShowMoreCats(v => !v)} class="text-[10px] text-primary hover:underline">
                        {showMoreCats() ? "Show less" : `Show all (${MOD_CATEGORIES.length})`}
                      </button>
                    </div>
                  </div>

                  {/* Clear filters */}
                  <Show when={hasFilters()}>
                    <button
                      onClick={() => { setSelCategories(new Set()); setSelLoaders(new Set()); setSelVersions(new Set()); setSelEnvs(new Set()); }}
                      class="w-full rounded border border-dashed border-border px-2 py-1 text-[10px] text-muted-foreground hover:text-foreground hover:border-primary/40 transition-colors"
                    >
                      Clear all filters
                    </button>
                  </Show>
                </div>

                {/* Results */}
                <div class="flex-1 min-h-0 overflow-y-auto p-4">
                  <div class="relative mb-3">
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
                      class="h-10 w-full rounded-xl border border-input bg-input pl-10 pr-10 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary transition-all"
                      autofocus
                    />
                  </div>

                  <Show when={!addModSearch() && !searching() && !hasFilters()}>
                    <p class="mb-3 text-xs text-muted-foreground">
                      Popular mods on Modrinth
                    </p>
                  </Show>

                <div class="space-y-1.5">
                  <For each={searchResults()}>
                    {mod => {
                      const existingModIds = () => {
                        const ids = new Set<string>();
                        const collect = (rows: ModrinthResult[] | any[]) => {
                          for (const r of rows) {
                            if (r.primaryModId) ids.add(r.primaryModId);
                            if (r.modrinth_id) ids.add(r.modrinth_id);
                            if (r.alternatives?.length) collect(r.alternatives);
                          }
                        };
                        collect(modRowsState());
                        return ids;
                      };
                      const isAdded  = () => addedIds().has(mod.id) || existingModIds().has(mod.id);
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

                  {/* Pagination */}
                  <Show when={totalPages() > 1}>
                    <div class="flex items-center justify-center gap-3 pt-3 pb-1">
                      <button
                        onClick={() => { const p = Math.max(0, page() - 1); setPage(p); void runSearch(p); }}
                        disabled={page() === 0}
                        class="rounded-md border border-border px-3 py-1 text-xs text-foreground hover:bg-muted disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
                      >
                        Previous
                      </button>
                      <span class="text-xs text-muted-foreground">
                        Page {page() + 1} of {totalPages()}
                      </span>
                      <button
                        onClick={() => { const p = Math.min(totalPages() - 1, page() + 1); setPage(p); void runSearch(p); }}
                        disabled={page() >= totalPages() - 1}
                        class="rounded-md border border-border px-3 py-1 text-xs text-foreground hover:bg-muted disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
                      >
                        Next
                      </button>
                    </div>
                  </Show>
                </div>

              </div>{/* end flex sidebar+results */}
              </div>{/* end Modrinth search tab */}
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
