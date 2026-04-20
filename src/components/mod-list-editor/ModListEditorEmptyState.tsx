import { PackageIcon } from "../icons";

interface ModListEditorEmptyStateProps {
  onAddMod: () => void;
}

export function ModListEditorEmptyState(props: ModListEditorEmptyStateProps) {
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
