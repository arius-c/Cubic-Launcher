import { MOD_LOADERS } from "../../lib/types";

export const isTauri = () => "__TAURI_INTERNALS__" in window;
export const ALL_LOADERS = ["any", ...MOD_LOADERS];

export function SectionHeader(props: { title: string }) {
  return (
    <div class="px-5 py-2 bg-muted/30 border-b border-border">
      <h3 class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{props.title}</h3>
    </div>
  );
}
