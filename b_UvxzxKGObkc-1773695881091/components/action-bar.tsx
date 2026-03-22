"use client"

import { useState } from "react"
import { 
  Plus, 
  FolderPlus, 
  Search, 
  Trash2, 
  X,
  Link2,
  AlertTriangle
} from "lucide-react"
import { useLauncherStore } from "@/lib/store"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog"
import { Badge } from "@/components/ui/badge"

interface ActionBarProps {
  onAddMod: () => void
}

export function ActionBar({ onAddMod }: ActionBarProps) {
  const {
    selectedRuleIds,
    clearSelection,
    deleteSelectedRules,
    createAestheticGroup,
    searchQuery,
    setSearchQuery,
  } = useLauncherStore()

  const [groupDialogOpen, setGroupDialogOpen] = useState(false)
  const [groupName, setGroupName] = useState("")
  const [showSearch, setShowSearch] = useState(false)

  const hasSelection = selectedRuleIds.length > 0

  const handleCreateGroup = () => {
    if (groupName.trim()) {
      createAestheticGroup(groupName.trim())
      setGroupName("")
      setGroupDialogOpen(false)
    }
  }

  if (hasSelection) {
    return (
      <div className="flex h-12 items-center justify-between border-b border-border bg-card px-4">
        <div className="flex items-center gap-3">
          <Badge variant="secondary" className="gap-1">
            {selectedRuleIds.length} selected
          </Badge>
          <Button
            variant="ghost"
            size="sm"
            onClick={clearSelection}
            className="h-8 gap-1.5 text-muted-foreground"
          >
            <X className="h-3.5 w-3.5" />
            Clear
          </Button>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="secondary" size="sm" className="h-8 gap-1.5">
            <FolderPlus className="h-3.5 w-3.5" />
            Add to Group
          </Button>
          <Button variant="secondary" size="sm" className="h-8 gap-1.5">
            <Link2 className="h-3.5 w-3.5" />
            Set Alternatives
          </Button>
          <Button variant="secondary" size="sm" className="h-8 gap-1.5">
            <AlertTriangle className="h-3.5 w-3.5" />
            Incompatibilities
          </Button>
          <AlertDialog>
            <AlertDialogTrigger asChild>
              <Button 
                variant="ghost" 
                size="sm" 
                className="h-8 gap-1.5 text-destructive hover:bg-destructive/10 hover:text-destructive"
              >
                <Trash2 className="h-3.5 w-3.5" />
                Delete
              </Button>
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>Delete selected mods?</AlertDialogTitle>
                <AlertDialogDescription>
                  This will remove {selectedRuleIds.length} mod(s) from your mod list. 
                  If any deleted mods have alternatives, those alternatives will also be removed.
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>Cancel</AlertDialogCancel>
                <AlertDialogAction
                  onClick={deleteSelectedRules}
                  className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
                >
                  Delete
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-12 items-center justify-between border-b border-border bg-card px-4">
      <div className="flex items-center gap-2">
        <Button 
          size="sm" 
          className="h-8 gap-1.5"
          onClick={onAddMod}
        >
          <Plus className="h-3.5 w-3.5" />
          Add Mod
        </Button>
        <Dialog open={groupDialogOpen} onOpenChange={setGroupDialogOpen}>
          <DialogTrigger asChild>
            <Button variant="secondary" size="sm" className="h-8 gap-1.5">
              <FolderPlus className="h-3.5 w-3.5" />
              Create Group
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Create Aesthetic Group</DialogTitle>
              <DialogDescription>
                Groups are visual containers to organize your mods. They don't affect how mods are loaded.
              </DialogDescription>
            </DialogHeader>
            <div className="py-4">
              <Input
                placeholder="Group name..."
                value={groupName}
                onChange={(e) => setGroupName(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleCreateGroup()}
              />
            </div>
            <DialogFooter>
              <Button variant="secondary" onClick={() => setGroupDialogOpen(false)}>
                Cancel
              </Button>
              <Button onClick={handleCreateGroup} disabled={!groupName.trim()}>
                Create
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>
      <div className="flex items-center gap-2">
        {showSearch ? (
          <div className="flex items-center gap-2">
            <Input
              placeholder="Search mods..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="h-8 w-48"
              autoFocus
            />
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              onClick={() => {
                setShowSearch(false)
                setSearchQuery("")
              }}
            >
              <X className="h-4 w-4" />
            </Button>
          </div>
        ) : (
          <Button 
            variant="ghost" 
            size="sm" 
            className="h-8 gap-1.5"
            onClick={() => setShowSearch(true)}
          >
            <Search className="h-3.5 w-3.5" />
            Search
          </Button>
        )}
      </div>
    </div>
  )
}
