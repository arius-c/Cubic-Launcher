"use client"

import { useState, useEffect } from "react"
import { Play, Terminal, X, Settings, Loader2 } from "lucide-react"
import { cn } from "@/lib/utils"
import { useLauncherStore } from "@/lib/store"
import { MINECRAFT_VERSIONS, MOD_LOADERS } from "@/lib/types"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Progress } from "@/components/ui/progress"

export function LaunchPanel() {
  const {
    selectedVersion,
    setSelectedVersion,
    selectedLoader,
    setSelectedLoader,
    launchState,
    setLaunchState,
    showLogViewer,
    setShowLogViewer,
    addLogLine,
    modLists,
    activeModListId,
  } = useLauncherStore()

  const activeModList = modLists.find(ml => ml.id === activeModListId)

  const handleLaunch = async () => {
    if (!activeModList || launchState.status !== "idle") return

    // Simulate launch sequence
    setLaunchState({ status: "resolving", progress: 0, logs: [] })
    addLogLine(`[Cubic] Starting launch sequence...`)
    addLogLine(`[Cubic] Target: Minecraft ${selectedVersion} with ${selectedLoader}`)
    addLogLine(`[Cubic] Mod list: ${activeModList.modlist_name}`)
    addLogLine(`[Cubic] Processing ${activeModList.rules.length} rules...`)

    await new Promise(r => setTimeout(r, 500))

    // Simulate resolution
    for (let i = 0; i < activeModList.rules.length; i++) {
      const rule = activeModList.rules[i]
      setLaunchState({ 
        progress: (i / activeModList.rules.length) * 50,
        currentMod: rule.rule_name 
      })
      addLogLine(`[Cubic] Resolving: ${rule.rule_name}`)
      
      // Simulate checking options
      const primaryMod = rule.options[0]?.mods[0]
      if (primaryMod) {
        const isCompatible = primaryMod.gameVersions?.includes(selectedVersion) && 
          primaryMod.loaders?.includes(selectedLoader)
        
        if (isCompatible) {
          addLogLine(`[Cubic]   ✓ ${primaryMod.name} is compatible`)
        } else if (rule.options.length > 1) {
          addLogLine(`[Cubic]   ✗ ${primaryMod.name} not available, checking alternatives...`)
          const alt = rule.options[1]?.mods[0]
          if (alt) {
            addLogLine(`[Cubic]   ✓ Using alternative: ${alt.name}`)
          }
        } else {
          addLogLine(`[Cubic]   ✗ ${primaryMod.name} not compatible, skipping rule`)
        }
      }
      await new Promise(r => setTimeout(r, 200))
    }

    setLaunchState({ status: "downloading", progress: 50 })
    addLogLine(`[Cubic] Checking mod cache...`)
    await new Promise(r => setTimeout(r, 300))
    addLogLine(`[Cubic] All mods found in cache`)

    setLaunchState({ progress: 70 })
    addLogLine(`[Cubic] Creating symlinks...`)
    await new Promise(r => setTimeout(r, 200))
    addLogLine(`[Cubic] Symlinks created in instances/${selectedVersion}-${selectedLoader}/mods/`)

    setLaunchState({ status: "launching", progress: 85 })
    addLogLine(`[Cubic] Detecting Java runtime...`)
    await new Promise(r => setTimeout(r, 200))
    addLogLine(`[Cubic] Found Java 21 at /usr/lib/jvm/java-21-openjdk/bin/java`)

    addLogLine(`[Cubic] Fetching Fabric loader libraries...`)
    await new Promise(r => setTimeout(r, 300))
    
    setLaunchState({ progress: 95 })
    addLogLine(`[Cubic] Constructing launch command...`)
    await new Promise(r => setTimeout(r, 200))
    
    setLaunchState({ status: "running", progress: 100 })
    addLogLine(`[Cubic] Launching Minecraft ${selectedVersion}...`)
    addLogLine(``)
    addLogLine(`[Minecraft] Loading Minecraft ${selectedVersion} with Fabric Loader`)
    
    // Simulate some minecraft logs
    await new Promise(r => setTimeout(r, 500))
    addLogLine(`[Minecraft] Loading mods...`)
    await new Promise(r => setTimeout(r, 300))
    addLogLine(`[Minecraft] Sodium loaded`)
    addLogLine(`[Minecraft] Lithium loaded`)
    addLogLine(`[Minecraft] Iris Shaders loaded`)
    await new Promise(r => setTimeout(r, 200))
    addLogLine(`[Minecraft] All mods loaded successfully`)
    
    // Keep running state for demo
    await new Promise(r => setTimeout(r, 2000))
    setLaunchState({ status: "idle" })
    addLogLine(`[Cubic] Game closed`)
  }

  const isLaunching = launchState.status !== "idle"

  return (
    <div className="flex flex-col border-t border-border bg-card">
      {/* Log Viewer */}
      {showLogViewer && (
        <div className="border-b border-border">
          <div className="flex items-center justify-between bg-muted/50 px-4 py-2">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Terminal className="h-4 w-4" />
              Launch Log
            </div>
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6"
              onClick={() => setShowLogViewer(false)}
            >
              <X className="h-4 w-4" />
            </Button>
          </div>
          <ScrollArea className="h-48 bg-background font-mono text-xs">
            <div className="p-3">
              {launchState.logs.length === 0 ? (
                <div className="text-muted-foreground">
                  Launch logs will appear here...
                </div>
              ) : (
                launchState.logs.map((log, i) => (
                  <div 
                    key={i} 
                    className={cn(
                      "whitespace-pre-wrap",
                      log.includes("✓") && "text-success",
                      log.includes("✗") && "text-destructive",
                      log.includes("[Cubic]") && "text-primary",
                      log.includes("[Minecraft]") && "text-muted-foreground"
                    )}
                  >
                    {log || "\u00A0"}
                  </div>
                ))
              )}
            </div>
          </ScrollArea>
        </div>
      )}

      {/* Progress Bar */}
      {isLaunching && (
        <div className="px-4 py-2">
          <div className="flex items-center justify-between text-xs text-muted-foreground mb-1">
            <span>
              {launchState.status === "resolving" && "Resolving mods..."}
              {launchState.status === "downloading" && "Checking cache..."}
              {launchState.status === "launching" && "Preparing launch..."}
              {launchState.status === "running" && "Game running"}
            </span>
            {launchState.currentMod && (
              <span>{launchState.currentMod}</span>
            )}
          </div>
          <Progress value={launchState.progress} className="h-1" />
        </div>
      )}

      {/* Launch Controls */}
      <div className="flex items-center gap-4 p-4">
        <div className="flex items-center gap-2">
          <Select value={selectedVersion} onValueChange={setSelectedVersion}>
            <SelectTrigger className="h-9 w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {MINECRAFT_VERSIONS.map((version) => (
                <SelectItem key={version} value={version}>
                  {version}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select value={selectedLoader} onValueChange={setSelectedLoader}>
            <SelectTrigger className="h-9 w-28">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {MOD_LOADERS.map((loader) => (
                <SelectItem key={loader.value} value={loader.value}>
                  {loader.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-1 items-center justify-end gap-2">
          <Button
            variant="ghost"
            size="icon"
            className="h-9 w-9"
            onClick={() => setShowLogViewer(!showLogViewer)}
          >
            <Terminal className="h-4 w-4" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-9 w-9"
          >
            <Settings className="h-4 w-4" />
          </Button>
          <Button
            size="lg"
            className="h-10 gap-2 px-8"
            onClick={handleLaunch}
            disabled={!activeModList || isLaunching}
          >
            {isLaunching ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                {launchState.status === "running" ? "Running" : "Launching..."}
              </>
            ) : (
              <>
                <Play className="h-4 w-4" />
                Play
              </>
            )}
          </Button>
        </div>
      </div>
    </div>
  )
}
