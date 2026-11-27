$ErrorActionPreference = 'Stop'
$root = 'D:\kaifa-soft\_build\test-env'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$boxRoot = Join-Path $root 'boxes'
New-Item -ItemType Directory -Force -Path $boxRoot | Out-Null
$box = Join-Path $boxRoot 'Demo'
New-Item -ItemType Directory -Force -Path $box | Out-Null

# --- layer 1: parent, has the program + base data
New-Item -ItemType Directory -Force -Path (Join-Path $box 'snapshot-1\drive\C\App') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $box 'snapshot-1\user\current') | Out-Null
Set-Content -Path (Join-Path $box 'snapshot-1\drive\C\App\app.exe') -Value 'PARENT-EXE' -NoNewline
Set-Content -Path (Join-Path $box 'snapshot-1\drive\C\App\shared.txt') -Value 'from-parent' -NoNewline
Set-Content -Path (Join-Path $box 'snapshot-1\user\current\state.txt') -Value 'parent-state' -NoNewline
Set-Content -Path (Join-Path $box 'snapshot-1\RegHive') -Value 'HIVE-1' -NoNewline

# --- layer 2: child, only changed data
New-Item -ItemType Directory -Force -Path (Join-Path $box 'snapshot-2\user\current') | Out-Null
Set-Content -Path (Join-Path $box 'snapshot-2\user\current\state.txt') -Value 'child-state' -NoNewline
Set-Content -Path (Join-Path $box 'snapshot-2\RegHive') -Value 'HIVE-2' -NoNewline

# --- deleted file recorded in layer 2
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$paths = "\drive\C\App\shared.txt|1`r`n"
[System.IO.File]::WriteAllBytes((Join-Path $box 'snapshot-2\FilePaths.dat'), $utf8NoBom.GetBytes($paths))

$ini = @"
[Snapshot_1]
Name=base
SnapshotDate=1795455407

[Snapshot_2]
Name=child
SnapshotDate=1795457407
Parent=1

[Current]
Snapshot=2

"@
[System.IO.File]::WriteAllText((Join-Path $box 'Snapshots.ini'), $ini, $utf8NoBom)

$sbieIni = @"
[GlobalSettings]
BoxRootFolder=$boxRoot

[Demo]
FileRootPath=$boxRoot\Demo

"@
[System.IO.File]::WriteAllText((Join-Path $root 'Sandboxie.ini'), $sbieIni, $utf8NoBom)
Write-Host 'test env ready'
