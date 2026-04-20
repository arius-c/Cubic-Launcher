import { Show } from "solid-js";
import { XIcon } from "../icons";

export function Modal(props: { children: any; onClose: () => void; maxWidth?: string }) {
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

export function ModalHeader(props: { title: string; description?: string; onClose: () => void }) {
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
