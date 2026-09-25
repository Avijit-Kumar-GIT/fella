param(
    [ValidateSet('Electron', 'Tauri')]
    [string]$Shell = 'Electron',
    [int]$Seconds = 8
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

function Require-Path([string]$Path, [string]$Hint) {
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "Missing $Path. $Hint"
    }
}

function Get-ProcessTree([int]$RootPid) {
    $snapshot = @(Get-CimInstance Win32_Process)
    $ids = [System.Collections.Generic.List[int]]::new()
    $queue = [System.Collections.Queue]::new()
    $ids.Add($RootPid)
    $queue.Enqueue($RootPid)

    while ($queue.Count -gt 0) {
        $parent = [int]$queue.Dequeue()
        foreach ($child in ($snapshot | Where-Object { $_.ParentProcessId -eq $parent })) {
            $childId = [int]$child.ProcessId
            if (-not $ids.Contains($childId)) {
                $ids.Add($childId)
                $queue.Enqueue($childId)
            }
        }
    }
    return $ids.ToArray()
}

if ($Shell -eq 'Electron') {
    $launcher = Join-Path $root 'node_modules/electron/dist/electron.exe'
    $entry = Join-Path $root 'electron/main.mjs'
    $engine = Join-Path $root 'src-tauri/target/release/fella.exe'
    Require-Path $launcher 'Run pnpm install first.'
    Require-Path $entry 'Run from the repository root.'
    Require-Path $engine 'Run pnpm electron:build first.'
    $arguments = @($entry)
    $env:FELLA_ENGINE_PATH = $engine
    $env:FELLA_DATA_DIR = Join-Path $env:TEMP 'fella-electron-memory-probe'
} else {
    $launcher = Join-Path $root 'src-tauri/target/release/fella.exe'
    Require-Path $launcher 'Run pnpm tauri build first.'
    $arguments = @()
}

$process = Start-Process -FilePath $launcher -ArgumentList $arguments -WorkingDirectory $root -PassThru
try {
    Start-Sleep -Seconds $Seconds
    if ($process.HasExited) {
        throw "$Shell exited before the memory sample was taken (code $($process.ExitCode))."
    }

    $ids = Get-ProcessTree $process.Id
    $rows = @(
        foreach ($id in $ids) {
            $item = Get-Process -Id $id -ErrorAction SilentlyContinue
            if ($null -ne $item) {
                [pscustomobject]@{
                    PID = $item.Id
                    Name = $item.ProcessName
                    WorkingSetMB = [math]::Round($item.WorkingSet64 / 1MB, 1)
                    PrivateMB = [math]::Round($item.PrivateMemorySize64 / 1MB, 1)
                    VirtualMB = [math]::Round($item.VirtualMemorySize64 / 1MB, 1)
                }
            }
        }
    )

    $rows | Sort-Object PID | Format-Table -AutoSize
    [pscustomobject]@{
        Shell = $Shell
        RootPID = $process.Id
        Processes = $rows.Count
        WorkingSetMB = [math]::Round(($rows | Measure-Object WorkingSetMB -Sum).Sum, 1)
        PrivateMB = [math]::Round(($rows | Measure-Object PrivateMB -Sum).Sum, 1)
        VirtualMB = [math]::Round(($rows | Measure-Object VirtualMB -Sum).Sum, 1)
        SampleAfterSeconds = $Seconds
    } | Format-List
} finally {
    if (-not $process.HasExited) {
        foreach ($id in (Get-ProcessTree $process.Id | Sort-Object -Descending)) {
            Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
        }
    }
}
