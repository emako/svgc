#Requires -Version 5.1
<#
.SYNOPSIS
    Build the svgc release binary.
.DESCRIPTION
    Enters the matching MSVC environment when needed, then runs cargo build --release.
    Does not start the program.
.PARAMETER Target
    Rust target triple. Defaults to the host (no --target). Use
    x86_64-pc-windows-msvc or aarch64-pc-windows-msvc for an explicit arch.
.PARAMETER Dev
    Build with the debug profile instead of release.
#>
param(
    [string] $Target = "",
    [switch] $Dev
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
Set-Location -LiteralPath $PSScriptRoot

function Get-HostRustTarget {
    $raw = & rustc -vV 2>$null
    if ($LASTEXITCODE -ne 0) {
        throw "rustc -vV failed"
    }
    foreach ($line in $raw) {
        if ($line -match '^host:\s*(.+)$') {
            return $Matches[1].Trim()
        }
    }
    throw "Could not parse host from rustc -vV"
}

function Get-VsInstallPath {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath $vswhere)) {
        return $null
    }
    $path = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($path)) {
        $path = & $vswhere -latest -products * -property installationPath
    }
    if ([string]::IsNullOrWhiteSpace($path)) {
        return $null
    }
    return $path.Trim()
}

function Test-Arm64CrossClInstalled {
    param([string] $VsInstallPath)
    $msvcRoot = Join-Path $VsInstallPath "VC\Tools\MSVC"
    if (-not (Test-Path -LiteralPath $msvcRoot)) {
        return $false
    }
    $found = Get-ChildItem -LiteralPath $msvcRoot -Directory -ErrorAction SilentlyContinue |
        ForEach-Object { Join-Path $_.FullName "bin\Hostx64\arm64\cl.exe" } |
        Where-Object { Test-Path -LiteralPath $_ }
    return ($null -ne $found -and @($found).Count -gt 0)
}

function Import-VcVars {
    param(
        [Parameter(Mandatory = $true)]
        [string] $VcArch
    )

    $vs = Get-VsInstallPath
    if (-not $vs) {
        throw "Visual Studio with C++ tools was not found (vswhere)."
    }
    $vcvars = Join-Path $vs "VC\Auxiliary\Build\vcvarsall.bat"
    if (-not (Test-Path -LiteralPath $vcvars)) {
        throw "vcvarsall.bat not found at $vcvars"
    }

    if ($VcArch -in @("x64_arm64", "amd64_arm64", "arm64") -and -not (Test-Arm64CrossClInstalled -VsInstallPath $vs)) {
        throw @"
MSVC ARM64 build tools are missing (no Hostx64\arm64\cl.exe).
In Visual Studio Installer, add individual component:
  MSVC v*** - VS *** C++ ARM64/ARM64EC build tools
  (Microsoft.VisualStudio.Component.VC.Tools.ARM64)
Then re-run this script.
"@
    }

    $cmd = "`"$vcvars`" $VcArch >nul 2>&1 && set"
    $output = & cmd /c $cmd
    if ($LASTEXITCODE -ne 0) {
        throw "vcvarsall.bat $VcArch failed (exit $LASTEXITCODE). Install MSVC ARM64 build tools for aarch64 targets."
    }
    foreach ($line in $output) {
        if ($line -match '^(.*?)=(.*)$') {
            $name = $Matches[1]
            $value = $Matches[2]
            [System.Environment]::SetEnvironmentVariable($name, $value, "Process")
        }
    }
    Write-Host "MSVC env: vcvarsall $VcArch"
}

function Test-ClAvailable {
    $null -ne (Get-Command cl.exe -ErrorAction SilentlyContinue)
}

if ([string]::IsNullOrWhiteSpace($Target)) {
    $Target = Get-HostRustTarget
}

$supported = @(
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc"
)
if ($Target -notin $supported) {
    throw "Unsupported -Target '$Target'. Use: $($supported -join ', ')"
}

$hostTarget = Get-HostRustTarget
$useExplicitTarget = $Target -ne $hostTarget
$profile = if ($Dev) { "debug" } else { "release" }
$cargoTargetRoot = if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $env:CARGO_TARGET_DIR
} else {
    Join-Path $PSScriptRoot "target"
}
if ($useExplicitTarget) {
    $outDir = Join-Path $cargoTargetRoot "$Target\$profile"
} else {
    $outDir = Join-Path $cargoTargetRoot $profile
}

switch ($Target) {
    "x86_64-pc-windows-msvc" { $vcArch = "x64" }
    "aarch64-pc-windows-msvc" {
        if ($hostTarget -eq "aarch64-pc-windows-msvc") {
            $vcArch = "arm64"
        } else {
            $vcArch = "x64_arm64"
        }
    }
}

if ($Target -eq "aarch64-pc-windows-msvc" -or -not (Test-ClAvailable)) {
    Import-VcVars -VcArch $vcArch
}

if (-not (Test-ClAvailable)) {
    throw "cl.exe not on PATH after vcvarsall $vcArch"
}

$exe = Join-Path $outDir "svgc.exe"

function Get-BuiltProcess {
    @(Get-CimInstance Win32_Process | Where-Object {
        $_.ExecutablePath -and (
            [string]::Equals($_.ExecutablePath, $exe, [StringComparison]::OrdinalIgnoreCase)
        )
    })
}

$running = @(Get-BuiltProcess)
foreach ($proc in $running) {
    Write-Host "Stopping process $($proc.ProcessId): $($proc.ExecutablePath)"
    Stop-Process -Id $proc.ProcessId -Force -ErrorAction SilentlyContinue
}

if ($running.Count -gt 0) {
    $deadline = (Get-Date).AddSeconds(15)
    do {
        $left = @(Get-BuiltProcess)
        if ($left.Count -eq 0) { break }
        Start-Sleep -Milliseconds 200
    } while ((Get-Date) -lt $deadline)

    if ($left.Count -gt 0) {
        $paths = ($left | ForEach-Object { $_.ExecutablePath }) -join ", "
        throw "Process still running, cannot overwrite: $paths"
    }
}

$cargoArgs = @("build")
if (-not $Dev) {
    $cargoArgs += "--release"
}
if ($useExplicitTarget) {
    $cargoArgs += @("--target", $Target)
}
Write-Host "cargo $($cargoArgs -join ' ')"
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed with exit code $LASTEXITCODE"
}

if (-not (Test-Path -LiteralPath $exe)) {
    throw "svgc.exe was not produced at $exe"
}

Write-Host "Built $exe"
