import { For, Show, createSignal } from "solid-js";
import { MINECRAFT_VERSIONS, MOD_LOADERS } from "../lib/types";
import {
  selectedMcVersion, setSelectedMcVersion,
  selectedModLoader, setSelectedModLoader,
  launchState, launchProgress, activeLaunchStage,
  launchLogs, logViewerOpen, setLogViewerOpen,
  activeAccount, accounts, setAccountsModalOpen,
} from "../store";
import { MaterialIcon, Loader2Icon, XIcon } from "./icons";

interface LaunchPanelProps {
  onLaunch: () => void;
  onSwitchAccount: (id: string) => Promise<void>;
}

export function LaunchPanel(props: LaunchPanelProps) {
  const isLaunching = () => launchState() === "resolving" || launchState() === "running";
  const [accountMenuOpen, setAccountMenuOpen] = createSignal(false);

  return (
    <div class="shrink-0 border-t border-borderColor bg-bgPanel shadow-glow z-10">

      {/* ── Log viewer ─────────────────────────────────────────────── */}
      <Show when={logViewerOpen()}>
        <div class="border-b border-borderColor">
          <div class="flex items-center justify-between bg-bgHover px-4 py-2">
            <div class="flex items-center gap-2 text-sm font-medium text-textMain">
              <MaterialIcon name="terminal" size="md" />
              Launch Log
            </div>
            <button
              onClick={() => setLogViewerOpen(false)}
              class="flex h-6 w-6 items-center justify-center rounded-md text-textMuted transition-colors hover:bg-bgHover hover:text-white"
            >
              <XIcon class="h-4 w-4" />
            </button>
          </div>
          <div class="h-48 overflow-y-auto bg-bgDark px-3 py-2 font-mono text-xs">
            <Show
              when={launchLogs().length > 0}
              fallback={<p class="text-textMuted">Launch logs will appear here...</p>}
            >
              <For each={launchLogs()}>
                {(line, i) => (
                  <div class={`flex gap-3 border-b border-borderColor/20 py-1 last:border-b-0 ${
                    line.includes("✓") ? "text-success" :
                    line.includes("✗") ? "text-destructive" :
                    line.startsWith("[Cubic]") || line.startsWith("[Launcher]") || line.startsWith("[Resolver]") ? "text-primary" :
                    "text-textMuted"
                  }`}>
                    <span class="shrink-0 select-none text-primary/50">{String(i() + 1).padStart(2, "0")}</span>
                    <span class="break-all">{line || "\u00A0"}</span>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </div>
      </Show>

      {/* ── Progress bar ───────────────────────────────────────────── */}
      <Show when={isLaunching()}>
        <div class="px-6 py-2">
          <div class="mb-1 flex items-center justify-between text-xs text-textMuted">
             <span>
               {launchState() === "resolving" ? "Resolving mods..." :
               launchState() === "running"   ? "Minecraft running..." :
                launchState() === "ready"     ? "Launch ready"   :
                                                "Idle"}
             </span>
            <span>{activeLaunchStage().label}</span>
          </div>
          <div class="h-1 overflow-hidden rounded-full bg-bgHover">
            <div
              class="h-full rounded-full bg-primary transition-all duration-300"
              style={{ width: `${launchProgress()}%` }}
            />
          </div>
        </div>
      </Show>

      {/* ── Main BottomBar layout ────────────────────────────────── */}
      <div class="h-24 px-6 flex items-center justify-between">

        {/* Left side: Account + Loader info */}
        <div class="flex items-center gap-6">
          {/* Account selector */}
          <div class="relative">
            <button
              onClick={() => setAccountMenuOpen(v => !v)}
              class="flex items-center gap-3 cursor-pointer hover:bg-bgHover p-2 rounded-lg transition-colors duration-75 border border-transparent hover:border-borderColor"
            >
              <div class="w-10 h-10 rounded-full border border-borderColor bg-primary/20 flex items-center justify-center text-primary font-bold">
                {(activeAccount()?.gamertag ?? "?").slice(0, 2).toUpperCase()}
              </div>
              <div class="flex flex-col">
                <span class="text-sm font-semibold text-white">
                  {activeAccount()?.gamertag ?? "Not logged in"}
                </span>
                <span class="text-xs text-textMuted flex items-center gap-1">
                  Microsoft Account
                  <MaterialIcon name="expand_more" size="sm" />
                </span>
              </div>
            </button>

            {/* Account dropdown menu */}
            <Show when={accountMenuOpen()}>
              <div class="absolute bottom-full left-0 mb-2 w-64 rounded-lg border border-borderColor bg-bgPanel py-2 shadow-lg z-50">
                <For each={accounts()}>
                  {(account) => (
                    <button
                      onClick={() => {
                        void props.onSwitchAccount(account.id);
                        setAccountMenuOpen(false);
                      }}
                      class={`flex w-full items-center gap-3 px-4 py-2 text-sm transition-colors hover:bg-bgHover ${
                        account.id === activeAccount()?.id ? "text-primary" : "text-textMain"
                      }`}
                    >
                      <div class="w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center text-primary font-bold text-xs">
                        {account.gamertag.slice(0, 2).toUpperCase()}
                      </div>
                      <span class="flex-1 text-left truncate">{account.gamertag}</span>
                      <Show when={account.id === activeAccount()?.id}>
                        <MaterialIcon name="check" size="sm" class="text-primary" />
                      </Show>
                    </button>
                  )}
                </For>
                <div class="my-1 border-t border-borderColor" />
                <button
                  onClick={() => { setAccountsModalOpen(true); setAccountMenuOpen(false); }}
                  class="flex w-full items-center gap-3 px-4 py-2 text-sm text-textMain transition-colors hover:bg-bgHover"
                >
                  <MaterialIcon name="add" size="md" />
                  <span>Manage Accounts</span>
                </button>
              </div>
            </Show>
          </div>

          {/* Divider */}
          <div class="h-8 w-px bg-borderColor hidden md:block" />

          {/* Loader + Version info */}
          <div class="hidden md:flex flex-col">
            <div class="flex items-center gap-2 text-sm text-textMain font-medium">
              <MaterialIcon name="extension" size="sm" class="opacity-70" />
              <select
                value={selectedModLoader()}
                onChange={e => setSelectedModLoader(e.currentTarget.value)}
                class="bg-transparent border-none text-sm text-textMain font-medium focus:outline-none cursor-pointer"
              >
                <For each={MOD_LOADERS as unknown as string[]}>
                  {l => <option value={l} class="bg-bgPanel">{l} Loader</option>}
                </For>
              </select>
            </div>
            <div class="flex items-center gap-1">
              <span class="text-xs text-textMuted">Minecraft</span>
              <select
                value={selectedMcVersion()}
                onChange={e => setSelectedMcVersion(e.currentTarget.value)}
                class="bg-transparent border-none text-xs text-textMuted focus:outline-none cursor-pointer"
              >
                <For each={MINECRAFT_VERSIONS as unknown as string[]}>
                  {v => <option value={v} class="bg-bgPanel">{v}</option>}
                </For>
              </select>
            </div>
          </div>
        </div>

        {/* Right side: Log button + Play button */}
        <div class="flex items-center gap-4">
          {/* Log toggle */}
          <button
            onClick={() => setLogViewerOpen(v => !v)}
            class={`w-10 h-10 rounded-full bg-bgDark border border-borderColor flex items-center justify-center transition-colors duration-75 ${
              logViewerOpen()
                ? "text-primary border-primary"
                : "text-textMuted hover:text-white hover:bg-bgHover"
            }`}
            title="Show Log"
          >
            <MaterialIcon name="terminal" size="md" />
          </button>

          {/* Play button */}
          <button
            onClick={props.onLaunch}
            disabled={isLaunching()}
            class="px-10 py-3 bg-primary hover:bg-brandPurpleHover text-white font-bold text-lg rounded-lg shadow-lg flex items-center gap-3 transition-colors duration-75 disabled:opacity-70 disabled:cursor-not-allowed"
          >
            <Show
              when={isLaunching()}
              fallback={
                <>
                  <MaterialIcon name="play_arrow" size="lg" />
                  PLAY
                </>
              }
            >
              <Loader2Icon class="h-6 w-6 animate-spin" />
              {launchState() === "running" ? "RUNNING" : `${launchProgress()}%`}
            </Show>
          </button>
        </div>
      </div>
    </div>
  );
}
