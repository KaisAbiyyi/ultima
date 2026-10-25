param()
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repoRoot
try {
    $sourceSha = (git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or (git status --porcelain)) { throw 'Qualification build requires a clean committed source tree' }
    npx.cmd tauri build --no-bundle -- --locked
    if ($LASTEXITCODE -ne 0) { throw 'Native build failed' }
    if ((git rev-parse HEAD).Trim() -ne $sourceSha -or (git status --porcelain)) { throw 'Source changed during build; no clean receipt' }
    $artifact = Join-Path $repoRoot 'src-tauri\target\release\ultima-desktop.exe'
    $receipt = @{
        schemaVersion = 1
        sourceSha = $sourceSha
        clean = $true
        artifactSha256 = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash.ToLowerInvariant()
        frontendLockSha256 = (Get-FileHash -LiteralPath 'package-lock.json' -Algorithm SHA256).Hash.ToLowerInvariant()
        nativeLockSha256 = (Get-FileHash -LiteralPath 'src-tauri\Cargo.lock' -Algorithm SHA256).Hash.ToLowerInvariant()
        builtAt = [DateTime]::UtcNow.ToString('o')
        signedPackagedCandidate = $false
    }
    [IO.File]::WriteAllText(($artifact + '.nc10-build.json'), ($receipt | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    Write-Output ('Native build receipt: ' + $artifact + '.nc10-build.json')
} finally { Pop-Location }
