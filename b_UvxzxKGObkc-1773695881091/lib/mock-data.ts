import type { ModList, ModRule, Account, FunctionalGroup } from "./types"

export const mockAccounts: Account[] = [
  {
    id: "1",
    gamertag: "CubicPlayer",
    uuid: "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    avatar: undefined,
    isActive: true,
  },
  {
    id: "2",
    gamertag: "Steve_Builder",
    uuid: "b2c3d4e5-f6a7-8901-bcde-f12345678901",
    avatar: undefined,
    isActive: false,
  },
]

export const mockFunctionalGroups: FunctionalGroup[] = [
  {
    id: "fg-1",
    name: "Performance Core",
    color: "hsl(280, 80%, 60%)",
    modIds: ["sodium", "lithium", "phosphor"],
  },
  {
    id: "fg-2",
    name: "Create Suite",
    color: "hsl(300, 70%, 55%)",
    modIds: ["create", "flywheel"],
  },
]

const performanceRules: ModRule[] = [
  {
    id: "rule-1",
    rule_name: "Rendering Engine",
    expanded: false,
    options: [
      {
        mods: [
          {
            id: "sodium",
            source: "modrinth",
            name: "Sodium",
            description: "A modern rendering engine for Minecraft which greatly improves frame rates",
            icon: "https://cdn.modrinth.com/data/AANobbMI/icon.png",
            author: "jellysquid3",
            downloads: 45000000,
            gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
            loaders: ["fabric", "neoforge"],
          },
        ],
        fallback_strategy: "continue",
      },
      {
        mods: [
          {
            id: "rubidium",
            source: "modrinth",
            name: "Rubidium",
            description: "Sodium fork for Forge",
            author: "Asek3",
            downloads: 8500000,
            gameVersions: ["1.20.1", "1.19.4", "1.19.2", "1.18.2"],
            loaders: ["forge"],
          },
        ],
        fallback_strategy: "continue",
      },
      {
        mods: [
          {
            id: "optifine",
            source: "local",
            file_name: "optifine-manual.jar",
            name: "OptiFine",
            description: "Manual OptiFine installation",
            gameVersions: ["1.20.1"],
            loaders: ["forge"],
          },
          {
            id: "optifabric",
            source: "modrinth",
            name: "OptiFabric",
            description: "Allows OptiFine to run on Fabric",
            author: "modmuss50",
            downloads: 3200000,
            gameVersions: ["1.20.1", "1.19.4"],
            loaders: ["fabric"],
          },
        ],
        fallback_strategy: "continue",
      },
    ],
  },
  {
    id: "rule-2",
    rule_name: "Server Performance",
    expanded: false,
    options: [
      {
        mods: [
          {
            id: "lithium",
            source: "modrinth",
            name: "Lithium",
            description: "General-purpose optimization mod for Minecraft",
            icon: "https://cdn.modrinth.com/data/gvQqBUqZ/icon.png",
            author: "jellysquid3",
            downloads: 28000000,
            gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
            loaders: ["fabric", "neoforge"],
          },
        ],
        fallback_strategy: "continue",
      },
    ],
  },
  {
    id: "rule-3",
    rule_name: "Light Engine",
    expanded: false,
    options: [
      {
        mods: [
          {
            id: "starlight",
            source: "modrinth",
            name: "Starlight",
            description: "Rewrites the light engine to fix lighting performance and errors",
            author: "Spottedleaf",
            downloads: 12000000,
            gameVersions: ["1.21.1", "1.20.4", "1.20.1", "1.19.4"],
            loaders: ["fabric", "forge"],
          },
        ],
        fallback_strategy: "continue",
      },
      {
        mods: [
          {
            id: "phosphor",
            source: "modrinth",
            name: "Phosphor",
            description: "Lighting engine improvements",
            author: "jellysquid3",
            downloads: 9500000,
            gameVersions: ["1.19.2", "1.18.2", "1.16.5"],
            loaders: ["fabric"],
          },
        ],
        fallback_strategy: "continue",
      },
    ],
  },
]

const utilityRules: ModRule[] = [
  {
    id: "rule-4",
    rule_name: "Minimap",
    expanded: false,
    options: [
      {
        mods: [
          {
            id: "xaeros-minimap",
            source: "modrinth",
            name: "Xaero's Minimap",
            description: "Displays the nearby world terrain, players, mobs, entities",
            author: "xaero96",
            downloads: 35000000,
            gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
            loaders: ["fabric", "forge", "neoforge"],
          },
        ],
        fallback_strategy: "continue",
      },
      {
        mods: [
          {
            id: "journeymap",
            source: "modrinth",
            name: "JourneyMap",
            description: "Real-time mapping in game or in a web browser",
            author: "techbrew",
            downloads: 42000000,
            gameVersions: ["1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
            loaders: ["fabric", "forge", "neoforge"],
          },
        ],
        fallback_strategy: "continue",
      },
    ],
  },
  {
    id: "rule-5",
    rule_name: "World Map",
    expanded: false,
    options: [
      {
        mods: [
          {
            id: "xaeros-worldmap",
            source: "modrinth",
            name: "Xaero's World Map",
            description: "Adds a full screen world map",
            author: "xaero96",
            downloads: 28000000,
            gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
            loaders: ["fabric", "forge", "neoforge"],
          },
        ],
        fallback_strategy: "continue",
      },
    ],
  },
  {
    id: "rule-6",
    rule_name: "Inventory Management",
    expanded: false,
    options: [
      {
        mods: [
          {
            id: "mouse-tweaks",
            source: "modrinth",
            name: "Mouse Tweaks",
            description: "Enhances inventory management",
            author: "YaLTeR",
            downloads: 18000000,
            gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
            loaders: ["fabric", "forge", "neoforge"],
          },
        ],
        fallback_strategy: "continue",
      },
    ],
  },
]

const contentRules: ModRule[] = [
  {
    id: "rule-7",
    rule_name: "Tech Mod",
    expanded: false,
    options: [
      {
        mods: [
          {
            id: "create",
            source: "modrinth",
            name: "Create",
            description: "Aesthetics and Automation with rotating contraptions",
            icon: "https://cdn.modrinth.com/data/LNytGWDc/icon.png",
            author: "simibubi",
            downloads: 52000000,
            gameVersions: ["1.21.1", "1.20.1", "1.19.2", "1.18.2"],
            loaders: ["fabric", "forge", "neoforge"],
          },
        ],
        fallback_strategy: "continue",
      },
    ],
  },
  {
    id: "rule-8",
    rule_name: "Shaders Support",
    expanded: false,
    options: [
      {
        mods: [
          {
            id: "iris",
            source: "modrinth",
            name: "Iris Shaders",
            description: "Shader mod for Fabric, compatible with Sodium",
            icon: "https://cdn.modrinth.com/data/YL57xq9U/icon.png",
            author: "coderbot",
            downloads: 38000000,
            gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
            loaders: ["fabric", "neoforge"],
          },
        ],
        fallback_strategy: "continue",
      },
      {
        mods: [
          {
            id: "oculus",
            source: "modrinth",
            name: "Oculus",
            description: "Iris shader mod port for Forge",
            author: "Asek3",
            downloads: 6800000,
            gameVersions: ["1.20.1", "1.19.4", "1.19.2"],
            loaders: ["forge"],
          },
        ],
        fallback_strategy: "continue",
      },
    ],
  },
]

export const mockModLists: ModList[] = [
  {
    id: "modlist-1",
    modlist_name: "Performance Pack",
    author: "CubicPlayer",
    description: "Maximum FPS optimization for all Minecraft versions",
    rules: [...performanceRules],
    aestheticGroups: [
      {
        id: "group-1",
        name: "Core Performance",
        collapsed: false,
        rules: performanceRules.slice(0, 2),
      },
    ],
    ungroupedRules: performanceRules.slice(2),
  },
  {
    id: "modlist-2",
    modlist_name: "Exploration Pack",
    author: "CubicPlayer",
    description: "Maps, minimaps, and utility mods for adventuring",
    rules: [...utilityRules],
    aestheticGroups: [
      {
        id: "group-2",
        name: "Navigation",
        collapsed: false,
        rules: utilityRules.slice(0, 2),
      },
    ],
    ungroupedRules: utilityRules.slice(2),
  },
  {
    id: "modlist-3",
    modlist_name: "Content & Shaders",
    author: "CubicPlayer",
    description: "Create mod with beautiful shader support",
    rules: [...contentRules],
    aestheticGroups: [],
    ungroupedRules: contentRules,
  },
  {
    id: "modlist-4",
    modlist_name: "Ultimate Pack",
    author: "CubicPlayer",
    description: "Everything combined - performance, utilities, and content",
    rules: [...performanceRules, ...utilityRules, ...contentRules],
    aestheticGroups: [
      {
        id: "group-3",
        name: "Performance",
        collapsed: false,
        rules: performanceRules,
      },
      {
        id: "group-4",
        name: "Utilities",
        collapsed: false,
        rules: utilityRules,
      },
      {
        id: "group-5",
        name: "Content",
        collapsed: true,
        rules: contentRules,
      },
    ],
    ungroupedRules: [],
  },
]

// Mock Modrinth search results
export const mockModrinthMods = [
  {
    id: "fabric-api",
    name: "Fabric API",
    description: "Lightweight and modular API providing common hooks and intercompatibility measures",
    author: "modmuss50",
    downloads: 180000000,
    icon: "https://cdn.modrinth.com/data/P7dR8mSH/icon.png",
    gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
    loaders: ["fabric", "quilt"],
  },
  {
    id: "modmenu",
    name: "Mod Menu",
    description: "Adds a mod menu to view the list of mods you have installed",
    author: "Prospector",
    downloads: 95000000,
    gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
    loaders: ["fabric", "quilt"],
  },
  {
    id: "indium",
    name: "Indium",
    description: "Sodium addon providing support for the Fabric Rendering API",
    author: "comp500",
    downloads: 15000000,
    gameVersions: ["1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
    loaders: ["fabric"],
  },
  {
    id: "entityculling",
    name: "Entity Culling",
    description: "Using async path-tracing to skip rendering non-visible entities",
    author: "tr7zw",
    downloads: 22000000,
    gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
    loaders: ["fabric", "forge", "neoforge"],
  },
  {
    id: "immediatelyfast",
    name: "ImmediatelyFast",
    description: "Speed up immediate mode rendering in Minecraft",
    author: "RaphiMC",
    downloads: 12000000,
    gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4"],
    loaders: ["fabric", "forge", "neoforge"],
  },
  {
    id: "ferritecore",
    name: "FerriteCore",
    description: "Memory usage optimizations",
    author: "malte0811",
    downloads: 18000000,
    gameVersions: ["1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
    loaders: ["fabric", "forge", "neoforge"],
  },
  {
    id: "emi",
    name: "EMI",
    description: "A featureful and accessible item and recipe viewer",
    author: "emilyploszaj",
    downloads: 25000000,
    gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
    loaders: ["fabric", "forge", "neoforge"],
  },
  {
    id: "jade",
    name: "Jade",
    description: "Shows information about what you are looking at",
    author: "Snownee",
    downloads: 45000000,
    gameVersions: ["1.21.5", "1.21.4", "1.21.1", "1.20.6", "1.20.4", "1.20.1"],
    loaders: ["fabric", "forge", "neoforge"],
  },
]
