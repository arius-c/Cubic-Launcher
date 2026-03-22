"use client"

import { useState } from "react"
import { 
  Box, 
  Plus, 
  Settings, 
  ChevronDown, 
  User,
  LogOut,
  Trash2
} from "lucide-react"
import { cn } from "@/lib/utils"
import { useLauncherStore } from "@/lib/store"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { Field, FieldLabel } from "@/components/ui/field"
import { ScrollArea } from "@/components/ui/scroll-area"

export function LauncherSidebar() {
  const { 
    modLists, 
    activeModListId, 
    setActiveModList,
    accounts,
    switchAccount,
    createModList,
    deleteModList
  } = useLauncherStore()

  const [newModListOpen, setNewModListOpen] = useState(false)
  const [newModListName, setNewModListName] = useState("")
  const [newModListDesc, setNewModListDesc] = useState("")

  const activeAccount = accounts.find(a => a.isActive)

  const handleCreateModList = () => {
    if (newModListName.trim()) {
      createModList(newModListName.trim(), newModListDesc.trim())
      setNewModListName("")
      setNewModListDesc("")
      setNewModListOpen(false)
    }
  }

  return (
    <div className="flex h-full w-64 flex-col border-r border-border bg-sidebar">
      {/* Logo / Header */}
      <div className="flex h-14 items-center gap-2 border-b border-sidebar-border px-4">
        <div className="flex h-8 w-8 items-center justify-center rounded-md bg-primary">
          <Box className="h-4 w-4 text-primary-foreground" />
        </div>
        <span className="text-lg font-semibold text-sidebar-foreground">Cubic</span>
      </div>

      {/* Mod Lists */}
      <div className="flex-1 overflow-hidden">
        <div className="flex items-center justify-between px-4 py-3">
          <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground">
            Mod Lists
          </span>
          <Dialog open={newModListOpen} onOpenChange={setNewModListOpen}>
            <DialogTrigger asChild>
              <Button variant="ghost" size="icon" className="h-6 w-6">
                <Plus className="h-3.5 w-3.5" />
              </Button>
            </DialogTrigger>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>Create New Mod List</DialogTitle>
                <DialogDescription>
                  Create a version-agnostic mod list that works across all Minecraft versions.
                </DialogDescription>
              </DialogHeader>
              <div className="grid gap-4 py-4">
                <Field>
                  <FieldLabel>Name</FieldLabel>
                  <Input
                    placeholder="My Awesome Pack"
                    value={newModListName}
                    onChange={(e) => setNewModListName(e.target.value)}
                  />
                </Field>
                <Field>
                  <FieldLabel>Description</FieldLabel>
                  <Textarea
                    placeholder="A brief description of your mod list..."
                    value={newModListDesc}
                    onChange={(e) => setNewModListDesc(e.target.value)}
                    rows={3}
                  />
                </Field>
              </div>
              <DialogFooter>
                <Button variant="secondary" onClick={() => setNewModListOpen(false)}>
                  Cancel
                </Button>
                <Button onClick={handleCreateModList} disabled={!newModListName.trim()}>
                  Create
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </div>

        <ScrollArea className="h-[calc(100%-52px)]">
          <div className="px-2 pb-4">
            {modLists.map((modList) => (
              <button
                key={modList.id}
                onClick={() => setActiveModList(modList.id)}
                className={cn(
                  "group relative flex w-full items-center gap-3 rounded-md px-2 py-2 text-left transition-colors",
                  activeModListId === modList.id
                    ? "bg-sidebar-accent text-sidebar-accent-foreground"
                    : "text-sidebar-foreground hover:bg-sidebar-accent/50"
                )}
              >
                <div 
                  className={cn(
                    "flex h-9 w-9 shrink-0 items-center justify-center rounded-md",
                    activeModListId === modList.id
                      ? "bg-primary text-primary-foreground"
                      : "bg-muted text-muted-foreground"
                  )}
                >
                  <Box className="h-4 w-4" />
                </div>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">
                    {modList.modlist_name}
                  </div>
                  <div className="truncate text-xs text-muted-foreground">
                    {modList.rules.length} rules
                  </div>
                </div>
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6 opacity-0 transition-opacity group-hover:opacity-100"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <Settings className="h-3.5 w-3.5" />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem>Rename</DropdownMenuItem>
                    <DropdownMenuItem>Duplicate</DropdownMenuItem>
                    <DropdownMenuItem>Export</DropdownMenuItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem 
                      className="text-destructive"
                      onClick={() => deleteModList(modList.id)}
                    >
                      <Trash2 className="mr-2 h-4 w-4" />
                      Delete
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </button>
            ))}
          </div>
        </ScrollArea>
      </div>

      {/* Account Selector */}
      <div className="border-t border-sidebar-border p-2">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button className="flex w-full items-center gap-3 rounded-md p-2 text-left transition-colors hover:bg-sidebar-accent">
              <div className="flex h-8 w-8 items-center justify-center rounded-full bg-primary/20 text-primary">
                <User className="h-4 w-4" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium text-sidebar-foreground">
                  {activeAccount?.gamertag ?? "Not logged in"}
                </div>
                <div className="text-xs text-muted-foreground">
                  Microsoft Account
                </div>
              </div>
              <ChevronDown className="h-4 w-4 text-muted-foreground" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            {accounts.map((account) => (
              <DropdownMenuItem 
                key={account.id}
                onClick={() => switchAccount(account.id)}
                className={cn(
                  "flex items-center gap-2",
                  account.isActive && "bg-accent"
                )}
              >
                <User className="h-4 w-4" />
                <span>{account.gamertag}</span>
                {account.isActive && (
                  <span className="ml-auto text-xs text-primary">Active</span>
                )}
              </DropdownMenuItem>
            ))}
            <DropdownMenuSeparator />
            <DropdownMenuItem>
              <Plus className="mr-2 h-4 w-4" />
              Add Account
            </DropdownMenuItem>
            <DropdownMenuItem className="text-destructive">
              <LogOut className="mr-2 h-4 w-4" />
              Sign Out
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  )
}
