param([string]$Target, [string]$New, [string]$Stage, [int]$ParentId, [string]$WorkingDirectory)
$ErrorActionPreference = 'Stop'
$Backup = Join-Path $Stage 'previous.exe'
$Report = Join-Path $Stage 'error.txt'
try {
    $ParentProcess = Get-Process -Id $ParentId -ErrorAction SilentlyContinue
    if ($ParentProcess) { $ParentProcess | Wait-Process -Timeout 90 }
    Move-Item -LiteralPath $Target -Destination $Backup
    try { Move-Item -LiteralPath $New -Destination $Target }
    catch { Move-Item -LiteralPath $Backup -Destination $Target; throw }
    Start-Process -FilePath $Target -WorkingDirectory $WorkingDirectory -ArgumentList '--skip-update-once'
    Remove-Item -LiteralPath $Backup -ErrorAction SilentlyContinue
    if (([IO.Path]::GetDirectoryName($Stage) -eq [IO.Path]::GetDirectoryName($Target)) -and
        [IO.Path]::GetFileName($Stage).StartsWith('.mtgo-update-')) {
        Remove-Item -LiteralPath $Stage -Recurse -Force -ErrorAction SilentlyContinue
    }
} catch {
    if (Test-Path -LiteralPath $Backup) {
        if (Test-Path -LiteralPath $Target) { Remove-Item -LiteralPath $Target }
        Move-Item -LiteralPath $Backup -Destination $Target
    }
    $_.ToString() | Set-Content -LiteralPath $Report -Encoding UTF8
    Start-Process -FilePath $Target -WorkingDirectory $WorkingDirectory -ArgumentList @('--skip-update-once', '--update-error', ('"' + $Report + '"'))
}
