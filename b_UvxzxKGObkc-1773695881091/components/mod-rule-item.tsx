"use client"

import { useState } from "react"
import { ChevronRight, AlertTriangle, Package, GripVertical } from "lucide-react"
import { cn } from "@/lib/utils"
import { useLauncherStore } from "@/lib/store"
import type { ModRule, Mod } from "@/lib/types"
import { Checkbox } from "@/components/ui/checkbox"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"

interface ModRuleItemProps {
  rule: ModRule
  depth?: number
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

function ModIcon({ mod }: { mod: Mod }) {
  if (mod.icon) {
    return (
      <img 
        src={mod.icon} 
        alt={mod.name}
        className="h-8 w-8 rounded-md object-cover"
      />
    )
  }
  return (
    <div className="flex h-8 w-8 items-center justify-center rounded-md bg-muted">
      <Package className="h-4 w-4 text-muted-foreground" />
    </div>
  )
}

export function ModRuleItem({ rule, depth = 0 }: ModRuleItemProps) {
  const { 
    selectedRuleIds, 
    toggleRuleSelection, 
    toggleRuleExpanded,
    selectedVersion,
    selectedLoader 
  } = useLauncherStore()

  const isSelected = selectedRuleIds.includes(rule.id)
  const primaryMod = rule.options[0]?.mods[0]
  const hasAlternatives = rule.options.length > 1

  // Check if primary mod is compatible with selected version/loader
  const isCompatible = primaryMod?.gameVersions?.includes(selectedVersion) && 
    primaryMod?.loaders?.includes(selectedLoader)

  const isLocalMod = primaryMod?.source === "local"
  const isGroupOption = (rule.options[0]?.mods.length ?? 0) > 1

  return (
    <div className={cn("group", depth > 0 && "border-l-2 border-border ml-4 pl-3")}>
      <div
        className={cn(
          "flex items-center gap-3 rounded-md px-2 py-2 transition-colors",
          isSelected 
            ? "bg-primary/10" 
            : "hover:bg-muted/50"
        )}
      >
        {/* Drag Handle */}
        <div className="cursor-grab opacity-0 transition-opacity group-hover:opacity-100">
          <GripVertical className="h-4 w-4 text-muted-foreground" />
        </div>

        {/* Checkbox */}
        <Checkbox
          checked={isSelected}
          onCheckedChange={() => toggleRuleSelection(rule.id)}
          className="border-muted-foreground"
        />

        {/* Mod Icon */}
        {primaryMod && <ModIcon mod={primaryMod} />}

        {/* Mod Info */}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate font-medium text-foreground">
              {rule.rule_name}
            </span>
            {isGroupOption && (
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger>
                    <Badge variant="secondary" className="h-5 text-xs">
                      Group ({rule.options[0]?.mods.length})
                    </Badge>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Functional group: all mods must be compatible</p>
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            )}
            {isLocalMod && (
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger>
                    <Badge variant="outline" className="h-5 gap-1 text-xs">
                      <AlertTriangle className="h-3 w-3 text-warning" />
                      Local
                    </Badge>
                  </TooltipTrigger>
                  <TooltipContent>
                    <p>Manual mod: verify dependencies yourself</p>
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            )}
            {!isCompatible && primaryMod && (
              <Badge 
                variant="outline" 
                className="h-5 border-destructive/50 text-xs text-destructive"
              >
                Incompatible
              </Badge>
            )}
          </div>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            {primaryMod?.author && <span>by {primaryMod.author}</span>}
            {primaryMod?.downloads && (
              <>
                <span>·</span>
                <span>{formatDownloads(primaryMod.downloads)} downloads</span>
              </>
            )}
          </div>
        </div>

        {/* Functional Group Tags */}
        <div className="flex items-center gap-1">
          {/* Placeholder for functional group tags */}
        </div>

        {/* Alternatives Expander */}
        {hasAlternatives && (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1 text-xs text-muted-foreground"
            onClick={() => toggleRuleExpanded(rule.id)}
          >
            <ChevronRight 
              className={cn(
                "h-3.5 w-3.5 transition-transform",
                rule.expanded && "rotate-90"
              )} 
            />
            {rule.options.length - 1} alternatives
          </Button>
        )}
      </div>

      {/* Alternatives (expanded) */}
      {rule.expanded && hasAlternatives && (
        <div className="mt-1 space-y-1">
          {rule.options.slice(1).map((option, index) => {
            const altMod = option.mods[0]
            if (!altMod) return null

            const altCompatible = altMod.gameVersions?.includes(selectedVersion) && 
              altMod.loaders?.includes(selectedLoader)

            return (
              <div
                key={`${rule.id}-alt-${index}`}
                className="ml-12 flex items-center gap-3 rounded-md px-2 py-1.5 text-sm text-muted-foreground hover:bg-muted/30"
              >
                <div className="h-px w-4 bg-border" />
                {altMod.icon ? (
                  <img 
                    src={altMod.icon} 
                    alt={altMod.name}
                    className="h-6 w-6 rounded object-cover"
                  />
                ) : (
                  <div className="flex h-6 w-6 items-center justify-center rounded bg-muted">
                    <Package className="h-3 w-3" />
                  </div>
                )}
                <span className={cn(!altCompatible && "line-through")}>
                  {altMod.name}
                </span>
                {option.mods.length > 1 && (
                  <Badge variant="secondary" className="h-4 text-[10px]">
                    +{option.mods.length - 1}
                  </Badge>
                )}
                {!altCompatible && (
                  <Badge 
                    variant="outline" 
                    className="h-4 border-destructive/50 text-[10px] text-destructive"
                  >
                    N/A
                  </Badge>
                )}
                <span className="ml-auto text-xs">
                  {option.fallback_strategy === "abort" ? "Stop if fails" : "Continue"}
                </span>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
