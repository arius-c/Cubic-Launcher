"use client"

import { useState } from "react"
import { 
  ChevronDown, 
  ChevronRight, 
  FolderOpen, 
  MoreHorizontal, 
  Pencil, 
  Trash2,
  Package
} from "lucide-react"
import { cn } from "@/lib/utils"
import { useLauncherStore } from "@/lib/store"
import { ActionBar } from "./action-bar"
import { ModRuleItem } from "./mod-rule-item"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Empty } from "@/components/ui/empty"
import type { AestheticGroup } from "@/lib/types"

interface ModListEditorProps {
  onAddMod: () => void
}

function AestheticGroupSection({ group }: { group: AestheticGroup }) {
  const { 
    toggleGroupCollapsed, 
    renameAestheticGroup, 
    deleteAestheticGroup,
    searchQuery 
  } = useLauncherStore()

  const [isEditing, setIsEditing] = useState(false)
  const [editName, setEditName] = useState(group.name)

  const filteredRules = searchQuery
    ? group.rules.filter(r => 
        r.rule_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        r.options[0]?.mods[0]?.name.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : group.rules

  if (searchQuery && filteredRules.length === 0) return null

  const handleRename = () => {
    if (editName.trim() && editName !== group.name) {
      renameAestheticGroup(group.id, editName.trim())
    }
    setIsEditing(false)
  }

  return (
    <div className="mb-4">
      <div className="flex items-center gap-2 px-2 py-1">
        <button
          onClick={() => toggleGroupCollapsed(group.id)}
          className="flex items-center gap-2 text-sm font-medium text-muted-foreground hover:text-foreground"
        >
          {group.collapsed ? (
            <ChevronRight className="h-4 w-4" />
          ) : (
            <ChevronDown className="h-4 w-4" />
          )}
          <FolderOpen className="h-4 w-4 text-primary" />
        </button>
        {isEditing ? (
          <input
            type="text"
            value={editName}
            onChange={(e) => setEditName(e.target.value)}
            onBlur={handleRename}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleRename()
              if (e.key === "Escape") {
                setEditName(group.name)
                setIsEditing(false)
              }
            }}
            className="flex-1 bg-transparent text-sm font-medium outline-none focus:ring-1 focus:ring-primary"
            autoFocus
          />
        ) : (
          <span 
            className="flex-1 text-sm font-medium text-foreground cursor-pointer"
            onClick={() => setIsEditing(true)}
          >
            {group.name}
          </span>
        )}
        <span className="text-xs text-muted-foreground">
          {group.rules.length} mods
        </span>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" className="h-6 w-6">
              <MoreHorizontal className="h-3.5 w-3.5" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => setIsEditing(true)}>
              <Pencil className="mr-2 h-4 w-4" />
              Rename
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem 
              className="text-destructive"
              onClick={() => deleteAestheticGroup(group.id)}
            >
              <Trash2 className="mr-2 h-4 w-4" />
              Delete Group
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      {!group.collapsed && (
        <div className="ml-2 mt-1 space-y-1 border-l-2 border-border/50 pl-4">
          {filteredRules.map((rule) => (
            <ModRuleItem key={rule.id} rule={rule} />
          ))}
          {filteredRules.length === 0 && !searchQuery && (
            <div className="py-4 text-center text-sm text-muted-foreground">
              No mods in this group. Drag mods here to organize.
            </div>
          )}
        </div>
      )}
    </div>
  )
}

export function ModListEditor({ onAddMod }: ModListEditorProps) {
  const { modLists, activeModListId, searchQuery } = useLauncherStore()

  const activeModList = modLists.find((ml) => ml.id === activeModListId)

  if (!activeModList) {
    return (
      <div className="flex flex-1 flex-col">
        <ActionBar onAddMod={onAddMod} />
        <div className="flex flex-1 items-center justify-center">
          <Empty className="max-w-md">
            <Empty.Icon>
              <Package className="h-10 w-10" />
            </Empty.Icon>
            <Empty.Title>No Mod List Selected</Empty.Title>
            <Empty.Description>
              Select a mod list from the sidebar or create a new one to get started.
            </Empty.Description>
          </Empty>
        </div>
      </div>
    )
  }

  const filteredUngroupedRules = searchQuery
    ? activeModList.ungroupedRules.filter(r => 
        r.rule_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        r.options[0]?.mods[0]?.name.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : activeModList.ungroupedRules

  const hasRules = activeModList.rules.length > 0
  const hasVisibleContent = activeModList.aestheticGroups.length > 0 || filteredUngroupedRules.length > 0

  return (
    <div className="flex flex-1 flex-col">
      <ActionBar onAddMod={onAddMod} />

      {/* Mod List Header */}
      <div className="border-b border-border bg-card/50 px-4 py-3">
        <h2 className="text-lg font-semibold text-foreground">
          {activeModList.modlist_name}
        </h2>
        <p className="text-sm text-muted-foreground">
          {activeModList.description}
        </p>
        <div className="mt-2 flex items-center gap-4 text-xs text-muted-foreground">
          <span>{activeModList.rules.length} rules</span>
          <span>·</span>
          <span>by {activeModList.author}</span>
        </div>
      </div>

      {/* Mod List Content */}
      <ScrollArea className="flex-1">
        <div className="p-4">
          {!hasRules ? (
            <Empty className="mx-auto max-w-md py-12">
              <Empty.Icon>
                <Package className="h-10 w-10" />
              </Empty.Icon>
              <Empty.Title>Empty Mod List</Empty.Title>
              <Empty.Description>
                Add mods from Modrinth or upload local JAR files to build your mod list.
              </Empty.Description>
              <Empty.Actions>
                <Button onClick={onAddMod}>
                  Add Your First Mod
                </Button>
              </Empty.Actions>
            </Empty>
          ) : (
            <>
              {/* Aesthetic Groups */}
              {activeModList.aestheticGroups.map((group) => (
                <AestheticGroupSection key={group.id} group={group} />
              ))}

              {/* Ungrouped Rules */}
              {filteredUngroupedRules.length > 0 && (
                <div className={cn(activeModList.aestheticGroups.length > 0 && "mt-4")}>
                  {activeModList.aestheticGroups.length > 0 && (
                    <div className="mb-2 flex items-center gap-2 px-2">
                      <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                        Ungrouped
                      </span>
                    </div>
                  )}
                  <div className="space-y-1">
                    {filteredUngroupedRules.map((rule) => (
                      <ModRuleItem key={rule.id} rule={rule} />
                    ))}
                  </div>
                </div>
              )}

              {/* No results */}
              {searchQuery && !hasVisibleContent && (
                <div className="py-12 text-center">
                  <p className="text-muted-foreground">
                    No mods found matching "{searchQuery}"
                  </p>
                </div>
              )}
            </>
          )}
        </div>
      </ScrollArea>
    </div>
  )
}
