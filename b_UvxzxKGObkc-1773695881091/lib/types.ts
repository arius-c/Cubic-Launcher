// Cubic Launcher Types based on Technical Documentation

export type ModSource = "modrinth" | "local"

export interface Mod {
  id: string
  source: ModSource
  file_name?: string // Only for local mods
  name: string
  description?: string
  icon?: string
  author?: string
  downloads?: number
  gameVersions?: string[]
  loaders?: string[]
}

export interface ModOption {
  mods: Mod[]
  fallback_strategy: "continue" | "abort"
  exclude_if_present?: string[]
}

export interface ModRule {
  id: string
  rule_name: string
  options: ModOption[]
  expanded?: boolean // UI state for showing alternatives
}

export interface AestheticGroup {
  id: string
  name: string
  collapsed: boolean
  rules: ModRule[]
}

export interface ModList {
  id: string
  modlist_name: string
  author: string
  description: string
  icon?: string
  rules: ModRule[]
  aestheticGroups: AestheticGroup[]
  ungroupedRules: ModRule[]
}

export interface FunctionalGroup {
  id: string
  name: string
  color: string
  modIds: string[]
}

export interface Instance {
  id: string
  modlistId: string
  minecraftVersion: string
  modLoader: ModLoader
  lastPlayed?: Date
}

export type ModLoader = "fabric" | "neoforge" | "forge" | "quilt"

export interface Account {
  id: string
  gamertag: string
  uuid: string
  avatar?: string
  isActive: boolean
}

export interface GlobalSettings {
  minRam: number
  maxRam: number
  jvmArguments: string
  javaPath?: string
  wrapperCommand?: string
}

export interface ModListSettings extends Partial<GlobalSettings> {
  modlistId: string
}

export interface DownloadProgress {
  modId: string
  modName: string
  progress: number
  total: number
}

export interface LaunchState {
  status: "idle" | "resolving" | "downloading" | "launching" | "running"
  progress?: number
  currentMod?: string
  logs: string[]
}

// Minecraft versions (subset for UI demo)
export const MINECRAFT_VERSIONS = [
  "1.21.5",
  "1.21.4",
  "1.21.3",
  "1.21.1",
  "1.20.6",
  "1.20.4",
  "1.20.1",
  "1.19.4",
  "1.19.2",
  "1.18.2",
  "1.16.5",
] as const

export const MOD_LOADERS: { value: ModLoader; label: string }[] = [
  { value: "fabric", label: "Fabric" },
  { value: "neoforge", label: "NeoForge" },
  { value: "forge", label: "Forge" },
  { value: "quilt", label: "Quilt" },
]
