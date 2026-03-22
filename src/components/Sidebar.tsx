import { For, Show } from "solid-js";
import {
  modListCards, selectedModListName,
  setCreateModlistModalOpen, minecraftVersions,
} from "../store";
import { MaterialIcon } from "./icons";

type HandleSelectModList = (name: string) => Promise<void>;

interface SidebarProps {
  onSelectModList: HandleSelectModList;
  onSwitchAccount: (id: string) => Promise<void>;
  onDeleteModList?: (name: string) => void;
}

export function Sidebar(props: SidebarProps) {
  return (
    <aside class="w-20 lg:w-64 border-r border-borderColor bg-bgPanel flex flex-col shrink-0 h-full">
      {/* Instances label */}
      <div class="p-3 lg:p-4 text-textMuted text-xs uppercase font-semibold tracking-wider hidden lg:block">
        Instances
      </div>

      {/* Scrollable mod list cards */}
      <div class="flex-1 overflow-y-auto scrollbar-hide py-2 flex flex-col gap-2 px-2 lg:px-3">
        <For each={modListCards()}>
          {(ml) => {
            const isActive = () => selectedModListName() === ml.name;

            return (
              <button
                onClick={() => void props.onSelectModList(ml.name)}
                class={`group flex items-center gap-3 p-2 rounded-lg cursor-pointer relative border-l-2 transition-colors duration-75 ${
                  isActive()
                    ? "bg-bgHover border-primary"
                    : "border-transparent hover:bg-bgHover"
                }`}
              >
                {/* Instance icon/thumbnail */}
                <div class={`w-10 h-10 rounded-lg shadow-sm flex-shrink-0 flex items-center justify-center overflow-hidden ${
                  ml.iconImage ? "" : (isActive() ? "bg-primary" : "bg-muted")
                }`}>
                  <Show when={ml.iconImage} fallback={
                    <span class="text-white font-bold text-sm">
                      {(ml.iconLabel || ml.name).slice(0, 2).toUpperCase()}
                    </span>
                  }>
                    <img src={ml.iconImage} class="block w-10 h-10 object-cover rounded-lg" alt="" />
                  </Show>
                </div>

                {/* Instance info (hidden on mobile) */}
                <div class="hidden lg:flex flex-col overflow-hidden">
                  <span class={`text-sm font-medium truncate transition-colors duration-75 ${
                    isActive() ? "text-white" : "text-textMuted group-hover:text-white"
                  }`}>
                    {ml.iconLabel || ml.name}
                  </span>
                  <span class="text-xs text-textMuted truncate">
                    {ml.modLoader || "Fabric"} {ml.mcVersion || minecraftVersions()[0] || "1.21.1"}
                  </span>
                </div>
              </button>
            );
          }}
        </For>

        {/* New Instance button */}
        <div class="mt-4 flex justify-center lg:justify-start lg:px-2">
          <button
            onClick={() => setCreateModlistModalOpen(true)}
            class="w-10 h-10 lg:w-full rounded-lg border border-dashed border-borderColor text-textMuted hover:text-white hover:border-textMuted hover:bg-bgHover transition-colors duration-75 flex items-center justify-center gap-2"
          >
            <MaterialIcon name="add" size="md" />
            <span class="text-sm font-medium hidden lg:block">New Instance</span>
          </button>
        </div>
      </div>
    </aside>
  );
}
