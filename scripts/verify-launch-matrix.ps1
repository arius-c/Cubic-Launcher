param(
    [string]$ModlistName = "Performance",

    [string[]]$MinecraftVersions = @(
        "1.16.5",
        "1.18",
        "1.20.2",
        "1.20.5",
        "1.21.1",
        "1.21.5",
        "1.21.6",
        "1.21.11",
        "26.1.1",
        "26.1.2"
    ),

    [string[]]$ModLoaders = @("Fabric", "NeoForge", "Forge"),

    [int]$TimeoutSeconds = 75,

    [int]$SuccessAfterSeconds = 15,

    [int]$MinimumAliveSeconds = 15,

    [ValidateSet("current", "on", "off")]
    [string]$CacheOnlyMode = "current",

    [string]$OutputDir = "",

    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$verifyScript = Join-Path $PSScriptRoot "verify-launch.ps1"
$cargoManifest = Join-Path $repoRoot "src-tauri\Cargo.toml"

if (-not (Test-Path $verifyScript)) {
    throw "Missing verifier script: $verifyScript"
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $OutputDir = Join-Path $repoRoot "launch-verification-$stamp"
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$results = New-Object System.Collections.Generic.List[object]
$total = $MinecraftVersions.Count * $ModLoaders.Count
$index = 0

if (-not $SkipBuild) {
    cargo build --manifest-path $cargoManifest
}

foreach ($version in $MinecraftVersions) {
    foreach ($loader in $ModLoaders) {
        $index += 1
        $label = "$version / $loader"
        Write-Host "[$index/$total] Verifying $label"

        try {
            $params = @{
                ModlistName = $ModlistName
                MinecraftVersion = $version
                ModLoader = $loader
                TimeoutSeconds = $TimeoutSeconds
                SuccessAfterSeconds = $SuccessAfterSeconds
                MinimumAliveSeconds = $MinimumAliveSeconds
                CacheOnlyMode = $CacheOnlyMode
                SkipBuild = $true
            }

            $output = @(& $verifyScript @params)
            $result = $output | Where-Object { $_ -isnot [string] } | Select-Object -Last 1

            if (-not $result) {
                throw "Verifier produced no structured result."
            }
        } catch {
            $result = [pscustomobject]@{
                started = $false
                success = $false
                state = "script_error"
                pid = $null
                launchLogDir = $null
                durationMs = 0
                cacheOnlyMode = $CacheOnlyMode
                failureKind = "script_error"
                failureSummary = $_.Exception.Message
                minecraftLogTail = @()
            }
        }

        $row = [pscustomobject]@{
            modlistName = $ModlistName
            minecraftVersion = $version
            modLoader = $loader
            success = [bool]$result.success
            state = [string]$result.state
            failureKind = [string]$result.failureKind
            failureSummary = [string]$result.failureSummary
            durationMs = if ($null -eq $result.durationMs) { 0 } else { [int64]$result.durationMs }
            launchLogDir = [string]$result.launchLogDir
        }

        $results.Add($row)

        $safeLoader = $loader.ToLowerInvariant()
        $safeVersion = $version -replace "[^A-Za-z0-9_.-]", "-"
        $result | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 (Join-Path $OutputDir "$safeVersion-$safeLoader.json")
        $row | Format-List | Out-String | Write-Host
    }
}

$jsonPath = Join-Path $OutputDir "summary.json"
$csvPath = Join-Path $OutputDir "summary.csv"

$results | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 $jsonPath
$results | Export-Csv -NoTypeInformation -Encoding UTF8 $csvPath

Write-Host "Wrote $jsonPath"
Write-Host "Wrote $csvPath"

$results
