param(
    [Parameter(Mandatory)][int]$RootProcessId,
    [Parameter(Mandatory)][string]$ExpectedExecutable
)
$ErrorActionPreference = 'Stop'
$rootProcess = Get-Process -Id $RootProcessId
$rootStarted = $rootProcess.StartTime.ToUniversalTime().Ticks
$expectedPath = (Resolve-Path -LiteralPath $ExpectedExecutable).ProviderPath
$children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$RootProcessId" |
    Where-Object { $_.ExecutablePath -and $_.ExecutablePath.Equals($expectedPath, [StringComparison]::OrdinalIgnoreCase) -and $_.CreationDate.ToUniversalTime().Ticks -ge $rootStarted })
if ($children.Count -ne 1) { throw 'Expected exactly one owned runtime child; no process terminated' }
$childProcess = Get-Process -Id $children[0].ProcessId
# Win32_Process CreationDate has microsecond precision; Get-Process retains
# 100 ns ticks. Require the same microsecond as well as parent and image.
if (-not $childProcess.Path.Equals($expectedPath, [StringComparison]::OrdinalIgnoreCase) -or
    [Math]::Abs($childProcess.StartTime.ToUniversalTime().Ticks - $children[0].CreationDate.ToUniversalTime().Ticks) -ge 10) {
    $pathMatches = $childProcess.Path.Equals($expectedPath, [StringComparison]::OrdinalIgnoreCase)
    $tickDelta = $childProcess.StartTime.ToUniversalTime().Ticks - $children[0].CreationDate.ToUniversalTime().Ticks
    throw "Runtime child identity changed; no process terminated (path match=$pathMatches, creation tick delta=$tickDelta)"
}
Stop-Process -InputObject $childProcess -Force
@{ ownedRuntimeTerminated = $true; pid = $childProcess.Id } | ConvertTo-Json -Compress
