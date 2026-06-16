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
.EXAMPLE
    irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.ps1 | iex
.EXAMPLE
    & ([scriptblock]::Create((irm https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.ps1))) -InstallDir 'C:\tools\git-warp'
#>
[CmdletBinding()]
param(
    [string]$Version,
    [string]$InstallDir,
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
        [string]$Tag
    )

    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        Fail "cargo is required for the cargo install method" $Repo
    }

    Write-Host "Installing Git-Warp $Tag from $Repo with Cargo"
    & cargo install --locked --force --git $Repo --tag $Tag --bin warp git-warp
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

switch ($method) {
    'binary' { Install-Binary -Repo $repoUrl -Tag $tag -Dir $installDir -Base $downloadBase -Skip $skipChecksum }
    'cargo'  { Install-Cargo -Repo $repoUrl -Tag $tag }
    default  { Fail "unsupported install method: $method; use 'binary' or 'cargo'" $repoUrl }
}

Write-Host ""

$installed = Join-Path $installDir 'warp.exe'
if (Test-Path -LiteralPath $installed) {
    & $installed --version
} elseif (Get-Command warp -ErrorAction SilentlyContinue) {
    & warp --version
} else {
    Write-Host "Git-Warp installed, but 'warp' is not on PATH yet."
}

if (-not (Test-OnPath -Dir $installDir)) {
    Write-Host ""
    Write-Host "Add $installDir to PATH so your shell can find 'warp':"
    Write-Host "  `$env:PATH = '$installDir;' + `$env:PATH"
    Write-Host "Persist it across sessions with:"
    Write-Host "  [Environment]::SetEnvironmentVariable('Path', `"$installDir;`" + [Environment]::GetEnvironmentVariable('Path', 'User'), 'User')"
    Write-Host "Open a new terminal or run 'warp doctor' after updating PATH."
}

Write-Host "Run 'warp doctor' to check your setup."
