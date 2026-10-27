# NC-10D Windows UIA *diagnostic* cold startup (NOT screen-pixel presentation).
# Run after building a clean, exact HEAD with build-exact.mjs. Three independent
# native launches with no model turn or owner profile; avoids fake cold-cache
# or compositor-visible acceptance claims. Close only processes we launched.
param(
    [Parameter(Mandatory)][string]$OutputFile,
    [ValidateRange(3, 10)][int]$Repetitions = 3
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$exe = Join-Path $PSScriptRoot 'target\release\ultima-native-renderer-spike.exe'
$receiptFile = Join-Path $PSScriptRoot 'target\release\nc10-build-receipt.json'
$exact = Get-Content -LiteralPath $receiptFile -Raw | ConvertFrom-Json
$source = (& git -C $root rev-parse HEAD).Trim()
if (@(& git -C $root status --porcelain --untracked-files=all).Count -ne 0) {
    throw 'Dirty source cannot produce an exact-head native cold-start diagnostic'
}
if ($source -ne $exact.sourceSha) { throw 'Source SHA not equal to exact build' }
$exeHash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
if ($exeHash -ne $exact.binarySha256) { throw 'Binary SHA no longer matches receipt' }
if (Test-Path -LiteralPath $OutputFile) { throw 'Refusing to overwrite an existing diagnostic' }
$parent = Split-Path -Parent $OutputFile
if (-not (Test-Path -LiteralPath $parent)) { throw 'Output directory must already exist' }
Add-Type -AssemblyName UIAutomationClient
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class Nc10dWindow {
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr handle, uint message, IntPtr wparam, IntPtr lparam);
}
'@
$measurements = @()
for ($i = 1; $i -le $Repetitions; $i++) {
    $elapsed = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $exe -ArgumentList @('--fixture-empty') -PassThru
    $composerFound = $false
    $sampleMs = $null
    try {
        for ($attempt = 0; $attempt -lt 200; $attempt++) {
            Start-Sleep -Milliseconds 50
            $process.Refresh()
            if ($process.HasExited) { break }
            if ($process.MainWindowHandle -eq [IntPtr]::Zero) { continue }
            try {
                $window = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
                $filter = [System.Windows.Automation.PropertyCondition]::new(
                    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
                    [System.Windows.Automation.ControlType]::Edit
                )
                $composer = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $filter)
                if ($null -ne $composer -and $composer.Current.IsEnabled) {
                    $composerFound = $true
                    $sampleMs = $elapsed.Elapsed.TotalMilliseconds
                    break
                }
            } catch {
                # Winit/egui may still be initializing the accessibility tree.
            }
        }
        if (-not $composerFound) { throw 'UIA composer did not become accessible' }
    } finally {
        $process.Refresh()
        if (-not $process.HasExited -and $process.MainWindowHandle -ne [IntPtr]::Zero) {
            [void][Nc10dWindow]::PostMessage($process.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
            if (-not $process.WaitForExit(10000)) {
                Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
                throw 'Native process did not close cleanly'
            }
        } elseif (-not $process.HasExited) {
            Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            throw 'Native process had no controlled closeable window'
        }
        if ($process.ExitCode -ne 0) { throw 'Native process returned nonzero exit code' }
    }
    $measurements += [PSCustomObject]@{
        repetition = $i; elapsedSpawnToAccessibleComposerMs = [Math]::Round($sampleMs, 3)
        composerAccessible = $composerFound; processClosedNormally = $true
    }
}
$sorted = @($measurements | ForEach-Object { $_.elapsedSpawnToAccessibleComposerMs } | Sort-Object)
$rank = [Math]::Max(0, [Math]::Ceiling(.95 * $sorted.Length) - 1)
$summary = [ordered]@{
    kind = 'NC10D_NATIVE_WIN_UIA_STARTUP_DIAGNOSTIC'
    sourceSha = $source
    binarySha256 = $exeHash
    cliVersion = 'NOT_RUN'
    runCount = $Repetitions
    cache = 'warm/uncontrolled'
    stage = 'process spawn to UIA composer accessible; not visible pixels'
    presentationVerified = $false
    p95Ms = $sorted[$rank]
    samples = $measurements
    physicalColdStartAcceptance = 'NOT_RUN'
}
$contents = ($summary | ConvertTo-Json -Depth 5)
[IO.File]::WriteAllText((Join-Path $parent (Split-Path -Leaf $OutputFile)), $contents, [Text.UTF8Encoding]::new($false))
Write-Output ($summary | ConvertTo-Json -Compress -Depth 5)
