import { For, Show, createSignal } from "solid-js";
import { versionRules, addVersionRule, removeVersionRule, updateVersionRule } from "../../store";
import { minecraftVersions, mcWithSnapshots, showSnapshots, setShowSnapshots } from "../../store";
import { MaterialIcon, XIcon } from "../icons";
import { ALL_LOADERS, SectionHeader } from "./shared";

export function VersionRulesSection(props: { modId: string }) {
  const [addingRule, setAddingRule] = createSignal(false);
  const [draftKind, setDraftKind] = createSignal<"exclude" | "only">("exclude");
  const [draftVersions, setDraftVersions] = createSignal<string[]>([]);
  const [draftLoader, setDraftLoader] = createSignal("any");

  const commitRule = () => {
    addVersionRule({ modId: props.modId, kind: draftKind(), mcVersions: draftVersions(), loader: draftLoader() });
    setAddingRule(false);
    setDraftVersions([]);
    setDraftLoader("any");
    setDraftKind("exclude");
  };

  return (
    <div>
      <SectionHeader title="Version Rules" />
      <div class="p-4 space-y-2">
        <For each={versionRules().filter(rule => rule.modId === props.modId)}>
          {rule => (
            <div class="flex flex-wrap items-center gap-2 rounded-md border border-border bg-background p-2">
              <select
                value={rule.kind}
                onChange={e => updateVersionRule(rule.id, { kind: e.currentTarget.value as "exclude" | "only" })}
                class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
              >
                <option value="exclude">Exclude when</option>
                <option value="only">Only when</option>
              </select>
              <select
                value={rule.mcVersions[0] ?? ""}
                onChange={e => updateVersionRule(rule.id, { mcVersions: e.currentTarget.value ? [e.currentTarget.value] : [] })}
                class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
              >
                <option value="">Any version</option>
                <For each={showSnapshots() ? mcWithSnapshots() : minecraftVersions()}>
                  {version => <option value={version}>{version}</option>}
                </For>
              </select>
              <select
                value={rule.loader}
                onChange={e => updateVersionRule(rule.id, { loader: e.currentTarget.value })}
                class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
              >
                <For each={ALL_LOADERS}>{loader => <option value={loader}>{loader === "any" ? "Any loader" : loader}</option>}</For>
              </select>
              <button
                onClick={() => removeVersionRule(rule.id)}
                class="ml-auto flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition-colors"
              >
                <XIcon class="h-3.5 w-3.5" />
              </button>
            </div>
          )}
        </For>

        <Show when={addingRule()}>
          <div class="rounded-md border border-primary/30 bg-primary/5 p-3 space-y-2">
            <div class="flex flex-wrap gap-2 items-center">
              <select
                value={draftKind()}
                onChange={e => setDraftKind(e.currentTarget.value as "exclude" | "only")}
                class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
              >
                <option value="exclude">Exclude when</option>
                <option value="only">Only when</option>
              </select>
              <select
                value={draftVersions()[0] ?? ""}
                onChange={e => setDraftVersions(e.currentTarget.value ? [e.currentTarget.value] : [])}
                class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
              >
                <option value="">Any version</option>
                <For each={showSnapshots() ? mcWithSnapshots() : minecraftVersions()}>
                  {version => <option value={version}>{version}</option>}
                </For>
              </select>
              <select
                value={draftLoader()}
                onChange={e => setDraftLoader(e.currentTarget.value)}
                class="rounded border border-border bg-input px-2 py-1 text-xs text-foreground"
              >
                <For each={ALL_LOADERS}>{loader => <option value={loader}>{loader === "any" ? "Any loader" : loader}</option>}</For>
              </select>
            </div>
            <div class="flex gap-2">
              <button
                onClick={commitRule}
                class="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90"
              >
                Add
              </button>
              <button
                onClick={() => { setAddingRule(false); setDraftVersions([]); }}
                class="rounded-md bg-secondary px-3 py-1 text-xs text-secondary-foreground hover:bg-secondary/80"
              >
                Cancel
              </button>
            </div>
          </div>
        </Show>

        <div class="flex items-center justify-between">
          <Show when={!addingRule()}>
            <button
              onClick={() => setAddingRule(true)}
              class="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
            >
              <MaterialIcon name="add" size="sm" />
              Add Rule
            </button>
          </Show>
          <label class="flex items-center gap-1 cursor-pointer select-none ml-auto">
            <input
              type="checkbox"
              checked={showSnapshots()}
              onChange={e => setShowSnapshots(e.currentTarget.checked)}
              class="accent-accentColor w-3 h-3"
            />
            <span class="text-xs text-muted-foreground">Show Snapshots</span>
          </label>
        </div>
      </div>
    </div>
  );
}
