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

function Get-ProcessTree([int]$RootPid, [int[]]$PreExistingPids) {
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

    # WebView2 can be brokered outside the Tauri process tree. Include its
    # browser/renderer processes when their user-data directory points at the
    # isolated app-data profile used for this probe.
    if ($Shell -eq 'Tauri') {
        $markers = @($env:LOCALAPPDATA, $env:APPDATA) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
        foreach ($candidate in ($snapshot | Where-Object { $_.Name -ieq 'msedgewebview2.exe' })) {
            $commandLine = [string]$candidate.CommandLine
            $matchesProfile = $false
            foreach ($marker in $markers) {
                if ($commandLine.IndexOf($marker, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
                    $matchesProfile = $true
                    break
                }
            }
            $isNewProcess = -not ($PreExistingPids -contains [int]$candidate.ProcessId)
            if (($matchesProfile -or $isNewProcess) -and -not $ids.Contains([int]$candidate.ProcessId)) {
                $ids.Add([int]$candidate.ProcessId)
                $queue.Enqueue([int]$candidate.ProcessId)
            }
        }

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
    }
    return $ids.ToArray()
}

$preExistingPids = @(Get-CimInstance Win32_Process | ForEach-Object { [int]$_.ProcessId })
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

if ($Shell -eq 'Electron') {
    $process = Start-Process -FilePath $launcher -ArgumentList $arguments -WorkingDirectory $root -PassThru
} else {
    $process = Start-Process -FilePath $launcher -WorkingDirectory $root -PassThru
}
try {
    Start-Sleep -Seconds $Seconds
    if ($process.HasExited) {
        throw "$Shell exited before the memory sample was taken (code $($process.ExitCode))."
    }

    $ids = Get-ProcessTree $process.Id $preExistingPids
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

    if ($Shell -eq 'Tauri' -and -not ($rows | Where-Object { $_.Name -ieq 'msedgewebview2' })) {
        Write-Warning 'No WebView2 process was discovered. The Tauri memory total may exclude the WebView2 runtime.'
    }
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
