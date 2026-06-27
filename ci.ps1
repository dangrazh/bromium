<#
.SYNOPSIS
    Local CI pipeline: bump version, build, commit, and publish via maturin.
.PARAMETER Patch
    Bump the patch version by 1.
.PARAMETER Minor
    Bump the minor version by 1.
#>
[CmdletBinding()]
param(
    [switch]$Patch,
    [switch]$Minor
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$WorkspaceRoot = 'C:\LocalData\Rust\bromium-ws-new'
$CrateDir      = Join-Path $WorkspaceRoot 'crates\bromium'
$CargoToml     = Join-Path $CrateDir 'Cargo.toml'
$VenvActivate  = Join-Path $CrateDir '.pyo3venv\Scripts\Activate.ps1'

# --- Validate arguments ---
if ($Patch -and $Minor) {
    Write-Error "Specify either --Patch or --Minor, not both."
}
if (-not $Patch -and -not $Minor) {
    Write-Error "Specify --Patch or --Minor."
}

# --- Step 0: ensure we are in the workspace root ---
Set-Location $WorkspaceRoot
Write-Host "Working directory: $(Get-Location)" -ForegroundColor Cyan

# --- Step 1: bump version in Cargo.toml ---
$content = Get-Content $CargoToml -Raw
if ($content -notmatch 'version\s*=\s*"(\d+)\.(\d+)\.(\d+)"') {
    Write-Error "Could not parse version from $CargoToml"
}
$major = [int]$Matches[1]
$minor = [int]$Matches[2]
$patch = [int]$Matches[3]
$oldVersion = "$major.$minor.$patch"

if ($Minor) {
    $minor++
    $patch = 0
} else {
    $patch++
}
$newVersion = "$major.$minor.$patch"

$content = $content -replace "version\s*=\s*`"$([regex]::Escape($oldVersion))`"", "version = `"$newVersion`""
Set-Content $CargoToml $content -NoNewline
Write-Host "Version bumped: $oldVersion -> $newVersion" -ForegroundColor Green

# --- Step 2: cargo build --release (entire workspace) ---
Write-Host "`nBuilding workspace in release mode..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Error "cargo build --release failed (exit code $LASTEXITCODE). Aborting."
}
Write-Host "Build succeeded." -ForegroundColor Green

# --- Step 3: git add & commit ---
Write-Host "`nCommitting changes..." -ForegroundColor Cyan
git add .
if ($LASTEXITCODE -ne 0) { Write-Error "git add failed." }

$bumpType = if ($Minor) { "minor" } else { "patch" }
$commitMsg = "Release v$newVersion ($bumpType version bump)"
git commit -m $commitMsg
if ($LASTEXITCODE -ne 0) { Write-Error "git commit failed." }
Write-Host "Committed: $commitMsg" -ForegroundColor Green

# --- Step 4: activate venv & maturin develop ---
Write-Host "`nActivating venv and running maturin develop..." -ForegroundColor Cyan
Set-Location $CrateDir
& $VenvActivate

maturin develop
if ($LASTEXITCODE -ne 0) {
    Write-Error "maturin develop failed (exit code $LASTEXITCODE). Aborting publish steps."
}
Write-Host "maturin develop succeeded." -ForegroundColor Green

# --- Step 5: maturin build --release ---
Write-Host "`nRunning maturin build --release..." -ForegroundColor Cyan
maturin build --release
if ($LASTEXITCODE -ne 0) {
    Write-Error "maturin build --release failed (exit code $LASTEXITCODE). Aborting."
}
Write-Host "maturin build --release succeeded." -ForegroundColor Green

# --- Step 6: maturin publish ---
Write-Host "`nPublishing with maturin..." -ForegroundColor Cyan
maturin publish
if ($LASTEXITCODE -ne 0) {
    Write-Error "maturin publish failed (exit code $LASTEXITCODE)."
}
Write-Host "`nPipeline complete — v$newVersion published." -ForegroundColor Green
