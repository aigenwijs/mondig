# Meet het geheugen als dicteren en bestanden elk een eigen model hebben (sprint 5-A, ADR 0012).
# Alleen Windows.
#
#   powershell -File scripts/meet-twee-modellen.ps1 -Audio <wav>
#
# Het dicteerwerkproces draait Parakeet Ultra (warm, na een eerste dictaat), het
# achtergrondwerkproces schrijft intussen bestanden uit met Canary, en er wordt gedicteerd
# terwijl dat loopt. Per werkproces: de piek in prive-bytes en in werkset (de werkset telt de
# via mmap geladen modelgewichten mee, die het besturingssysteem bij geheugendruk zelf
# terugneemt) en het laagste vrije geheugen van de hele computer.
#
# Bouw eerst: cargo build --release -p mondig-engine -p mondig-cli --example meet_twee_modellen

param(
    [Parameter(Mandatory = $true)][string]$Audio,
    [int]$Herhalingen = 12,
    [int]$RustSeconden = 8,
    # Standaard de release-bouw in deze repo; geef andere paden mee om bijvoorbeeld een
    # werkproces uit een andere map te meten.
    [string]$Engine = '',
    [string]$Voorbeeld = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$release = Join-Path $root 'target\release'
if ($Engine -eq '') { $Engine = Join-Path $release 'mondig-engine.exe' }
if ($Voorbeeld -eq '') { $Voorbeeld = Join-Path $release 'examples\meet_twee_modellen.exe' }

function Mb([double]$bytes) { [math]::Round($bytes / 1MB, 0) }
function VrijMb { [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1KB, 0) }

# De werkprocessen worden via het voorbeeld gevonden, dus een draaiende Mondig stoort die meting
# niet; het vrije geheugen van de hele computer wel.
if (Get-Process -Name 'mondig', 'mondig-engine' -ErrorAction SilentlyContinue) {
    Write-Host 'Let op: Mondig draait nog; het cijfer voor de hele computer is dan minder zuiver.'
}

$totaal = [math]::Round((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB, 1)
$vrijVoor = VrijMb
Write-Host ("computer_geheugen_GB={0}" -f $totaal)
Write-Host ("vrij_voor_MB={0}" -f $vrijVoor)

$uit = Join-Path $env:TEMP 'mondig-meet-twee-modellen.txt'
Remove-Item $uit -ErrorAction SilentlyContinue
$meet = Start-Process -FilePath $Voorbeeld -NoNewWindow -PassThru `
    -RedirectStandardOutput $uit `
    -ArgumentList @("`"$Audio`"", "`"$Engine`"", $Herhalingen, $RustSeconden)

# De werkprocessen van het voorbeeld, in de volgorde van starten: eerst dicteren, dan bestanden.
function Werkprocessen([int]$ouder) {
    Get-CimInstance Win32_Process -Filter "ParentProcessId=$ouder AND Name='mondig-engine.exe'" |
        Sort-Object CreationDate |
        ForEach-Object { Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }
}

$piek = @(@{ Prive = 0; Werkset = 0 }, @{ Prive = 0; Werkset = 0 })
$alleenDicterenPrive = $null; $alleenDicterenWerkset = $null
$laagsteVrij = $vrijVoor
$gezien = $false
while (-not $meet.HasExited) {
    $procs = @(Werkprocessen $meet.Id)
    for ($i = 0; $i -lt [math]::Min(2, $procs.Count); $i++) {
        $piek[$i].Prive = [math]::Max($piek[$i].Prive, $procs[$i].PrivateMemorySize64)
        $piek[$i].Werkset = [math]::Max($piek[$i].Werkset, $procs[$i].WorkingSet64)
    }
    $laagsteVrij = [math]::Min($laagsteVrij, (VrijMb))
    # Voordat het tweede werkproces er is: alleen het dicteermodel, warm.
    if ($procs.Count -eq 1 -and (Test-Path $uit) -and (Select-String -Path $uit -Pattern '^dicteren_warm=' -Quiet)) {
        $alleenDicterenPrive = $procs[0].PrivateMemorySize64
        $alleenDicterenWerkset = $procs[0].WorkingSet64
    }
    # In rust met beide modellen geladen: de waarden na het uitschrijven.
    if (-not $gezien -and (Test-Path $uit) -and (Select-String -Path $uit -Pattern '^rust_s=' -Quiet)) {
        Start-Sleep -Seconds 2
        $procs = @(Werkprocessen $meet.Id)
        if ($procs.Count -ge 2) {
            Write-Host ("rust_dicteren_prive_MB={0}" -f (Mb $procs[0].PrivateMemorySize64))
            Write-Host ("rust_dicteren_werkset_MB={0}" -f (Mb $procs[0].WorkingSet64))
            Write-Host ("rust_bestanden_prive_MB={0}" -f (Mb $procs[1].PrivateMemorySize64))
            Write-Host ("rust_bestanden_werkset_MB={0}" -f (Mb $procs[1].WorkingSet64))
            Write-Host ("rust_vrij_MB={0}" -f (VrijMb))
        }
        $gezien = $true
    }
    Start-Sleep -Milliseconds 100
}
Get-Content $uit
if ($null -ne $alleenDicterenPrive) {
    Write-Host ("alleen_dicteren_prive_MB={0}" -f (Mb $alleenDicterenPrive))
    Write-Host ("alleen_dicteren_werkset_MB={0}" -f (Mb $alleenDicterenWerkset))
}
Write-Host ("piek_dicteren_prive_MB={0}" -f (Mb $piek[0].Prive))
Write-Host ("piek_dicteren_werkset_MB={0}" -f (Mb $piek[0].Werkset))
Write-Host ("piek_bestanden_prive_MB={0}" -f (Mb $piek[1].Prive))
Write-Host ("piek_bestanden_werkset_MB={0}" -f (Mb $piek[1].Werkset))
Write-Host ("piek_samen_prive_MB={0}" -f (Mb ($piek[0].Prive + $piek[1].Prive)))
Write-Host ("piek_samen_werkset_MB={0}" -f (Mb ($piek[0].Werkset + $piek[1].Werkset)))
Write-Host ("laagste_vrij_MB={0}" -f $laagsteVrij)
Write-Host ("verbruik_computer_MB={0}" -f ($vrijVoor - $laagsteVrij))
