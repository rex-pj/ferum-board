<#
.SYNOPSIS
    Rebuilds every distributable archive under examples/ from its source directory.

.DESCRIPTION
    Themes  -> examples/themes/<dir>/   ==> examples/themes/<dir>-<version>.zip
    Plugins -> examples/plugins/<dir>/  ==> examples/plugins/<dir>-<version>.fpkg

    These archives are what an admin actually uploads, and they had drifted from
    their sources because packaging was a manual step: two theme archives still
    contained a <script src="htmx.min.js"> tag their sources no longer had, and
    the plugin packages disagreed with each other about whether README.md ships
    (announcement-banner included it, discord-notifier did not, both had one).
    Deterministic packaging is the fix — always archive the whole source
    directory, never hand-pick.

    The archive root is the CONTENTS of the source directory, not the directory
    itself: theme.json / plugin.toml sit at the top level of the archive. The
    installers assume this.

    Validation mirrors what the server enforces on upload, so a package that
    would be rejected at upload time fails here instead — at build time, where
    it is cheap to notice.

    PowerShell-only, deliberately: this repo's Git Bash has no `zip`, `7z` or
    `python`, and `Compress-Archive` is the one tool present on every Windows dev
    box. It emits forward-slash entry names, which the Rust extractor requires.

.PARAMETER Check
    Verify archives are up to date without writing anything. Exits non-zero if
    any archive is missing or its contents differ from the source. Intended for
    CI, so a stale artifact fails the build instead of shipping.

.EXAMPLE
    pwsh scripts/package-examples.ps1
    pwsh scripts/package-examples.ps1 -Check
#>
[CmdletBinding()]
param([switch]$Check)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem

$repoRoot   = Split-Path -Parent $PSScriptRoot
$failures   = @()
$built      = 0
$discovered = 0   # guards against a vacuous success — see the check at the end

# Extensions the theme upload handler refuses. Catching them here turns a
# confusing upload-time rejection into a build-time error naming the file.
$ForbiddenThemeExt = @('.svelte', '.ts', '.tsx', '.jsx', '.vue')

function Get-TomlValue {
    param([string]$Path, [string]$Key)
    $line = Select-String -Path $Path -Pattern "^\s*$Key\s*=" -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $line) { return $null }
    if ($line.Line -match '=\s*"([^"]*)"') { return $Matches[1] }
    return $null
}

function Get-RelativeFiles {
    param([string]$Dir)
    Get-ChildItem -Path $Dir -Recurse -File |
        ForEach-Object { $_.FullName.Substring($Dir.Length + 1).Replace('\', '/') } |
        Sort-Object
}

function Get-ArchiveEntries {
    param([string]$Path)
    if (-not (Test-Path $Path)) { return @() }
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Path)
    try { return @($zip.Entries | ForEach-Object { $_.FullName } | Sort-Object) }
    finally { $zip.Dispose() }
}

# Content hash per entry, not just the name list.
#
# Comparing names alone is what let the htmx drift survive: the two stale theme
# archives held exactly the files their sources held, and differed only inside
# templates/base.html. A check that cannot see that is worse than no check,
# because it reports "ok".
function Get-ArchiveHashes {
    param([string]$Path)
    $map = @{}
    if (-not (Test-Path $Path)) { return $map }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    $zip = [System.IO.Compression.ZipFile]::OpenRead($Path)
    try {
        foreach ($e in $zip.Entries) {
            if ([string]::IsNullOrEmpty($e.Name)) { continue }   # directory entry
            $s  = $e.Open()
            $ms = New-Object System.IO.MemoryStream
            try {
                $s.CopyTo($ms)
                $map[$e.FullName] = [BitConverter]::ToString($sha.ComputeHash($ms.ToArray()))
            } finally { $ms.Dispose(); $s.Dispose() }
        }
    } finally { $zip.Dispose(); $sha.Dispose() }
    return $map
}

function Get-SourceHashes {
    param([string]$Dir)
    $map = @{}
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        foreach ($f in Get-ChildItem -Path $Dir -Recurse -File) {
            $rel = $f.FullName.Substring($Dir.Length + 1).Replace('\', '/')
            $map[$rel] = [BitConverter]::ToString($sha.ComputeHash([IO.File]::ReadAllBytes($f.FullName)))
        }
    } finally { $sha.Dispose() }
    return $map
}

function Build-Archive {
    param([string]$SourceDir, [string]$OutFile, [string]$Label)

    $script:discovered++
    $srcFiles = Get-RelativeFiles -Dir $SourceDir
    if ($srcFiles.Count -eq 0) {
        $script:failures += "$Label - source directory is empty"
        return
    }

    if ($Check) {
        $have = Get-ArchiveEntries -Path $OutFile
        if ($have.Count -eq 0) {
            $script:failures += "$Label - archive missing: $(Split-Path -Leaf $OutFile)"
            return
        }
        $diff = Compare-Object -ReferenceObject $srcFiles -DifferenceObject $have
        if ($diff) {
            $added   = ($diff | Where-Object SideIndicator -eq '=>' | ForEach-Object InputObject) -join ', '
            $missing = ($diff | Where-Object SideIndicator -eq '<=' | ForEach-Object InputObject) -join ', '
            $detail  = @()
            if ($missing) { $detail += "missing from archive: $missing" }
            if ($added)   { $detail += "stale in archive: $added" }
            $script:failures += "$Label - file list differs ($($detail -join '; '))"
            return
        }

        $srcH = Get-SourceHashes  -Dir $SourceDir
        $arcH = Get-ArchiveHashes -Path $OutFile
        $changed = @($srcH.Keys | Where-Object { $srcH[$_] -ne $arcH[$_] } | Sort-Object)
        if ($changed.Count -gt 0) {
            $script:failures += "$Label - content differs: $($changed -join ', ')"
        } else {
            Write-Host "  ok    $Label ($($srcFiles.Count) files)"
        }
        return
    }

    # Rebuild from scratch: -Update would leave files deleted from the source
    # still present in the archive, which is exactly how these drifted.
    if (Test-Path $OutFile) { Remove-Item $OutFile -Force }
    Compress-Archive -Path (Join-Path $SourceDir '*') -DestinationPath $OutFile -Force

    $have = Get-ArchiveEntries -Path $OutFile
    $diff = Compare-Object -ReferenceObject $srcFiles -DifferenceObject $have
    if ($diff) {
        $script:failures += "$Label - archive does not match source after write"
        return
    }
    $size = [math]::Round((Get-Item $OutFile).Length / 1KB, 1)
    Write-Host "  built $Label -> $(Split-Path -Leaf $OutFile) ($($srcFiles.Count) files, $size KB)"
    $script:built++
}

# ── Themes ────────────────────────────────────────────────────────────────────
Write-Host "`nThemes"
$themesRoot = Join-Path $repoRoot 'examples/themes'
foreach ($dir in Get-ChildItem -Path $themesRoot -Directory | Sort-Object Name) {
    $label    = "theme/$($dir.Name)"
    $manifest = Join-Path $dir.FullName 'theme.json'

    if (-not (Test-Path $manifest)) { $failures += "$label - missing theme.json"; continue }
    if (-not (Test-Path (Join-Path $dir.FullName 'templates/base.html'))) {
        # The upload handler rejects an archive without it.
        $failures += "$label - missing templates/base.html"; continue
    }

    $bad = Get-ChildItem -Path $dir.FullName -Recurse -File |
           Where-Object { $ForbiddenThemeExt -contains $_.Extension }
    if ($bad) {
        $failures += "$label - contains file types the uploader rejects: $($bad.Name -join ', ')"
        continue
    }

    $version = (Get-Content $manifest -Raw | ConvertFrom-Json).version
    if (-not $version) { $failures += "$label - theme.json has no version"; continue }

    Build-Archive -SourceDir $dir.FullName `
                  -OutFile (Join-Path $themesRoot "$($dir.Name)-$version.zip") `
                  -Label $label
}

# ── Plugins ───────────────────────────────────────────────────────────────────
Write-Host "`nPlugins"
$pluginsRoot = Join-Path $repoRoot 'examples/plugins'
foreach ($dir in Get-ChildItem -Path $pluginsRoot -Directory | Sort-Object Name) {
    $label    = "plugin/$($dir.Name)"
    $manifest = Join-Path $dir.FullName 'plugin.toml'

    if (-not (Test-Path $manifest)) { $failures += "$label - missing plugin.toml"; continue }

    $version = Get-TomlValue -Path $manifest -Key 'version'
    if (-not $version) { $failures += "$label - plugin.toml has no version"; continue }

    # A Script-tier plugin names its bundle; a Manifest-tier one has no code at
    # all. Only verify the file exists when the manifest actually points at one.
    $bundle = Get-TomlValue -Path $manifest -Key 'bundle_file'
    if ($bundle -and -not (Test-Path (Join-Path $dir.FullName $bundle))) {
        $failures += "$label - plugin.toml references missing bundle '$bundle'"
        continue
    }

    Build-Archive -SourceDir $dir.FullName `
                  -OutFile (Join-Path $pluginsRoot "$($dir.Name)-$version.fpkg") `
                  -Label $label
}

# ── Result ────────────────────────────────────────────────────────────────────
Write-Host ""
# Finding nothing must never read as success. A wrong repoRoot (a copied or
# renamed checkout) makes both loops iterate zero times, and without this the
# script would report "All archives match their sources" having compared nothing.
if ($discovered -eq 0) {
    Write-Host "FAILED:" -ForegroundColor Red
    Write-Host "  - no theme or plugin directories found under $repoRoot\examples\" -ForegroundColor Red
    Write-Host "    (is repoRoot wrong? expected examples\themes\ and examples\plugins\)" -ForegroundColor Red
    exit 1
}

if ($failures.Count -gt 0) {
    Write-Host "FAILED:" -ForegroundColor Red
    $failures | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    exit 1
}
if ($Check) { Write-Host "All archives match their sources." -ForegroundColor Green }
else        { Write-Host "Rebuilt $built archive(s)." -ForegroundColor Green }
