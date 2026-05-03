import { MOCK_MODRINTH } from "../../store";
import type { ModrinthResult } from "../../lib/types";

export interface AddModDialogProps {
  onAddModrinth: (id: string, name: string) => Promise<void>;
  onAddContent?: (contentType: string, id: string, name: string) => Promise<void>;
  onUploadLocal: () => Promise<void>;
  onDropJar?: (path: string) => Promise<void>;
}

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

export interface SearchFilters {
  categories: string[];
  loaders: string[];
  versions: string[];
  environments: string[];
  sortBy: string;
}

export const MOD_CATEGORIES = [
  "adventure", "decoration", "economy", "equipment", "food", "game-mechanics",
  "library", "magic", "management", "minigame", "mobs", "optimization",
  "social", "storage", "technology", "transportation", "utility", "worldgen",
];

export const MOD_LOADERS_SEARCH = ["fabric", "forge", "neoforge"];
export const SORT_OPTIONS = [
  { value: "relevance", label: "Relevance" },
  { value: "downloads", label: "Downloads" },
  { value: "follows", label: "Follows" },
  { value: "updated", label: "Updated" },
  { value: "newest", label: "Newest" },
];

export function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

export async function searchModrinth(query: string, filters: SearchFilters, offset = 0, projectType = "mod"): Promise<{ results: ModrinthResult[]; totalHits: number }> {
  try {
    const facetGroups: string[][] = [[`project_type:${projectType}`]];
    if (filters.categories.length > 0) facetGroups.push(filters.categories.map(category => `categories:${category}`));
    if (filters.loaders.length > 0) facetGroups.push(filters.loaders.map(loader => `categories:${loader}`));
    if (filters.versions.length > 0) facetGroups.push(filters.versions.map(version => `versions:${version}`));
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
      results: data.hits.map(hit => ({
        id: hit.slug || hit.project_id,
        name: hit.title,
        author: hit.author,
        description: hit.description,
        categories: hit.categories.slice(0, 3),
        iconUrl: hit.icon_url,
        downloads: hit.downloads,
      })),
    };
  } catch {
    const filtered = MOCK_MODRINTH.filter(mod =>
      mod.name.toLowerCase().includes(query.toLowerCase()) ||
      mod.author.toLowerCase().includes(query.toLowerCase())
    );
    return { results: filtered, totalHits: filtered.length };
  }
}
