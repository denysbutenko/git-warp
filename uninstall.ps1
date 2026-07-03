#requires -Version 5.1
<#
.SYNOPSIS
    Uninstall Git-Warp on Windows.
.DESCRIPTION
    Removes the Git-Warp binary installed by install.ps1, strips Claude/Codex
    hook entries before deletion, and prints cleanup hints for leftover
    PowerShell-profile snippets.

    Defaults to "$env:LOCALAPPDATA\Programs\git-warp\bin" (the install.ps1
    default). Override with -InstallDir or $env:GIT_WARP_INSTALL_DIR.

    For a Cargo install with a custom root, pass -InstallRoot or
    $env:GIT_WARP_INSTALL_ROOT (mirrors install.ps1). The uninstaller then
    targets "<root>\bin\warp.exe".

    Other detected warp.exe installs (Cargo, custom directories) are listed
    but not touched.
.PARAMETER InstallDir
    Directory containing warp.exe. Defaults to the install.ps1 location.
.PARAMETER InstallRoot
    Cargo install root that owns "<root>\bin\warp.exe". Overrides -InstallDir
    when set. Mirrors install.ps1 -InstallRoot / $env:GIT_WARP_INSTALL_ROOT.
.PARAMETER DryRun
    Print the actions that would run without modifying the system.
.EXAMPLE
    irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/uninstall.ps1 | iex
.EXAMPLE
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/uninstall.ps1))) -DryRun
.EXAMPLE
    $env:GIT_WARP_KEEP_HOOKS = '1'
    & .\uninstall.ps1
#>
[CmdletBinding()]
param(
    [string]$InstallDir,
    [string]$InstallRoot,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Coalesce {
    param([string]$Explicit, [string]$EnvName, [string]$Default)
    if ($Explicit) { return $Explicit }
    $envValue = [Environment]::GetEnvironmentVariable($EnvName)
    if ($envValue) { return $envValue }
    return $Default
}

function Write-Warn {
    param([string]$Message)
    [Console]::Error.WriteLine("warning: $Message")
}

function Write-Err {
    param([string]$Message)
    [Console]::Error.WriteLine("error: $Message")
}

function Get-BinaryVersion {
    param([string]$Path)
    try {
        $line = & $Path --version 2>$null | Select-Object -First 1
        if ($line) { return $line.ToString().Trim() }
    } catch {
        # binary refused to run; treat as no version
    }
    return $null
}

function Remove-Hooks {
    param([string]$Target, [bool]$Preview)

    if (-not (Test-Path -LiteralPath $Target)) { return }
    if ([Environment]::GetEnvironmentVariable('GIT_WARP_KEEP_HOOKS') -eq '1') { return }

    $hasGit = Test-Path -LiteralPath '.git'

    if ($Preview) {
        Write-Host "Would remove user hooks with: $Target hooks-remove --level user --runtime all"
        if ($hasGit) {
            Write-Host "Would remove project hooks with: $Target hooks-remove --level project --runtime all"
        }
        return
    }

    Write-Host "Removing agent hooks..."
    try {
        & $Target hooks-remove --level user --runtime all *> $null
        if ($LASTEXITCODE -ne 0) { Write-Warn "failed to remove user hooks" }
    } catch {
        Write-Warn "failed to remove user hooks ($($_.Exception.Message))"
    }

    if ($hasGit) {
        try {
            & $Target hooks-remove --level project --runtime all *> $null
            if ($LASTEXITCODE -ne 0) { Write-Warn "failed to remove project hooks" }
        } catch {
            Write-Warn "failed to remove project hooks ($($_.Exception.Message))"
        }
    }
}

function Remove-Binary {
    param([string]$Target, [string]$Dir, [bool]$Preview)

    if (-not (Test-Path -LiteralPath $Target)) {
        Write-Host "No Git-Warp binary found at $Target; nothing to remove."
        return
    }

    if ($Preview) {
        Write-Host "Would remove $Target"
        return
    }

    try {
        Remove-Item -LiteralPath $Target -Force
    } catch {
        Write-Err "failed to remove $Target ($($_.Exception.Message))"
        throw
    }
    Write-Host "Removed $Target"

    if (Test-Path -LiteralPath $Dir) {
        $remaining = Get-ChildItem -LiteralPath $Dir -Force -ErrorAction SilentlyContinue
        if (-not $remaining) {
            try {
                Remove-Item -LiteralPath $Dir -Force
                Write-Host "Removed empty install directory $Dir"
            } catch {
                # leave directory in place if removal fails
            }
        }
    }
}

function Test-ProfileSnippet {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return $false }
    try {
        $content = Get-Content -LiteralPath $Path -Raw -ErrorAction Stop
    } catch {
        return $false
    }
    if (-not $content) { return $false }
    return ($content -match 'warp_cd' -or $content -match 'warp __complete')
}

function Show-ProfileLeftovers {
    $candidates = @()
    if ($PROFILE -is [string]) {
        $candidates += $PROFILE
    } else {
        foreach ($name in 'AllUsersAllHosts', 'AllUsersCurrentHost', 'CurrentUserAllHosts', 'CurrentUserCurrentHost') {
            $value = $PROFILE.$name
            if ($value) { $candidates += $value }
        }
    }

    $found = @()
    foreach ($path in ($candidates | Select-Object -Unique)) {
        if (Test-ProfileSnippet -Path $path) { $found += $path }
    }

    if ($found.Count -eq 0) { return }

    Write-Host ""
    Write-Host "Leftover PowerShell-profile snippets detected:"
    foreach ($path in $found) { Write-Host "  - $path" }
    Write-Host "Remove lines containing 'warp_cd' or 'warp __complete' from these files."
}

function Show-OtherInstalls {
    param([string]$Target, [string]$InstallRoot)

    $candidates = @()
    if ($env:GIT_WARP_INSTALL_DIR) {
        $candidates += (Join-Path $env:GIT_WARP_INSTALL_DIR 'warp.exe')
    }
    if ($env:LOCALAPPDATA) {
        $candidates += (Join-Path $env:LOCALAPPDATA 'Programs\git-warp\bin\warp.exe')
    }
    if ($env:GIT_WARP_INSTALL_ROOT) {
        $candidates += (Join-Path $env:GIT_WARP_INSTALL_ROOT 'bin\warp.exe')
    }
    if ($InstallRoot) {
        $candidates += (Join-Path $InstallRoot 'bin\warp.exe')
    }
    if ($env:USERPROFILE) {
        $candidates += (Join-Path $env:USERPROFILE '.cargo\bin\warp.exe')
    }

    $targetNorm = $Target.ToLowerInvariant()
    $printed = New-Object 'System.Collections.Generic.HashSet[string]'
    $lines = @()
    foreach ($path in $candidates) {
        if (-not $path) { continue }
        $norm = $path.ToLowerInvariant()
        if ($norm -eq $targetNorm) { continue }
        if (-not $printed.Add($norm)) { continue }
        if (-not (Test-Path -LiteralPath $path)) { continue }
        $version = Get-BinaryVersion -Path $path
        if ($version) {
            $lines += "  - $path ($version)"
        } else {
            $lines += "  - $path"
        }
    }

    if ($lines.Count -eq 0) { return }

    Write-Host ""
    Write-Host "Other Git-Warp installs detected (not removed):"
    foreach ($line in $lines) { Write-Host $line }
    Write-Host "Remove them manually, or with the matching tool:"
    Write-Host "  Cargo install:    cargo uninstall git-warp"
    Write-Host "  Other directory:  Remove-Item <path>"
}

function Show-PathLeftover {
    param([string]$Target)

    $active = Get-Command warp -ErrorAction SilentlyContinue
    if (-not $active) { return }

    $activePath = $active.Source
    if (-not $activePath) { return }
    if ($activePath.ToLowerInvariant() -eq $Target.ToLowerInvariant()) { return }

    Write-Host ""
    Write-Host "'warp' is still on PATH at $activePath."
    Write-Host "Remove it or adjust PATH if you intended a complete uninstall."
}

$defaultDir = Join-Path $env:LOCALAPPDATA 'Programs\git-warp\bin'
$installDir = Coalesce $InstallDir 'GIT_WARP_INSTALL_DIR' $defaultDir
$installRoot = Coalesce $InstallRoot 'GIT_WARP_INSTALL_ROOT' $null
if ($installRoot) {
    $installDir = Join-Path $installRoot 'bin'
}
$target = Join-Path $installDir 'warp.exe'

Remove-Hooks -Target $target -Preview $DryRun.IsPresent
Remove-Binary -Target $target -Dir $installDir -Preview $DryRun.IsPresent
Show-ProfileLeftovers
Show-OtherInstalls -Target $target -InstallRoot $installRoot
Show-PathLeftover -Target $target
