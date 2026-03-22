"use client"

import { create } from "zustand"
import type { ModList, ModLoader, Account, LaunchState, ModRule, AestheticGroup } from "./types"
import { mockModLists, mockAccounts } from "./mock-data"

interface LauncherState {
  // Mod Lists
  modLists: ModList[]
  activeModListId: string | null
  
  // Launch Configuration
  selectedVersion: string
  selectedLoader: ModLoader
  
  // Accounts
  accounts: Account[]
  
  // UI State
  selectedRuleIds: string[]
  launchState: LaunchState
  showLogViewer: boolean
  searchQuery: string
  
  // Actions
  setActiveModList: (id: string) => void
  setSelectedVersion: (version: string) => void
  setSelectedLoader: (loader: ModLoader) => void
  toggleRuleSelection: (ruleId: string) => void
  clearSelection: () => void
  selectAllRules: () => void
  deleteSelectedRules: () => void
  toggleRuleExpanded: (ruleId: string) => void
  toggleGroupCollapsed: (groupId: string) => void
  createAestheticGroup: (name: string) => void
  renameAestheticGroup: (groupId: string, name: string) => void
  deleteAestheticGroup: (groupId: string) => void
  addRuleToGroup: (ruleId: string, groupId: string) => void
  removeRuleFromGroup: (ruleId: string) => void
  addRule: (rule: ModRule) => void
  setSearchQuery: (query: string) => void
  setShowLogViewer: (show: boolean) => void
  addLogLine: (line: string) => void
  setLaunchState: (state: Partial<LaunchState>) => void
  switchAccount: (accountId: string) => void
  createModList: (name: string, description: string) => void
  deleteModList: (id: string) => void
}

export const useLauncherStore = create<LauncherState>((set, get) => ({
  // Initial State
  modLists: mockModLists,
  activeModListId: mockModLists[0]?.id ?? null,
  selectedVersion: "1.21.1",
  selectedLoader: "fabric",
  accounts: mockAccounts,
  selectedRuleIds: [],
  launchState: {
    status: "idle",
    logs: [],
  },
  showLogViewer: false,
  searchQuery: "",
  
  // Actions
  setActiveModList: (id) => set({ activeModListId: id, selectedRuleIds: [] }),
  
  setSelectedVersion: (version) => set({ selectedVersion: version }),
  
  setSelectedLoader: (loader) => set({ selectedLoader: loader }),
  
  toggleRuleSelection: (ruleId) => set((state) => ({
    selectedRuleIds: state.selectedRuleIds.includes(ruleId)
      ? state.selectedRuleIds.filter(id => id !== ruleId)
      : [...state.selectedRuleIds, ruleId]
  })),
  
  clearSelection: () => set({ selectedRuleIds: [] }),
  
  selectAllRules: () => {
    const state = get()
    const activeModList = state.modLists.find(ml => ml.id === state.activeModListId)
    if (!activeModList) return
    
    const allRuleIds = activeModList.rules.map(r => r.id)
    set({ selectedRuleIds: allRuleIds })
  },
  
  deleteSelectedRules: () => set((state) => {
    const activeModList = state.modLists.find(ml => ml.id === state.activeModListId)
    if (!activeModList) return state
    
    const newRules = activeModList.rules.filter(r => !state.selectedRuleIds.includes(r.id))
    const newGroups = activeModList.aestheticGroups.map(g => ({
      ...g,
      rules: g.rules.filter(r => !state.selectedRuleIds.includes(r.id))
    }))
    const newUngrouped = activeModList.ungroupedRules.filter(r => !state.selectedRuleIds.includes(r.id))
    
    return {
      modLists: state.modLists.map(ml =>
        ml.id === state.activeModListId
          ? { ...ml, rules: newRules, aestheticGroups: newGroups, ungroupedRules: newUngrouped }
          : ml
      ),
      selectedRuleIds: []
    }
  }),
  
  toggleRuleExpanded: (ruleId) => set((state) => ({
    modLists: state.modLists.map(ml => ({
      ...ml,
      rules: ml.rules.map(r =>
        r.id === ruleId ? { ...r, expanded: !r.expanded } : r
      ),
      aestheticGroups: ml.aestheticGroups.map(g => ({
        ...g,
        rules: g.rules.map(r =>
          r.id === ruleId ? { ...r, expanded: !r.expanded } : r
        )
      })),
      ungroupedRules: ml.ungroupedRules.map(r =>
        r.id === ruleId ? { ...r, expanded: !r.expanded } : r
      )
    }))
  })),
  
  toggleGroupCollapsed: (groupId) => set((state) => ({
    modLists: state.modLists.map(ml => ({
      ...ml,
      aestheticGroups: ml.aestheticGroups.map(g =>
        g.id === groupId ? { ...g, collapsed: !g.collapsed } : g
      )
    }))
  })),
  
  createAestheticGroup: (name) => set((state) => {
    const activeModList = state.modLists.find(ml => ml.id === state.activeModListId)
    if (!activeModList) return state
    
    const newGroup: AestheticGroup = {
      id: `group-${Date.now()}`,
      name,
      collapsed: false,
      rules: []
    }
    
    return {
      modLists: state.modLists.map(ml =>
        ml.id === state.activeModListId
          ? { ...ml, aestheticGroups: [...ml.aestheticGroups, newGroup] }
          : ml
      )
    }
  }),
  
  renameAestheticGroup: (groupId, name) => set((state) => ({
    modLists: state.modLists.map(ml => ({
      ...ml,
      aestheticGroups: ml.aestheticGroups.map(g =>
        g.id === groupId ? { ...g, name } : g
      )
    }))
  })),
  
  deleteAestheticGroup: (groupId) => set((state) => {
    const activeModList = state.modLists.find(ml => ml.id === state.activeModListId)
    if (!activeModList) return state
    
    const group = activeModList.aestheticGroups.find(g => g.id === groupId)
    if (!group) return state
    
    return {
      modLists: state.modLists.map(ml =>
        ml.id === state.activeModListId
          ? {
              ...ml,
              aestheticGroups: ml.aestheticGroups.filter(g => g.id !== groupId),
              ungroupedRules: [...ml.ungroupedRules, ...group.rules]
            }
          : ml
      )
    }
  }),
  
  addRuleToGroup: (ruleId, groupId) => set((state) => {
    const activeModList = state.modLists.find(ml => ml.id === state.activeModListId)
    if (!activeModList) return state
    
    const rule = activeModList.rules.find(r => r.id === ruleId)
    if (!rule) return state
    
    return {
      modLists: state.modLists.map(ml =>
        ml.id === state.activeModListId
          ? {
              ...ml,
              aestheticGroups: ml.aestheticGroups.map(g =>
                g.id === groupId
                  ? { ...g, rules: [...g.rules, rule] }
                  : { ...g, rules: g.rules.filter(r => r.id !== ruleId) }
              ),
              ungroupedRules: ml.ungroupedRules.filter(r => r.id !== ruleId)
            }
          : ml
      )
    }
  }),
  
  removeRuleFromGroup: (ruleId) => set((state) => {
    const activeModList = state.modLists.find(ml => ml.id === state.activeModListId)
    if (!activeModList) return state
    
    let removedRule: ModRule | undefined
    
    for (const group of activeModList.aestheticGroups) {
      const found = group.rules.find(r => r.id === ruleId)
      if (found) {
        removedRule = found
        break
      }
    }
    
    if (!removedRule) return state
    
    return {
      modLists: state.modLists.map(ml =>
        ml.id === state.activeModListId
          ? {
              ...ml,
              aestheticGroups: ml.aestheticGroups.map(g => ({
                ...g,
                rules: g.rules.filter(r => r.id !== ruleId)
              })),
              ungroupedRules: [...ml.ungroupedRules, removedRule!]
            }
          : ml
      )
    }
  }),
  
  addRule: (rule) => set((state) => ({
    modLists: state.modLists.map(ml =>
      ml.id === state.activeModListId
        ? {
            ...ml,
            rules: [...ml.rules, rule],
            ungroupedRules: [...ml.ungroupedRules, rule]
          }
        : ml
    )
  })),
  
  setSearchQuery: (query) => set({ searchQuery: query }),
  
  setShowLogViewer: (show) => set({ showLogViewer: show }),
  
  addLogLine: (line) => set((state) => ({
    launchState: {
      ...state.launchState,
      logs: [...state.launchState.logs, line]
    }
  })),
  
  setLaunchState: (newState) => set((state) => ({
    launchState: { ...state.launchState, ...newState }
  })),
  
  switchAccount: (accountId) => set((state) => ({
    accounts: state.accounts.map(a => ({
      ...a,
      isActive: a.id === accountId
    }))
  })),
  
  createModList: (name, description) => set((state) => {
    const activeAccount = state.accounts.find(a => a.isActive)
    const newModList: ModList = {
      id: `modlist-${Date.now()}`,
      modlist_name: name,
      author: activeAccount?.gamertag ?? "Unknown",
      description,
      rules: [],
      aestheticGroups: [],
      ungroupedRules: []
    }
    
    return {
      modLists: [...state.modLists, newModList],
      activeModListId: newModList.id
    }
  }),
  
  deleteModList: (id) => set((state) => {
    const newModLists = state.modLists.filter(ml => ml.id !== id)
    return {
      modLists: newModLists,
      activeModListId: newModLists[0]?.id ?? null
    }
  })
}))
