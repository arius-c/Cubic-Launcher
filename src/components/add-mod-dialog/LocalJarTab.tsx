import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { localJarRuleName, setLocalJarRuleName } from "../../store";
import { UploadIcon } from "../icons";

export function LocalJarTab(props: { contentType: string; onUploadLocal: () => Promise<void>; onDropJar?: (path: string) => Promise<void> }) {
  const [dragging, setDragging] = createSignal(false);

  const isMod = () => props.contentType === "mod";
  const fileExt = () => isMod() ? ".jar" : ".zip";
  const fileLabel = () => isMod() ? "JAR" : "ZIP";
  const typeLabel = () => {
    switch (props.contentType) {
      case "resourcepack": return "Resource Pack";
      case "datapack": return "Data Pack";
      case "shader": return "Shader";
      default: return "JAR";
    }
  };

  onMount(async () => {
    try {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const win = getCurrentWindow();
      const unlisten = await win.onDragDropEvent(async event => {
        if (event.payload.type === "over" || event.payload.type === "enter") {
          setDragging(true);
        } else if (event.payload.type === "leave") {
          setDragging(false);
        } else if (event.payload.type === "drop") {
          setDragging(false);
          const paths: string[] = event.payload.paths ?? [];
          const matchedPath = paths.find(path => path.endsWith(fileExt()));
          if (matchedPath && props.onDropJar) {
            await props.onDropJar(matchedPath);
          }
        }
      });
      onCleanup(() => unlisten());
    } catch {
      // not in Tauri
    }
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
          {dragging() ? `Drop ${fileLabel()} file here` : `Upload ${typeLabel()} File`}
        </h4>
        <p class="mb-4 max-w-xs text-center text-sm text-muted-foreground">
          Drag & drop a <code>{fileExt()}</code> file here, or click Browse Files below.
        </p>
        <div class="flex w-full max-w-xs flex-col gap-3">
          <input
            type="text"
            placeholder="Rule name (optional - defaults to filename)"
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
        <Show when={isMod()}>
          <p class="mt-4 max-w-xs text-center text-xs text-warning">
            Local mods carry a dependency warning - you must manually verify and add required library mods.
          </p>
        </Show>
      </div>
    </div>
  );
}
