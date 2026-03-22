import { For, Show } from "solid-js";
import {
  setSelectedIds, search, setSearch,
  selectedCount, createAestheticGroup as createGroup, openIncompatibilityEditor,
  selectedTopLevelId, openAlternativesPanel, setFunctionalGroupModalOpen,
  functionalGroups, tagFilter, setTagFilter, toggleTagFilter, sortOrder, setSortOrder,
  functionalGroupTagClass, openLinkModal,
} from "../store";
import { MaterialIcon, XIcon } from "./icons";

interface ActionBarProps {
  onAddMod: () => void;
  onDeleteSelected: () => void;
}

export function ActionBar(props: ActionBarProps) {
  /* ── Contextual bar (when mods selected) ──────────────────────── */
  const ContextualBar = () => (
    <header class="h-14 bg-primary/20 border-b border-primary flex items-center px-6 justify-between shrink-0">
      <div class="flex items-center gap-4">
        <span class="text-sm font-medium text-primary border-r border-primary/30 pr-4">
          {selectedCount()} mod{selectedCount() !== 1 ? "s" : ""} selected
        </span>
        <div class="flex items-center gap-2">
          <button
            onClick={createGroup}
            class="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-primary hover:text-white hover:bg-primary/40 rounded-lg border border-primary/30 transition-colors duration-75"
          >
            <MaterialIcon name="folder_open" size="md" />
            Create Group
          </button>
          <button
            onClick={props.onDeleteSelected}
            class="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-primary hover:text-white hover:bg-primary/40 rounded-lg border border-primary/30 transition-colors duration-75"
          >
            <MaterialIcon name="delete" size="md" />
            Delete
          </button>
          <button
            onClick={() => setFunctionalGroupModalOpen(true)}
            class="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-primary hover:text-white hover:bg-primary/40 rounded-lg border border-primary/30 transition-colors duration-75"
          >
            <MaterialIcon name="folder_special" size="md" />
            Add Tag
          </button>
          <button
            onClick={openLinkModal}
            disabled={selectedCount() < 2}
            class="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-primary hover:text-white hover:bg-primary/40 rounded-lg border border-primary/30 transition-colors duration-75 disabled:opacity-50 disabled:cursor-not-allowed"
            title={selectedCount() >= 2 ? "Link selected mods" : "Select 2 or more mods to link"}
          >
            <MaterialIcon name="link" size="md" />
            Link
          </button>
          <button
            onClick={openIncompatibilityEditor}
            class="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-primary hover:text-white hover:bg-primary/40 rounded-lg border border-primary/30 transition-colors duration-75"
          >
            <MaterialIcon name="warning" size="md" />
            Incompatibilities
          </button>
          <button
            onClick={() => {
              const pid = selectedTopLevelId();
              if (pid) openAlternativesPanel(pid);
            }}
            disabled={!selectedTopLevelId()}
            class="flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-primary hover:text-white hover:bg-primary/40 rounded-lg border border-primary/30 transition-colors duration-75 disabled:opacity-50 disabled:cursor-not-allowed"
            title={selectedTopLevelId() ? "Manage fallback alternatives" : "Select exactly one mod"}
          >
            <MaterialIcon name="account_tree" size="md" />
            Alternatives
          </button>
        </div>
      </div>
      <button
        onClick={() => setSelectedIds([])}
        class="text-primary hover:text-white p-1 transition-colors duration-75"
      >
        <MaterialIcon name="close" size="lg" />
      </button>
    </header>
  );

  /* ── Normal bar ───────────────────────────────────────────────── */
  const sortBtn = (label: string, value: "default" | "name-az" | "name-za") => (
    <button
      onClick={() => setSortOrder(value)}
      class={`px-2 py-0.5 rounded text-xs transition-colors duration-75 ${
        sortOrder() === value
          ? "bg-primary/20 text-primary font-medium"
          : "text-textMuted hover:text-textMain"
      }`}
    >
      {label}
    </button>
  );

  const hasActiveFilters = () => tagFilter().size > 0 || sortOrder() !== "default";

  const NormalBar = () => (
    <div class="px-6 py-2 bg-bgPanel border-b border-borderColor shrink-0 flex items-center justify-between gap-3">
      {/* Left: Sort + Tag filters + Add Mod */}
      <div class="flex items-center gap-2 flex-wrap">
        {/* Tag filter pills */}
        <Show when={functionalGroups().length > 0}>
          <For each={functionalGroups()}>
            {g => {
              const active = () => tagFilter().has(g.id);
              return (
                <button
                  onClick={() => toggleTagFilter(g.id)}
                  class={`${functionalGroupTagClass(g.tone)} cursor-pointer transition-opacity duration-75 ${active() ? "opacity-100" : "opacity-50 hover:opacity-80"}`}
                  title={active() ? `Remove "${g.name}" filter` : `Show only "${g.name}" mods`}
                >
                  <Show when={active()}>
                    <MaterialIcon name="filter_alt" size="sm" class="-ml-0.5" />
                  </Show>
                  {g.name}
                </button>
              );
            }}
          </For>
        </Show>

        {/* Clear filters */}
        <Show when={hasActiveFilters()}>
          <button
            onClick={() => { setTagFilter(new Set<string>()); setSortOrder("default"); }}
            class="flex items-center gap-1 px-2 py-0.5 rounded-md text-xs text-textMuted hover:text-white hover:bg-muted/40 border border-dashed border-borderColor transition-colors duration-75"
            title="Reset all filters and sort"
          >
            <XIcon class="h-3 w-3" />
            Clear
          </button>
        </Show>

        <Show when={functionalGroups().length > 0}>
          <div class="h-4 w-px bg-borderColor" />
        </Show>

        {/* Add Mod */}
        <button
          onClick={props.onAddMod}
          class="px-4 py-1.5 rounded-lg bg-primary hover:bg-brandPurpleHover text-white text-sm font-medium flex items-center gap-2 transition-colors duration-75"
        >
          <MaterialIcon name="add" size="md" />
          Add Mod
        </button>

        {/* Sort group */}
        <div class="flex items-center gap-0.5 rounded-md border border-borderColor bg-bgDark px-1 py-0.5">
          <MaterialIcon name="sort" size="sm" class="text-textMuted mr-0.5" />
          {sortBtn("Default", "default")}
          {sortBtn("A → Z", "name-az")}
          {sortBtn("Z → A", "name-za")}
        </div>
      </div>

      {/* Right: Search */}
      <div class="flex-1 max-w-md relative">
        <MaterialIcon name="search" size="md" class="absolute left-3 top-1/2 -translate-y-1/2 text-textMuted" />
        <input
          type="text"
          placeholder="Search installed mods..."
          value={search()}
          onInput={e => setSearch(e.currentTarget.value)}
          class="w-full bg-bgDark border border-borderColor rounded-lg py-1.5 pl-9 pr-3 text-sm text-textMain placeholder-textMuted focus:outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors duration-75"
        />
        <Show when={search()}>
          <button
            onClick={() => setSearch("")}
            class="absolute right-2 top-1/2 -translate-y-1/2 text-textMuted hover:text-white transition-colors"
          >
            <XIcon class="h-4 w-4" />
          </button>
        </Show>
      </div>
    </div>
  );

  return (
    <Show when={selectedCount() > 0} fallback={<NormalBar />}>
      <ContextualBar />
    </Show>
  );
}
