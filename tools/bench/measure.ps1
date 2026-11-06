# Misura del budget a riposo (SPEC §3, §15.4). Avviare MicFX, chiudere la finestra e attendere che il
# keepalive scada, poi:
#   powershell -ExecutionPolicy Bypass -File tools\bench\measure.ps1 -Seconds 120
param(
    [int]$Seconds = 60,
    [string]$Process = 'micfx'
)

$p = Get-Process -Name $Process -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $p) { Write-Error "Processo $Process non trovato"; exit 1 }

function Get-Children([int]$ParentId) {
    Get-CimInstance Win32_Process -Filter "ParentProcessId = $ParentId" | ForEach-Object {
        $_
        Get-Children $_.ProcessId
    }
}

$cpu0 = $p.TotalProcessorTime
$cs0 = ($p.Threads | Measure-Object -Property Id).Count
# Contatore grezzo cumulativo per thread: i nomi delle classi WMI non sono localizzati.
function Get-ContextSwitches { (Get-CimInstance Win32_PerfRawData_PerfProc_Thread -Filter "Name LIKE '$Process/%'" | Measure-Object -Property ContextSwitchesPersec -Sum).Sum }
$ctx0 = Get-ContextSwitches
Start-Sleep -Seconds $Seconds
$p.Refresh()
$cpu1 = $p.TotalProcessorTime
$ctx1 = Get-ContextSwitches
$cores = [Environment]::ProcessorCount

$cpuPct = ($cpu1 - $cpu0).TotalMilliseconds / ($Seconds * 1000 * $cores) * 100
$ctxPerSec = ($ctx1 - $ctx0) / $Seconds
$ws = [math]::Round($p.WorkingSet64 / 1MB, 1)
$priv = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
$webviews = @(Get-Children $p.Id | Where-Object { $_.Name -like 'msedgewebview2*' }).Count

function Row($name, $value, $target, [bool]$ok) {
    $mark = if ($ok) { 'OK ' } else { 'NO ' }
    '{0} {1,-34} {2,-14} target {3}' -f $mark, $name, $value, $target
}

"MicFX a riposo, $Seconds s"
Row 'CPU media' ('{0:N3} %' -f $cpuPct) '0,00 %' ($cpuPct -lt 0.005)
Row 'Context switch del processo' ('{0:N2}/s' -f $ctxPerSec) '0/s' ($ctxPerSec -lt 0.5)
Row 'Working set' "$ws MB" '<= 20 MB' ($ws -le 20)
Row 'Private bytes' "$priv MB" '-' $true
Row 'Thread' $cs0 '-' $true
Row 'Processi msedgewebview2 figli' $webviews '0' ($webviews -eq 0)
''
'Per timer ed energia: powercfg /energy /duration 60 (prompt amministratore).'
