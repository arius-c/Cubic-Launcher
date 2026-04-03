import { Show } from "solid-js";
import { modIcons } from "../store";
import { PackageIcon } from "./icons";

/** Tiny inline mod icon. Looks up the icon URL from the modIcons signal. */
export function ModIcon(props: { modrinthId?: string; name?: string; class?: string }) {
  const size = () => props.class ?? "h-4 w-4";
  const url = () => props.modrinthId ? modIcons().get(props.modrinthId) : undefined;
  return (
    <div class={`${size()} shrink-0 overflow-hidden rounded`}>
      <Show when={url()} fallback={<PackageIcon class={`${size()} text-muted-foreground`} />}>
        <img src={url()!} alt={props.name ?? ""} class={`${size()} object-cover`} onError={e => { e.currentTarget.style.display = "none"; }} />
      </Show>
    </div>
  );
}
