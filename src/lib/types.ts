// ── Mod-list data types ───────────────────────────────────────────────────────

export type ModRow = {
  id: string;
  name: string;
  /** Modrinth project slug — present for Modrinth mods, absent for local mods. Used for icon fetching. */
  modrinth_id?: string;
  kind: "modrinth" | "local";
  area: string;
  note: string;
  tags: string[];
  alternatives?: ModRow[];
};

export type ModListCard = {
  name: string;
  status: "Ready" | "Resolving" | "Offline";
  accent: string;
  description: string;
  iconImage?: string;
  iconLabel?: string;
  iconAccent?: string;
  mcVersion?: string;
  modLoader?: string;
};

export type ModrinthResult = {
  id: string;
  name: string;
  author: string;
  description: string;
  categories: string[];
};

export type AestheticGroup = {
  id: string;
  name: string;
  collapsed: boolean;
  blockIds: string[];
  scopeRowId?: string | null;
};

export type FunctionalGroup = {
  id: string;
  name: string;
  tone: string;
  modIds: string[];
};

export type IncompatibilityRule = {
  winnerId: string;
  loserId: string;
};

export type LinkRule = {
  /** The mod that requires the other */
  fromId: string;
  /** The mod being required */
  toId: string;
};

export type VersionRule = {
  id: string;
  modId: string;
  kind: 'exclude' | 'only';
  mcVersions: string[];
  loader: string;
};

export type CustomConfig = {
  id: string;
  modId: string;
  mcVersions: string[];
  loader: string;
  targetPath: string;
  files: string[];
};

export type DownloadProgressItem = {
  filename: string;
  progress: number;
  status: "queued" | "downloading" | "complete";
};

export type LauncherUiError = {
  id: string;
  title: string;
  message: string;
  detail: string;
  severity: "warning" | "error";
  scope: "launch" | "download" | "account";
};

export type AccountSummary = {
  id: string;
  gamertag: string;
  email: string;
  status: "online" | "offline";
  lastMode: "microsoft" | "offline";
};

export type LaunchResolutionStage = {
  label: string;
  detail: string;
  progress: number;
};

// ── Tauri IPC payloads ────────────────────────────────────────────────────────

export type ShellSnapshot = {
  modlists: Array<{
    name: string;
    description: string;
    author?: string | null;
    rule_count: number;
  }>;
  active_account?: {
    microsoft_id: string;
    xbox_gamertag?: string | null;
    status: "online" | "offline";
    last_mode: "microsoft" | "offline";
  } | null;
  global_settings: {
    min_ram_mb: number;
    max_ram_mb: number;
    custom_jvm_args: string;
    profiler_enabled: boolean;
    wrapper_command: string;
    java_path_override: string;
  };
  selected_modlist_overrides: {
    modlist_name?: string | null;
    min_ram_mb?: number | null;
    max_ram_mb?: number | null;
    custom_jvm_args?: string | null;
    profiler_enabled?: boolean | null;
    wrapper_command?: string | null;
    minecraft_version?: string | null;
    mod_loader?: string | null;
  };
};

export type EditorSnapshot = {
  modlist_name: string;
  rows: ModRow[];
  incompatibilities: IncompatibilityRule[];
  groups: Array<{ id: string; name: string; collapsed: boolean; blockIds: string[] }>;
};

export type LaunchProgressEvent = {
  state: "idle" | "resolving" | "ready" | "running";
  progress: number;
  stage: string;
  detail: string;
};

export type ProcessLogEvent = {
  stream: "stdout" | "stderr";
  line: string;
};

export type ProcessExitEvent = {
  success: boolean;
  exitCode?: number | null;
};

// ── Static constants ──────────────────────────────────────────────────────────

export const MOD_LOADERS = ["Fabric", "NeoForge", "Forge", "Quilt", "Vanilla"] as const;

export const LAUNCH_STAGES: LaunchResolutionStage[] = [
  { label: "Resolve Rules",   detail: "Evaluating Mod-list rules, exclusions and fallback order.", progress: 18 },
  { label: "Check Cache",     detail: "Inspecting cached JARs and dependency records before download planning.", progress: 41 },
  { label: "Prepare Instance",detail: "Refreshing symlinks, configs and launch metadata for the selected target.", progress: 73 },
  { label: "Launch Ready",    detail: "Java runtime, loader profile and launch command are ready to hand off.", progress: 100 },
];
