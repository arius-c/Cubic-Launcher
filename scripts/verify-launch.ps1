param(
    [Parameter(Mandatory = $true)]
    [string]$ModlistName,

    [Parameter(Mandatory = $true)]
    [string]$MinecraftVersion,

    [string]$ModLoader = "Fabric",

    [int]$TimeoutSeconds = 45,

    [int]$SuccessAfterSeconds = 15,

    [int]$MinimumAliveSeconds = 10,

    [int]$MinimumWindowSeconds = 10,

    [ValidateSet("current", "on", "off")]
    [string]$CacheOnlyMode = "current",

    [switch]$KeepRunningOnSuccess,

    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$cargoManifest = Join-Path $repoRoot "src-tauri\Cargo.toml"
$debugExe = Join-Path $repoRoot "src-tauri\target\debug\cubic_launcher.exe"
$resultPath = Join-Path ([System.IO.Path]::GetTempPath()) ("cubic-launch-verify-" + [guid]::NewGuid().ToString() + ".json")
$process = $null
$launcherPid = $null
$result = $null

function Get-LogTail {
    param(
        [string]$Path,
        [int]$LineCount = 80
    )

    if (-not (Test-Path $Path)) {
        return @()
    }

    return @(Get-Content $Path -Tail $LineCount)
}

function Get-FailureInfo {
    param(
        [string[]]$Lines
    )

    $text = ($Lines -join "`n").ToLowerInvariant()

    if ($text.Contains("requires version") -and $text.Contains("which is missing") -and $text.Contains("mod '")) {
        return @{
            failureKind = "missing_dependency"
            failureSummary = "Fabric reported a missing required dependency in the selected mod set."
        }
    }

    if ($text.Contains("requires version") -and $text.Contains("java") -and $text.Contains("wrong version is present")) {
        return @{
            failureKind = "wrong_java"
            failureSummary = "A selected mod requires a newer Java runtime than the launcher used."
        }
    }

    if ($text.Contains("noclassdeffounderror") -or ($text.Contains("classnotfoundexception") -and $text.Contains("caused by:") -and -not $text.Contains("error loading class:"))) {
        return @{
            failureKind = "missing_class"
            failureSummary = "Minecraft crashed because a required class was missing at runtime."
        }
    }

    if ($text.Contains("nosuchmethoderror")) {
        return @{
            failureKind = "method_mismatch"
            failureSummary = "Minecraft crashed because two mods loaded incompatible method signatures."
        }
    }

    if ($text.Contains("could not execute entrypoint stage") -or $text.Contains("exception caught from launcher")) {
        return @{
            failureKind = "entrypoint_crash"
            failureSummary = "Minecraft crashed while loading a mod entrypoint."
        }
    }

    if ($text.Contains("incompatible mods found")) {
        return @{
            failureKind = "incompatible_mods"
            failureSummary = "Fabric rejected the selected mod set as incompatible."
        }
    }

    return $null
}

function Test-HealthyLaunch {
    param(
        [string[]]$Lines
    )

    if ($Lines.Count -eq 0) {
        return $false
    }

    return ($Lines | Where-Object {
        $_.Contains("[Render thread/") -or
        $_.Contains("Loading Minecraft") -or
        $_.Contains("OpenAL initialized") -or
        $_.Contains("Reloading ResourceManager")
    } | Select-Object -First 1) -ne $null
}

function Get-RunningProcess {
    param(
        [int]$Id
    )

    $process = Get-Process -Id $Id -ErrorAction SilentlyContinue
    if ($process) {
        $process.Refresh()
    }
    return $process
}

function Get-LauncherLogRoot {
    return Join-Path $env:LOCALAPPDATA "com.cubic.launcher\logs\launches"
}

function Get-BootstrapFallback {
    param(
        [datetime]$StartedAt,
        [string]$ModlistName,
        [string]$MinecraftVersion,
        [string]$ModLoader
    )

    $launchesRoot = Get-LauncherLogRoot
    if (-not (Test-Path $launchesRoot)) {
        return $null
    }

    $modlistSlug = ($ModlistName.ToLowerInvariant() -replace "[^a-z0-9_-]", "-").Trim("-")
    $versionSlug = ($MinecraftVersion.ToLowerInvariant() -replace "[^a-z0-9_-]", "-").Trim("-")
    $loaderSlug = ($ModLoader.ToLowerInvariant() -replace "[^a-z0-9_-]", "-").Trim("-")

    $candidate = Get-ChildItem $launchesRoot -Directory |
        Where-Object {
            $_.LastWriteTime -ge $StartedAt.AddSeconds(-5) -and
            $_.Name -like "*-$modlistSlug-$versionSlug-$loaderSlug"
        } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1

    if (-not $candidate) {
        return $null
    }

    $launcherLogPath = Join-Path $candidate.FullName "launcher.log"
    if (-not (Test-Path $launcherLogPath)) {
        return $null
    }

    $spawnLine = Get-Content $launcherLogPath | Where-Object { $_ -match "Spawned Minecraft process with PID (\d+)" } | Select-Object -Last 1
    if (-not $spawnLine) {
        return $null
    }

    $spawnMatch = [regex]::Match($spawnLine, "Spawned Minecraft process with PID (\d+)")
    if (-not $spawnMatch.Success) {
        return $null
    }

    $pid = [int]$spawnMatch.Groups[1].Value
    return [pscustomobject]@{
        started = $true
        success = $false
        state = "started"
        pid = $pid
        launchLogDir = $candidate.FullName
    }
}

if (-not $SkipBuild -or -not (Test-Path $debugExe)) {
    cargo build --manifest-path $cargoManifest
}

if (-not (Test-Path $debugExe)) {
    throw "Launcher executable was not built: $debugExe"
}

$request = @{
    modlistName = $ModlistName
    minecraftVersion = $MinecraftVersion
    modLoader = $ModLoader
    timeoutSeconds = $TimeoutSeconds
    successAfterSeconds = $SuccessAfterSeconds
    terminateOnSuccess = -not $KeepRunningOnSuccess.IsPresent
    terminateOnTimeout = $true
} | ConvertTo-Json -Compress

$previousRequest = $env:CUBIC_AUTOMATION_VERIFY_REQUEST
$previousOutput = $env:CUBIC_AUTOMATION_VERIFY_OUTPUT
$previousExit = $env:CUBIC_AUTOMATION_VERIFY_EXIT
$previousCacheOnly = $env:CUBIC_AUTOMATION_CACHE_ONLY_MODE

try {
    $env:CUBIC_AUTOMATION_VERIFY_REQUEST = $request
    $env:CUBIC_AUTOMATION_VERIFY_OUTPUT = $resultPath
    $env:CUBIC_AUTOMATION_VERIFY_EXIT = "1"
    if ($CacheOnlyMode -eq "current") {
        Remove-Item Env:CUBIC_AUTOMATION_CACHE_ONLY_MODE -ErrorAction SilentlyContinue
    } else {
        $env:CUBIC_AUTOMATION_CACHE_ONLY_MODE = $CacheOnlyMode
    }

    $bootstrapStart = Get-Date
    $process = Start-Process -FilePath $debugExe -WorkingDirectory $repoRoot -PassThru
    $bootstrapWaitSeconds = [Math]::Max($TimeoutSeconds, 90)
    $deadline = (Get-Date).AddSeconds($bootstrapWaitSeconds)

    while ((Get-Date) -lt $deadline) {
        if (Test-Path $resultPath) {
            break
        }

        if ($process.HasExited) {
            break
        }

        Start-Sleep -Seconds 1
        $process.Refresh()
    }

    if (Test-Path $resultPath) {
        $bootstrap = Get-Content $resultPath -Raw | ConvertFrom-Json
    } else {
        $bootstrap = Get-BootstrapFallback -StartedAt $bootstrapStart -ModlistName $ModlistName -MinecraftVersion $MinecraftVersion -ModLoader $ModLoader
        if (-not $bootstrap) {
            throw "Launcher startup did not write automation output: $resultPath"
        }
    }

    if (-not $bootstrap.started) {
        $result = $bootstrap
    } else {
        $launcherPid = [int]$bootstrap.pid
        $launchLogDir = [string]$bootstrap.launchLogDir
        $minecraftLogPath = Join-Path $launchLogDir "minecraft.log"
        $summaryLogPath = Join-Path $launchLogDir "summary.log"
        $verificationStart = Get-Date
        $successDeadline = $verificationStart.AddSeconds([Math]::Max($SuccessAfterSeconds, 1))
        $timeoutDeadline = $verificationStart.AddSeconds([Math]::Max($TimeoutSeconds, 5))
        $processAliveSince = $null
        $windowVisibleSince = $null

        while ((Get-Date) -lt $timeoutDeadline) {
            $tail = Get-LogTail -Path $minecraftLogPath -LineCount 80
            $failure = Get-FailureInfo -Lines $tail
            if ($failure) {
                $result = [pscustomobject]@{
                    started = $true
                    success = $false
                    state = "crashed"
                    pid = $launcherPid
                    launchLogDir = $launchLogDir
                    durationMs = [int][Math]::Round(((Get-Date) - $verificationStart).TotalMilliseconds)
                    failureKind = $failure.failureKind
                    failureSummary = $failure.failureSummary
                    minecraftLogTail = $tail
                }
                break
            }

            $summary = if (Test-Path $summaryLogPath) { Get-Content $summaryLogPath } else { @() }
            $exitLine = $summary | Where-Object { $_ -like "minecraft_exit_success=*" } | Select-Object -Last 1
            $exitCodeLine = $summary | Where-Object { $_ -like "minecraft_exit_code=*" } | Select-Object -Last 1
            if ($exitLine) {
                $exitCodeSuffix = ""
                if ($exitCodeLine) {
                    $exitCode = $exitCodeLine.Substring("minecraft_exit_code=".Length)
                    if ($exitCode -and $exitCode -ne "none") {
                        $exitCodeSuffix = " (exit code $exitCode)"
                    }
                }

                $result = [pscustomobject]@{
                    started = $true
                    success = $false
                    state = "exited"
                    pid = $launcherPid
                    launchLogDir = $launchLogDir
                    durationMs = [int][Math]::Round(((Get-Date) - $verificationStart).TotalMilliseconds)
                    failureKind = "process_exited"
                    failureSummary = "Minecraft exited before the verification success window was reached$exitCodeSuffix."
                    minecraftLogTail = $tail
                }
                break
            }

            $launcherProcessAlive = Get-RunningProcess -Id $launcherPid
            if ($launcherProcessAlive) {
                if (-not $processAliveSince) {
                    $processAliveSince = Get-Date
                }
                $hasWindow = $launcherProcessAlive.MainWindowHandle -ne 0
                if ($hasWindow) {
                    if (-not $windowVisibleSince) {
                        $windowVisibleSince = Get-Date
                    }
                } else {
                    $windowVisibleSince = $null
                }
            } else {
                $processAliveSince = $null
                $windowVisibleSince = $null
            }

            $aliveLongEnough = $false
            if ($processAliveSince) {
                $aliveLongEnough = ((Get-Date) - $processAliveSince).TotalSeconds -ge [Math]::Max($MinimumAliveSeconds, 1)
            }

            $windowLongEnough = $false
            if ($windowVisibleSince) {
                $windowLongEnough = ((Get-Date) - $windowVisibleSince).TotalSeconds -ge [Math]::Max($MinimumWindowSeconds, 1)
            }
            $healthyLaunch = Test-HealthyLaunch -Lines $tail

            if ((Get-Date) -ge $successDeadline -and $launcherProcessAlive -and $aliveLongEnough -and $healthyLaunch) {
                $result = [pscustomobject]@{
                    started = $true
                    success = $true
                    state = "running"
                    pid = $launcherPid
                    launchLogDir = $launchLogDir
                    durationMs = [int][Math]::Round(((Get-Date) - $verificationStart).TotalMilliseconds)
                    cacheOnlyMode = $CacheOnlyMode
                    failureKind = $null
                    failureSummary = $null
                    minecraftLogTail = $tail
                }
                break
            }

            Start-Sleep -Seconds 1
        }

        if (-not $result) {
            $tail = Get-LogTail -Path $minecraftLogPath -LineCount 80
            $aliveRequirement = [Math]::Max($MinimumAliveSeconds, 1)
            $windowRequirement = [Math]::Max($MinimumWindowSeconds, 1)
            $result = [pscustomobject]@{
                started = $true
                success = $false
                state = "timed_out"
                pid = $launcherPid
                launchLogDir = $launchLogDir
                durationMs = [int][Math]::Round(((Get-Date) - $verificationStart).TotalMilliseconds)
                cacheOnlyMode = $CacheOnlyMode
                failureKind = "timed_out"
                failureSummary = "Minecraft did not satisfy the startup verification within $TimeoutSeconds seconds. Required process alive time: $aliveRequirement seconds. The verifier accepts either a visible game window for $windowRequirement seconds or strong in-game log markers."
                minecraftLogTail = $tail
            }
        }
    }

    $result
}
finally {
    if ($launcherPid -and ((-not $result) -or (-not $result.success) -or (-not $KeepRunningOnSuccess.IsPresent))) {
        Stop-Process -Id $launcherPid -Force -ErrorAction SilentlyContinue
    }

    if ($process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }

    if ($null -eq $previousRequest) {
        Remove-Item Env:CUBIC_AUTOMATION_VERIFY_REQUEST -ErrorAction SilentlyContinue
    } else {
        $env:CUBIC_AUTOMATION_VERIFY_REQUEST = $previousRequest
    }

    if ($null -eq $previousOutput) {
        Remove-Item Env:CUBIC_AUTOMATION_VERIFY_OUTPUT -ErrorAction SilentlyContinue
    } else {
        $env:CUBIC_AUTOMATION_VERIFY_OUTPUT = $previousOutput
    }

    if ($null -eq $previousExit) {
        Remove-Item Env:CUBIC_AUTOMATION_VERIFY_EXIT -ErrorAction SilentlyContinue
    } else {
        $env:CUBIC_AUTOMATION_VERIFY_EXIT = $previousExit
    }

    if ($null -eq $previousCacheOnly) {
        Remove-Item Env:CUBIC_AUTOMATION_CACHE_ONLY_MODE -ErrorAction SilentlyContinue
    } else {
        $env:CUBIC_AUTOMATION_CACHE_ONLY_MODE = $previousCacheOnly
    }
}
