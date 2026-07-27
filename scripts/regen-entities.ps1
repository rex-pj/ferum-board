<#
.SYNOPSIS
    Regenerate Sea-ORM entities from the database schema.

.DESCRIPTION
    Closes the strong-typing loop: migrations define the schema, this script
    regenerates crates/ferum-infrastructure/src/entities/ from it, and the
    repositories (which reference entity columns) then fail to compile on any
    column/table rename.

    Steps:
      1. Reset a throwaway "regen" database and run every migration against it.
      2. Run `sea-orm-cli generate entity` over that schema, overwriting the
         entities directory in place.
      3. Review the result with `git diff` and `cargo check`.

    The entities are pristine `sea-orm-cli` output (serde derives, Eq added only
    when field types allow it), so an in-place overwrite is safe; git is the
    review surface.

.NOTES
    Requires sea-orm-cli:  cargo install sea-orm-cli --version ^2.0
    Run from the repository root.
#>
[CmdletBinding()]
param(
    # Base server URL without a database segment. Defaults to the dev DATABASE_URL's server.
    [string]$ServerUrl,
    # Name of the throwaway database used purely for generation.
    [string]$RegenDb = "ferum_board_regen"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$backend = Join-Path $repoRoot "backend"
$entitiesDir = Join-Path $backend "crates/ferum-infrastructure/src/entities"

if (-not (Get-Command sea-orm-cli -ErrorAction SilentlyContinue)) {
    throw "sea-orm-cli not found. Install it: cargo install sea-orm-cli --version ^2.0"
}

# Derive the server URL from backend/.env if not supplied.
if (-not $ServerUrl) {
    $envFile = Join-Path $backend ".env"
    $line = (Get-Content $envFile | Where-Object { $_ -match "^DATABASE_URL=" } | Select-Object -First 1)
    if (-not $line) { throw "DATABASE_URL not found in $envFile; pass -ServerUrl explicitly." }
    $dbUrl = $line -replace "^DATABASE_URL=", ""
    $ServerUrl = $dbUrl.Substring(0, $dbUrl.LastIndexOf("/"))
}

$regenUrl = "$ServerUrl/$RegenDb"
Write-Host "==> Regen database: $regenUrl" -ForegroundColor Cyan

# 1. Reset + migrate the throwaway database (migration crate's CLI).
#    `fresh` drops every table then re-applies all migrations.
Push-Location $backend
try {
    $env:DATABASE_URL = $regenUrl
    Write-Host "==> migration fresh" -ForegroundColor Cyan
    # `--features cli` is required: the migration bin target is behind
    # `required-features = ["cli"]` so the server binary does not link an
    # argument parser. Without it this invocation silently builds nothing to run.
    cargo run --quiet --package migration --features cli -- fresh
    if ($LASTEXITCODE -ne 0) { throw "migration fresh failed (does database '$RegenDb' exist?)" }

    # 2. Generate entities in place. Flags match the existing output.
    #    `--entity-format dense` (sea-orm 2.0) folds relations into `Model` as
    #    typed `BelongsTo`/`HasMany` fields, replacing the separate `Relation`
    #    enum and the hand-written `impl Related<..>` blocks.
    Write-Host "==> sea-orm-cli generate entity -> $entitiesDir" -ForegroundColor Cyan
    sea-orm-cli generate entity `
        --database-url $regenUrl `
        --output-dir $entitiesDir `
        --entity-format dense `
        --with-serde both `
        --date-time-crate chrono
    if ($LASTEXITCODE -ne 0) { throw "sea-orm-cli generate entity failed" }
}
finally {
    Pop-Location
}

Write-Host ""
Write-Host "Done. Review and verify:" -ForegroundColor Green
Write-Host "  git -C `"$repoRoot`" diff -- backend/crates/ferum-infrastructure/src/entities/"
Write-Host "  cargo check --manifest-path `"$backend/Cargo.toml`""
