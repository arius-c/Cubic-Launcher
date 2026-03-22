"use client"

import { useState } from "react"
import { LauncherSidebar } from "@/components/launcher-sidebar"
import { ModListEditor } from "@/components/mod-list-editor"
import { LaunchPanel } from "@/components/launch-panel"
import { AddModDialog } from "@/components/add-mod-dialog"

export default function CubicLauncher() {
  const [addModOpen, setAddModOpen] = useState(false)

  return (
    <div className="flex h-screen bg-background">
      {/* Sidebar */}
      <LauncherSidebar />

      {/* Main Content */}
      <div className="flex flex-1 flex-col overflow-hidden">
        {/* Editor Area */}
        <ModListEditor onAddMod={() => setAddModOpen(true)} />

        {/* Launch Panel */}
        <LaunchPanel />
      </div>

      {/* Add Mod Dialog */}
      <AddModDialog open={addModOpen} onOpenChange={setAddModOpen} />
    </div>
  )
}
