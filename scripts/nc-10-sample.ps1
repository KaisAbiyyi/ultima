param(
    [Parameter(Mandatory)][int]$RootProcessId,
    [Parameter(Mandatory)][string]$OutputFile,
    [Parameter(Mandatory)][string]$StopFile,
    [ValidateRange(1000,5000)][int]$IntervalMs = 1000,
    [ValidateRange(1000,3900000)][int]$DurationMs = 180000
)
$ErrorActionPreference = 'Stop'
$timer = [Diagnostics.Stopwatch]::StartNew()
$previousCpu = @{}
$previousTime = 0.0
$rootStarted = (Get-Process -Id $RootProcessId).StartTime.ToUniversalTime().Ticks
$writer = [IO.StreamWriter]::new($OutputFile, $false, [Text.UTF8Encoding]::new($false))
try {
    while ($timer.ElapsedMilliseconds -le $DurationMs -and -not (Test-Path -LiteralPath $StopFile)) {
        $rootProcess = Get-Process -Id $RootProcessId -ErrorAction SilentlyContinue
        if (-not $rootProcess -or $rootProcess.StartTime.ToUniversalTime().Ticks -ne $rootStarted) { break }
        $processRows = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name,CreationDate)
        $owned = [Collections.Generic.HashSet[int]]::new()
        [void]$owned.Add($RootProcessId)
        do {
            $changed = $false
            foreach ($row in $processRows) {
                if ($owned.Contains([int]$row.ParentProcessId) -and $row.CreationDate.ToUniversalTime().Ticks -ge $rootStarted) {
                    if ($owned.Add([int]$row.ProcessId)) { $changed = $true }
                }
            }
        } while ($changed)
        $elapsed = $timer.Elapsed.TotalMilliseconds
        $rss = 0L
        $cpuDelta = 0.0
        $currentCpu = @{}
        $tree = @()
        foreach ($processRow in $processRows) {
            if (-not $owned.Contains([int]$processRow.ProcessId)) { continue }
            $processValue = Get-Process -Id $processRow.ProcessId -ErrorAction SilentlyContinue
            if (-not $processValue) { continue }
            $identityKey = '{0}:{1}' -f $processValue.Id,$processValue.StartTime.ToUniversalTime().Ticks
            $cpuSeconds = $processValue.TotalProcessorTime.TotalSeconds
            $currentCpu[$identityKey] = $cpuSeconds
            if ($previousCpu.ContainsKey($identityKey)) { $cpuDelta += [Math]::Max(0, $cpuSeconds - $previousCpu[$identityKey]) }
            $rss += $processValue.WorkingSet64
            $tree += @{ pid = $processValue.Id; parentPid = [int]$processRow.ParentProcessId; name = $processValue.ProcessName; rssBytes = $processValue.WorkingSet64 }
        }
        $cpuPercent = if ($previousTime -gt 0 -and $elapsed -gt $previousTime) { $cpuDelta / (($elapsed - $previousTime) / 1000) * 100 } else { 0 }
        $writer.WriteLine((@{ elapsedMs = [Math]::Round($elapsed); rssBytes = $rss; cpuPercent = $cpuPercent; processCount = $tree.Count; tree = $tree } | ConvertTo-Json -Compress -Depth 4))
        $writer.Flush()
        $previousCpu = $currentCpu
        $previousTime = $elapsed
        Start-Sleep -Milliseconds $IntervalMs
    }
} finally { $writer.Dispose() }
