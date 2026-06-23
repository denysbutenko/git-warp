#requires -Version 5.1
<#
.SYNOPSIS
    Install Git-Warp on Windows.
.DESCRIPTION
    Downloads the prebuilt Git-Warp .zip for x86_64-pc-windows-msvc, verifies
    its SHA256, and installs warp.exe into a user-writable directory.

    Defaults to the latest published release; pin with -Version or
    $env:GIT_WARP_VERSION. Default install directory is
    "$env:LOCALAPPDATA\Programs\git-warp\bin"; override with -InstallDir or
    $env:GIT_WARP_INSTALL_DIR.

    For -Method cargo, override the cargo install root with -InstallRoot or
    $env:GIT_WARP_INSTALL_ROOT (cargo writes to <root>\bin\warp.exe).
.EXAMPLE
    irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.ps1 | iex
.EXAMPLE
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.ps1))) -InstallDir 'C:\tools\git-warp'
.EXAMPLE
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.ps1))) -Method cargo -InstallRoot 'C:\tools\git-warp'
#>
[CmdletBinding()]
param(
    [string]$Version,
    [string]$InstallDir,
    [string]$InstallRoot,
    [string]$RepoUrl,
    [string]$DownloadBase,
    [ValidateSet('binary', 'cargo')]
    [string]$Method,
    [switch]$SkipChecksum
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

try {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
} catch {
    # Older runtimes may not expose Tls12; Invoke-WebRequest will still negotiate the OS default.
}

function Coalesce {
    param([string]$Explicit, [string]$EnvName, [string]$Default)
    if ($Explicit) { return $Explicit }
    $envValue = [Environment]::GetEnvironmentVariable($EnvName)
    if ($envValue) { return $envValue }
    return $Default
}

function Write-Err {
    param([string]$Message)
    [Console]::Error.WriteLine("error: $Message")
}

function Show-CargoHint {
    param([string]$Repo)
    [Console]::Error.WriteLine("")
    [Console]::Error.WriteLine("If you have Rust and Cargo installed, retry with:")
    [Console]::Error.WriteLine("  & ([scriptblock]::Create((irm $Repo/raw/main/install.ps1))) -Method cargo")
}

function Fail {
    param([string]$Message, [string]$Repo)
    Write-Err $Message
    if ($Repo) { Show-CargoHint -Repo $Repo }
    throw $Message
}

function Resolve-LatestTag {
    param([string]$Repo)

    if ($Repo -notmatch '^https://github\.com/') {
        Fail "GIT_WARP_REPO_URL ($Repo) is not on github.com; cannot auto-resolve the latest release" $Repo
    }

    $slug = $Repo -replace '^https://github\.com/', ''
    $api = "https://api.github.com/repos/$slug/releases/latest"

    try {
        $resp = Invoke-RestMethod -UseBasicParsing -Uri $api -Headers @{ 'User-Agent' = 'git-warp-install' }
    } catch {
        Fail "GitHub release lookup failed at $api ($($_.Exception.Message))" $Repo
    }

    if (-not $resp.tag_name) {
        Fail "could not parse tag_name from $api response" $Repo
    }

    return $resp.tag_name
}

function Get-ChecksumFromSidecar {
    param([string]$Path)
    $first = (Get-Content -LiteralPath $Path -TotalCount 1) -as [string]
    if (-not $first) { return $null }
    $token = ($first.Trim() -split '\s+', 2)[0]
    if (-not $token) { return $null }
    return $token.ToLowerInvariant()
}

function Test-OnPath {
    param([string]$Dir)
    $needle = $Dir.TrimEnd('\').ToLowerInvariant()
    foreach ($entry in $env:PATH -split ';') {
        if (-not $entry) { continue }
        if ($entry.TrimEnd('\').ToLowerInvariant() -eq $needle) { return $true }
    }
    return $false
}

function Get-CandidateInstallDirs {
    param([string]$Primary)
    $candidates = New-Object System.Collections.Generic.List[string]
    if ($Primary) { [void]$candidates.Add($Primary) }
    if ($env:LOCALAPPDATA) { [void]$candidates.Add((Join-Path $env:LOCALAPPDATA 'Programs\git-warp\bin')) }
    $cargoHome = [Environment]::GetEnvironmentVariable('CARGO_HOME')
    if ($cargoHome) {
        [void]$candidates.Add((Join-Path $cargoHome 'bin'))
    } elseif ($env:USERPROFILE) {
        [void]$candidates.Add((Join-Path $env:USERPROFILE '.cargo\bin'))
    }
    if ($env:ProgramData) { [void]$candidates.Add((Join-Path $env:ProgramData 'chocolatey\bin')) }
    if ($env:ProgramFiles) { [void]$candidates.Add((Join-Path $env:ProgramFiles 'Git-Warp\bin')) }

    $seen = @{}
    $result = New-Object System.Collections.Generic.List[string]
    foreach ($dir in $candidates) {
        $key = $dir.TrimEnd('\').ToLowerInvariant()
        if (-not $key) { continue }
        if ($seen.ContainsKey($key)) { continue }
        $seen[$key] = $true
        [void]$result.Add($dir)
    }
    return $result
}

function Get-ExistingWarpBinaries {
    param([string]$Primary)
    $found = New-Object System.Collections.Generic.List[pscustomobject]
    $seen = @{}
    foreach ($dir in Get-CandidateInstallDirs -Primary $Primary) {
        $binary = Join-Path $dir 'warp.exe'
        if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { continue }
        $key = $binary.ToLowerInvariant()
        if ($seen.ContainsKey($key)) { continue }
        $seen[$key] = $true
        $versionLine = $null
        try {
            $versionLine = (& $binary --version 2>$null | Select-Object -First 1)
        } catch {
            $versionLine = $null
        }
        [void]$found.Add([pscustomobject]@{ Path = $binary; Version = $versionLine })
    }
    return $found
}

function Show-ExistingInstalls {
    param([string]$Primary, [string]$Repo)
    $existing = Get-ExistingWarpBinaries -Primary $Primary
    if (-not $existing -or $existing.Count -eq 0) { return }
    Write-Host "Existing Git-Warp installs detected:"
    foreach ($entry in $existing) {
        if ($entry.Version) {
            Write-Host ("  - {0} ({1})" -f $entry.Path, $entry.Version)
        } else {
            Write-Host ("  - {0}" -f $entry.Path)
        }
    }
    $primaryBinary = Join-Path $Primary 'warp.exe'
    Write-Host "The installer will replace $primaryBinary; other locations are left as-is."
    Write-Host "Run 'irm $Repo/raw/main/uninstall.ps1 | iex' to remove the default install,"
    Write-Host "or 'cargo uninstall git-warp' to remove a Cargo install."
    Write-Host ""
}

function Resolve-InstalledBinary {
    param([string]$Method, [string]$InstallDir, [string]$InstallRoot)
    if ($Method -eq 'cargo') {
        if ($InstallRoot) {
            return Join-Path $InstallRoot 'bin\warp.exe'
        }
        $cargoHome = [Environment]::GetEnvironmentVariable('CARGO_HOME')
        if ($cargoHome) {
            $cargoBin = Join-Path $cargoHome 'bin'
        } elseif ($env:USERPROFILE) {
            $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
        } else {
            return $null
        }
        return Join-Path $cargoBin 'warp.exe'
    }
    return Join-Path $InstallDir 'warp.exe'
}

function Show-ActiveWarpShadowing {
    param([string]$InstalledPath)
    if (-not $InstalledPath) { return }
    $command = Get-Command warp -ErrorAction SilentlyContinue
    if (-not $command) { return }
    $active = $command.Source
    if (-not $active) { return }
    if ($active.TrimEnd('\').ToLowerInvariant() -eq $InstalledPath.TrimEnd('\').ToLowerInvariant()) { return }
    $installDir = Split-Path -Parent $InstalledPath
    Write-Host ""
    Write-Host "Note: 'warp' on PATH resolves to $active, not the binary just installed at $InstalledPath."
    Write-Host "Reorder PATH to put $installDir first, or remove the older binary at $active."
}

function Install-Binary {
    param(
        [string]$Repo,
        [string]$Tag,
        [string]$Dir,
        [string]$Base,
        [bool]$Skip
    )

    $target = 'x86_64-pc-windows-msvc'
    $asset = "git-warp-$Tag-$target.zip"
    $sumName = "$asset.sha256"
    $url = "$Base/$asset"
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("git-warp-install-" + [System.Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null

    try {
        $archive = Join-Path $tmp $asset
        Write-Host "Downloading Git-Warp $Tag for $target"
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $archive
        } catch {
            Fail "failed to download $url ($($_.Exception.Message))" $Repo
        }

        if (-not $Skip) {
            $sumPath = Join-Path $tmp $sumName
            try {
                Invoke-WebRequest -UseBasicParsing -Uri "$url.sha256" -OutFile $sumPath
            } catch {
                Fail "failed to download $url.sha256 ($($_.Exception.Message))" $Repo
            }

            $expected = Get-ChecksumFromSidecar -Path $sumPath
            if (-not $expected) {
                Fail "checksum sidecar for $asset is empty or malformed" $Repo
            }
            $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($expected -ne $actual) {
                Write-Err "checksum verification failed for $asset"
                Write-Err "  expected: $expected"
                Write-Err "  actual:   $actual"
                Show-CargoHint -Repo $Repo
                throw 'checksum mismatch'
            }
        } else {
            Write-Host "Skipping checksum verification (GIT_WARP_SKIP_CHECKSUM=1)"
        }

        $extract = Join-Path $tmp 'extract'
        New-Item -ItemType Directory -Force -Path $extract | Out-Null
        Expand-Archive -Force -Path $archive -DestinationPath $extract
        $binary = Join-Path $extract 'warp.exe'
        if (-not (Test-Path -LiteralPath $binary)) {
            Fail "release archive did not contain warp.exe" $Repo
        }

        New-Item -ItemType Directory -Force -Path $Dir | Out-Null
        Copy-Item -Force -Path $binary -Destination (Join-Path $Dir 'warp.exe')
    } finally {
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue -LiteralPath $tmp
    }
}

function Install-Cargo {
    param(
        [string]$Repo,
        [string]$Tag,
        [string]$InstallRoot
    )

    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        Fail "cargo is required for the cargo install method" $Repo
    }

    $cargoArgs = @('install', '--locked', '--force', '--git', $Repo, '--tag', $Tag, '--bin', 'warp')
    if ($InstallRoot) {
        $cargoArgs += @('--root', $InstallRoot)
        Write-Host "Installing Git-Warp $Tag from $Repo with Cargo into $InstallRoot\bin"
    } else {
        Write-Host "Installing Git-Warp $Tag from $Repo with Cargo"
    }
    $cargoArgs += 'git-warp'

    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        Fail "Cargo install failed for Git-Warp $Tag" $Repo
    }
}

$repoUrl = Coalesce $RepoUrl 'GIT_WARP_REPO_URL' 'https://github.com/denysbutenko/git-warp'
$method = Coalesce $Method 'GIT_WARP_INSTALL_METHOD' 'binary'

$skipChecksum = $SkipChecksum.IsPresent
if (-not $skipChecksum) {
    $skipEnv = [Environment]::GetEnvironmentVariable('GIT_WARP_SKIP_CHECKSUM')
    if ($skipEnv -eq '1') { $skipChecksum = $true }
}

$defaultDir = Join-Path $env:LOCALAPPDATA 'Programs\git-warp\bin'
$installDir = Coalesce $InstallDir 'GIT_WARP_INSTALL_DIR' $defaultDir
$installRoot = Coalesce $InstallRoot 'GIT_WARP_INSTALL_ROOT' $null
if (-not $installRoot -and $installDir -ne $defaultDir) {
    $trimmed = $installDir.TrimEnd('\')
    if ($trimmed.ToLowerInvariant().EndsWith('\bin')) {
        $installRoot = Split-Path -Parent $trimmed
    }
}

if ($Version) {
    $tag = $Version
} else {
    $envVersion = [Environment]::GetEnvironmentVariable('GIT_WARP_VERSION')
    if ($envVersion) {
        $tag = $envVersion
    } else {
        $tag = Resolve-LatestTag -Repo $repoUrl
        Write-Host "Resolved latest Git-Warp release: $tag"
    }
}

$downloadBase = Coalesce $DownloadBase 'GIT_WARP_DOWNLOAD_BASE' "$repoUrl/releases/download/$tag"

Show-ExistingInstalls -Primary $installDir -Repo $repoUrl

switch ($method) {
    'binary' { Install-Binary -Repo $repoUrl -Tag $tag -Dir $installDir -Base $downloadBase -Skip $skipChecksum }
    'cargo'  { Install-Cargo -Repo $repoUrl -Tag $tag -InstallRoot $installRoot }
    default  { Fail "unsupported install method: $method; use 'binary' or 'cargo'" $repoUrl }
}

Write-Host ""

$installedPath = Resolve-InstalledBinary -Method $method -InstallDir $installDir -InstallRoot $installRoot
if ($installedPath -and (Test-Path -LiteralPath $installedPath)) {
    & $installedPath --version
} else {
    Write-Host "Git-Warp installed, but 'warp.exe' was not found at $installedPath."
}

$pathDir = if ($installedPath) { Split-Path -Parent $installedPath } else { $installDir }
if (-not (Test-OnPath -Dir $pathDir)) {
    Write-Host ""
    Write-Host "Add $pathDir to PATH so your shell can find 'warp':"
    Write-Host "  `$env:PATH = '$pathDir;' + `$env:PATH"
    Write-Host "Persist it across sessions with:"
    Write-Host "  [Environment]::SetEnvironmentVariable('Path', `"$pathDir;`" + [Environment]::GetEnvironmentVariable('Path', 'User'), 'User')"
    Write-Host "Open a new terminal or run 'warp doctor' after updating PATH."
}

Show-ActiveWarpShadowing -InstalledPath $installedPath

Write-Host "Run 'warp doctor' to check your setup."
