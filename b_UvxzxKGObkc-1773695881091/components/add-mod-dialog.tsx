"use client"

import { useState, useMemo } from "react"
import { 
  Search, 
  Download, 
  Package, 
  Upload, 
  Check,
  X,
  Loader2
} from "lucide-react"
import { cn } from "@/lib/utils"
import { useLauncherStore } from "@/lib/store"
import { mockModrinthMods } from "@/lib/mock-data"
import type { Mod, ModRule } from "@/lib/types"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Badge } from "@/components/ui/badge"

interface AddModDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

function formatDownloads(num: number): string {
  if (num >= 1000000) {
    return `${(num / 1000000).toFixed(1)}M`
  }
  if (num >= 1000) {
    return `${(num / 1000).toFixed(0)}K`
  }
  return num.toString()
}

interface ModSearchResultProps {
  mod: typeof mockModrinthMods[0]
  isAdded: boolean
  isAdding: boolean
  onAdd: () => void
}

function ModSearchResult({ mod, isAdded, isAdding, onAdd }: ModSearchResultProps) {
  return (
    <div className="flex items-start gap-3 rounded-md border border-border bg-card p-3 transition-colors hover:bg-muted/30">
      {mod.icon ? (
        <img 
          src={mod.icon} 
          alt={mod.name}
          className="h-12 w-12 rounded-md object-cover"
        />
      ) : (
        <div className="flex h-12 w-12 items-center justify-center rounded-md bg-muted">
          <Package className="h-6 w-6 text-muted-foreground" />
        </div>
      )}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <h4 className="font-medium text-foreground">{mod.name}</h4>
          <div className="flex items-center gap-1">
            {mod.loaders.slice(0, 3).map((loader) => (
              <Badge key={loader} variant="secondary" className="h-5 text-[10px]">
                {loader}
              </Badge>
            ))}
          </div>
        </div>
        <p className="mt-0.5 line-clamp-2 text-sm text-muted-foreground">
          {mod.description}
        </p>
        <div className="mt-1.5 flex items-center gap-3 text-xs text-muted-foreground">
          <span>by {mod.author}</span>
          <span>·</span>
          <span className="flex items-center gap-1">
            <Download className="h-3 w-3" />
            {formatDownloads(mod.downloads)}
          </span>
          <span>·</span>
          <span>{mod.gameVersions[0]}</span>
        </div>
      </div>
      <Button
        size="sm"
        variant={isAdded ? "secondary" : "default"}
        className="h-8 gap-1.5"
        onClick={onAdd}
        disabled={isAdded || isAdding}
      >
        {isAdding ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : isAdded ? (
          <>
            <Check className="h-3.5 w-3.5" />
            Added
          </>
        ) : (
          "Add"
        )}
      </Button>
    </div>
  )
}

export function AddModDialog({ open, onOpenChange }: AddModDialogProps) {
  const { addRule, modLists, activeModListId } = useLauncherStore()
  const [searchQuery, setSearchQuery] = useState("")
  const [addingMods, setAddingMods] = useState<Set<string>>(new Set())
  const [addedMods, setAddedMods] = useState<Set<string>>(new Set())

  const activeModList = modLists.find(ml => ml.id === activeModListId)
  
  // Get IDs of mods already in the mod list
  const existingModIds = useMemo(() => {
    if (!activeModList) return new Set<string>()
    const ids = new Set<string>()
    activeModList.rules.forEach(rule => {
      rule.options.forEach(option => {
        option.mods.forEach(mod => {
          ids.add(mod.id)
        })
      })
    })
    return ids
  }, [activeModList])

  const filteredMods = useMemo(() => {
    if (!searchQuery.trim()) return mockModrinthMods
    const query = searchQuery.toLowerCase()
    return mockModrinthMods.filter(mod =>
      mod.name.toLowerCase().includes(query) ||
      mod.description.toLowerCase().includes(query) ||
      mod.author.toLowerCase().includes(query)
    )
  }, [searchQuery])

  const handleAddMod = async (mod: typeof mockModrinthMods[0]) => {
    setAddingMods(prev => new Set(prev).add(mod.id))
    
    // Simulate API call delay
    await new Promise(r => setTimeout(r, 500))
    
    const newRule: ModRule = {
      id: `rule-${Date.now()}`,
      rule_name: mod.name,
      expanded: false,
      options: [{
        mods: [{
          id: mod.id,
          source: "modrinth",
          name: mod.name,
          description: mod.description,
          icon: mod.icon,
          author: mod.author,
          downloads: mod.downloads,
          gameVersions: mod.gameVersions,
          loaders: mod.loaders,
        } as Mod],
        fallback_strategy: "continue"
      }]
    }
    
    addRule(newRule)
    setAddingMods(prev => {
      const next = new Set(prev)
      next.delete(mod.id)
      return next
    })
    setAddedMods(prev => new Set(prev).add(mod.id))
  }

  const handleClose = () => {
    setSearchQuery("")
    setAddedMods(new Set())
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={handleClose}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Add Mod</DialogTitle>
          <DialogDescription>
            Search Modrinth for mods or upload a local JAR file.
          </DialogDescription>
        </DialogHeader>
        
        <Tabs defaultValue="modrinth" className="mt-2">
          <TabsList className="grid w-full grid-cols-2">
            <TabsTrigger value="modrinth" className="gap-2">
              <Search className="h-4 w-4" />
              Search Modrinth
            </TabsTrigger>
            <TabsTrigger value="local" className="gap-2">
              <Upload className="h-4 w-4" />
              Upload Local
            </TabsTrigger>
          </TabsList>
          
          <TabsContent value="modrinth" className="mt-4">
            <div className="relative">
              <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                placeholder="Search mods..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-10"
              />
            </div>
            
            <ScrollArea className="mt-4 h-[400px] pr-4">
              <div className="space-y-2">
                {filteredMods.map((mod) => (
                  <ModSearchResult
                    key={mod.id}
                    mod={mod}
                    isAdded={existingModIds.has(mod.id) || addedMods.has(mod.id)}
                    isAdding={addingMods.has(mod.id)}
                    onAdd={() => handleAddMod(mod)}
                  />
                ))}
                {filteredMods.length === 0 && (
                  <div className="flex flex-col items-center justify-center py-12 text-center">
                    <Package className="mb-4 h-12 w-12 text-muted-foreground/50" />
                    <p className="text-muted-foreground">
                      No mods found for "{searchQuery}"
                    </p>
                  </div>
                )}
              </div>
            </ScrollArea>
          </TabsContent>
          
          <TabsContent value="local" className="mt-4">
            <div className="flex flex-col items-center justify-center rounded-lg border-2 border-dashed border-border bg-muted/30 py-16">
              <Upload className="mb-4 h-12 w-12 text-muted-foreground/50" />
              <h4 className="mb-2 font-medium text-foreground">Upload JAR File</h4>
              <p className="mb-4 max-w-xs text-center text-sm text-muted-foreground">
                Drag and drop a mod JAR file here, or click to browse.
              </p>
              <Button variant="secondary">
                Browse Files
              </Button>
              <p className="mt-4 text-xs text-muted-foreground">
                You'll need to specify compatible Minecraft versions after upload.
              </p>
            </div>
          </TabsContent>
        </Tabs>
        
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="secondary" onClick={handleClose}>
            Done
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  )
}
