import { For, Show, createSignal } from "solid-js";
import {
  functionalGroups, functionalGroupsByBlockId, addModToFunctionalGroup, removeFunctionalGroupMember,
  createFunctionalGroupForMod, functionalGroupTagClass, functionalGroupTagStyle,
} from "../../store";
import { MaterialIcon, XIcon } from "../icons";
import { SectionHeader } from "./shared";

export function TagsSection(props: { modId: string }) {
  const myFGroups = () => functionalGroupsByBlockId().get(props.modId) ?? [];
  const unassignedFGroups = () => functionalGroups().filter(group => !group.modIds.includes(props.modId));
  const [addingTag, setAddingTag] = createSignal(false);
  const [newTagName, setNewTagName] = createSignal("");

  return (
    <div>
      <SectionHeader title="Tags" />
      <div class="p-4 space-y-2">
        <div class="flex flex-wrap gap-1.5">
          <For each={myFGroups()}>
            {group => (
              <span class={functionalGroupTagClass(group.tone)} style={functionalGroupTagStyle(group.tone)}>
                {group.name}
                <button
                  onClick={() => removeFunctionalGroupMember(group.id, props.modId)}
                  class="ml-0.5 opacity-50 hover:opacity-100 transition-opacity"
                  title={`Remove from "${group.name}"`}
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

        <div class="relative inline-block">
          <button
            onClick={() => { setAddingTag(open => !open); setNewTagName(""); }}
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
                {group => (
                  <button
                    onClick={() => { addModToFunctionalGroup(group.id, props.modId); setAddingTag(false); }}
                    class="w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-muted/30 transition-colors"
                  >
                    <span class={functionalGroupTagClass(group.tone)} style={functionalGroupTagStyle(group.tone)}>{group.name}</span>
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
                      createFunctionalGroupForMod(newTagName(), props.modId);
                      setAddingTag(false);
                      setNewTagName("");
                    }
                  }}
                  placeholder="New tag..."
                  class="flex-1 min-w-0 rounded border border-border bg-input px-2 py-1 text-xs text-foreground placeholder-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring"
                  onClick={e => e.stopPropagation()}
                />
                <button
                  onClick={e => {
                    e.stopPropagation();
                    if (!newTagName().trim()) return;
                    createFunctionalGroupForMod(newTagName(), props.modId);
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
  );
}
