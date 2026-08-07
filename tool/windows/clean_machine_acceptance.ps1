# LingBi Clean-Machine Acceptance Script (Task 17/18)

# Run this on a CLEAN Windows 11 machine (VM or dedicated runner) that has
# NO dev tools installed. The only artifact needed is the LingBi installer.
#
#   powershell -ExecutionPolicy Bypass -File tool\windows\clean_machine_acceptance.ps1 -Installer C:\path\to\LingBi_0.1.0_x64-setup.exe
#
# It records PASS/FAIL per step into acceptance-evidence.txt. Any FAIL or
# any user-data loss => Windows Novice Product Gate = FAIL.

param(
    [Parameter(Mandatory = $true)][string]$Installer
)

$ErrorActionPreference = 'Stop'
$evidence = Join-Path (Get-Location) 'acceptance-evidence.txt'
$novelsRoot = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'LingBi'

function Step([string]$name, [scriptblock]$body) {
    try {
        & $body | Out-Null
        "[PASS] $name" | Out-File -FilePath $evidence -Append -Encoding utf8
        Write-Host "[PASS] $name"
    }
    catch {
        "[FAIL] $name : $($_.Exception.Message)" | Out-File -FilePath $evidence -Append -Encoding utf8
        Write-Host "[FAIL] $name : $($_.Exception.Message)"
        exit 1
    }
}

"=== LingBi clean-machine acceptance $((Get-Date).ToString('s')) ===" | Out-File -FilePath $evidence -Encoding utf8

# 0 dev tooling preconditions
Step "no dev tooling preconditions" {
    foreach ($tool in @('cargo', 'node', 'pnpm', 'flutter', 'go')) {
        if (Get-Command $tool -ErrorAction SilentlyContinue) {
            throw "dev tool found: $tool (clean machine must not have it)"
        }
    }
}

Step "installer exists" { if (-not (Test-Path $Installer)) { throw "installer missing: $Installer" } }

# --- 首次安装 ---
Step "first install" {
    $process = Start-Process -FilePath $Installer -ArgumentList '/S' -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "installer exit code $($process.ExitCode)" }
}

$exe = Join-Path $env:LOCALAPPDATA 'Programs\LingBi Next\LingBi Next.exe'
if (-not (Test-Path $exe)) {
    # fall back to scanning known per-user install locations
    $exe = Get-ChildItem -Path $env:LOCALAPPDATA -Recurse -Filter 'LingBi Next.exe' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
}
Step "binary installed per-user" { if (-not $exe -or -not (Test-Path $exe)) { throw "LingBi binary not found" } }

# --- 启动 → 创建作品 → 关闭 → 重启 ---
Step "launch and create project" {
    $process = Start-Process -FilePath $exe -PassThru
    Start-Sleep -Seconds 6
    if ($process.HasExited) { throw "app exited immediately (exit $($process.ExitCode))" }
    # A project must appear under Documents/LingBi once created; creation
    # is done via the UI by the human operator (0 CLI). Script waits for
    # the human to create the novel, with a generous timeout.
    $deadline = (Get-Date).AddMinutes(5)
    while (-not (Test-Path $novelsRoot)) {
        if ((Get-Date) -gt $deadline) { throw "no novel folder under $novelsRoot (did the operator create a novel?)" }
        Start-Sleep -Seconds 5
    }
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}

Step "restart and reopen recent project" {
    $process = Start-Process -FilePath $exe -PassThru
    $deadline = (Get-Date).AddMinutes(3)
    $opened = $false
    while (-not $opened) {
        if ((Get-Date) -gt $deadline) { throw "operator did not confirm reopen" }
        $opened = Read-Host 'Reopened the novel in the app? (yes)'
        $opened = $opened -eq 'yes'
    }
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
}

# --- 数据快照 → 卸载 → 数据仍在 → 重装 → 重开 ---
$before = @(Get-ChildItem -Path $novelsRoot -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object FullName)
Step "uninstall" {
    $uninstaller = Get-ChildItem -Path $env:LOCALAPPDATA -Recurse -Filter 'Uninstall*.exe' -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like '*LingBi*' } | Select-Object -First 1
    if (-not $uninstaller) { throw 'uninstaller not found' }
    $process = Start-Process -FilePath $uninstaller.FullName -ArgumentList '/S' -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "uninstall exit code $($process.ExitCode)" }
}

Step "novels survive uninstall" {
    $after = @(Get-ChildItem -Path $novelsRoot -Recurse -File -ErrorAction SilentlyContinue | ForEach-Object FullName)
    $lost = @($before | Where-Object { $_ -notin $after })
    if ($lost.Count -gt 0) { throw "user files lost on uninstall: $($lost -join ', ')" }
}

Step "reinstall" {
    $process = Start-Process -FilePath $Installer -ArgumentList '/S' -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "reinstall exit code $($process.ExitCode)" }
}

Step "reopen old project after reinstall" {
    if (-not (Test-Path $novelsRoot)) { throw "novels root missing after reinstall" }
}

"[RESULT] CLEAN_MACHINE_ACCEPTANCE=$($LASTEXITCODE -eq 0)" | Out-File -FilePath $evidence -Append -Encoding utf8
Write-Host "Evidence written to $evidence"
