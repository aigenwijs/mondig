# Meet laadtijd en geheugen voor het laadbeleid (ADR 0012, sprint 3a). Alleen Windows.
#
#   powershell -File scripts/meet-laden.ps1 -Audio <wav>
#
# 1. Het werkproces: laadtijd, de eerste en de volgende uitschrijvingen, en het geheugen
#    (prive-bytes en werkset apart) met een warm model in rust.
# 2. De app zelf in rust, zonder werkproces: Mondig plus de WebView2-processen eronder.
#
# Na een herstart van de pc meet stap 1 de koude start (model nog niet in de bestandscache).
# Bouw eerst: cargo build --release -p mondig-engine -p mondig-cli --example meet_laden, en in app/: npx tauri build --no-bundle
# (een kale cargo build van de app laadt de interface van de ontwikkelserver).

param(
    [Parameter(Mandatory = $true)][string]$Audio,
    [int]$Herhalingen = 3,
    [int]$RustSeconden = 15,
    [int]$AppSeconden = 20
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$release = Join-Path $root 'target\release'
$engine = Join-Path $release 'mondig-engine.exe'

function Mb([double]$bytes) { [math]::Round($bytes / 1MB, 0) }

# Geheugen van een proces en alle nakomelingen (WebView2 draait in eigen processen).
function Boom([int]$procesId) {
    $alle = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
    $ids = [System.Collections.Generic.List[int]]::new()
    $ids.Add($procesId)
    for ($i = 0; $i -lt $ids.Count; $i++) {
        foreach ($p in $alle) { if ($p.ParentProcessId -eq $ids[$i]) { $ids.Add([int]$p.ProcessId) } }
    }
    $prive = 0; $werkset = 0
    foreach ($id in $ids) {
        $p = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($p) { $prive += $p.PrivateMemorySize64; $werkset += $p.WorkingSet64 }
    }
    [pscustomobject]@{ Processen = $ids.Count; PriveMB = (Mb $prive); WerksetMB = (Mb $werkset) }
}

# Een draaiende Mondig (bijvoorbeeld in de systeembalk) verstoort beide metingen: stap 1 zou
# zijn werkproces kunnen meten, en in stap 2 geeft single-instance het stokje door en sluit.
if (Get-Process -Name 'mondig', 'mondig-engine' -ErrorAction SilentlyContinue) {
    Write-Host 'Mondig draait nog. Sluit het via de systeembalk (Afsluiten) en start dit script opnieuw.'
    exit 1
}

# Het werkproces van het meetvoorbeeld: een kind van dat proces, niet zomaar een mondig-engine.
function Werkproces([int]$ouder) {
    $kind = Get-CimInstance Win32_Process -Filter "ParentProcessId=$ouder AND Name='mondig-engine.exe'" |
        Select-Object -First 1
    if ($kind) { Get-Process -Id $kind.ProcessId -ErrorAction SilentlyContinue }
}

Write-Host '== 1. Werkproces: laden, uitschrijven, rust =='
$uit = Join-Path $env:TEMP 'mondig-meet-laden.txt'
Remove-Item $uit -ErrorAction SilentlyContinue
# Start-Process plakt de argumenten met spaties aan elkaar, dus paden tussen aanhalingstekens.
$meet = Start-Process -FilePath (Join-Path $release 'examples\meet_laden.exe') -NoNewWindow -PassThru `
    -RedirectStandardOutput $uit `
    -ArgumentList @("`"$Audio`"", "`"$engine`"", $Herhalingen, $RustSeconden)

$piek = 0
while (-not $meet.HasExited) {
    $e = Werkproces $meet.Id
    if ($e) { $piek = [math]::Max($piek, $e.PrivateMemorySize64) }
    if ((Test-Path $uit) -and (Select-String -Path $uit -Pattern '^rust_s=' -Quiet)) { break }
    Start-Sleep -Milliseconds 100
}
Get-Content $uit
# Even laten bezinken, dan het werkproces met warm model in rust bemonsteren.
Start-Sleep -Seconds 3
$e = Werkproces $meet.Id
if ($e) {
    Write-Host ("werkproces_piek_prive_MB={0}" -f (Mb $piek))
    Write-Host ("werkproces_rust_prive_MB={0}" -f (Mb $e.PrivateMemorySize64))
    Write-Host ("werkproces_rust_werkset_MB={0}" -f (Mb $e.WorkingSet64))
} else {
    Write-Host 'Het werkproces draait niet meer; geen geheugenmeting.'
}
$meet.WaitForExit()

Write-Host "`n== 2. App in rust, zonder werkproces =="
$app = Start-Process -FilePath (Join-Path $release 'mondig.exe') -PassThru
Start-Sleep -Seconds $AppSeconden
if ($app.HasExited) {
    Write-Host 'De app sloot zichzelf direct af (draaide er toch al een?); geen app-meting.'
    exit 1
}
$boom = Boom $app.Id
Write-Host ("app_processen={0}" -f $boom.Processen)
Write-Host ("app_rust_prive_MB={0}" -f $boom.PriveMB)
Write-Host ("app_rust_werkset_MB={0}" -f $boom.WerksetMB)
Write-Host ("engine_draait={0}" -f [bool](Get-Process -Name 'mondig-engine' -ErrorAction SilentlyContinue))
Stop-Process -Id $app.Id
