#!/usr/bin/env node
// Bundelscript voor het werkproces (ADR 0012, "Bundelen op Windows"; macOS: ADR 0029).
// Op Windows bouwt het werkproces mondig-engine in release en zet alles klaar in app/src-tauri:
//
//   binaries/mondig-engine-<target-triple>.exe   (bundle.externalBin)
//   engine-libs/                                  (bundle.resources, naast Mondig.exe)
//
// engine-libs bevat een vaste lijst: transcribe.dll en ggml*.dll uit de build, plus de
// C++-runtime (msvcp140*.dll en vcruntime140*.dll) uit dezelfde Visual Studio-installatie.
// Geen jokerteken op *.dll: een oud bestand als DirectML.dll mag er niet in sluipen.
// Ontbreekt een verplicht bestand, dan stopt het script met een foutmelding, zodat er
// nooit een installer zonder runtime ontstaat (zoals Handy's stage_vc_runtime_dlls).
//
//   node scripts/engine-bundelen.mjs
//
// Op macOS (alleen Apple Silicon, aarch64-apple-darwin) bouwt het script het werkproces met
// Metal en zet het klaar als binaries/mondig-engine-aarch64-apple-darwin. Er komt geen
// engine-libs bij: de rekenmodules en de Metal-shaders (metallib) zitten statisch in het
// werkproces zelf (transcribe-cpp met de feature `metal`, GGML_METAL_EMBED_LIBRARY=ON in
// transcribe-cpp-sys/bindings/rust/sys/build.rs). Het script controleert met otool dat het
// werkproces alleen naar systeembibliotheken verwijst.
//
// MONDIG_VC_REDIST_DIR wijst handmatig naar de map met de runtime, bijvoorbeeld
// C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Redist\MSVC\14.44.35112\x64\Microsoft.VC143.CRT

import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
// Dezelfde map als cargo: CARGO_TARGET_DIR als die gezet is (relatief aan de repo, want cargo
// draait hieronder met cwd root), anders target/.
const release = join(resolve(root, process.env.CARGO_TARGET_DIR || 'target'), 'release');
const binMap = join(root, 'app', 'src-tauri', 'binaries');
const libMap = join(root, 'app', 'src-tauri', 'engine-libs');

const stop = (melding) => {
  console.error(`Fout: ${melding}`);
  process.exit(1);
};

if (process.platform !== 'win32' && process.platform !== 'darwin') {
  console.error(
    'Dit script bundelt alleen voor Windows en macOS (Apple Silicon). Bundelen voor Linux volgt later.'
  );
  process.exit(1);
}

/** Zoekt de map met de C++-runtime van de nieuwste Visual Studio-installatie. */
function zoekRedistMap() {
  const handmatig = process.env.MONDIG_VC_REDIST_DIR;
  if (handmatig) {
    if (!existsSync(handmatig)) stop(`MONDIG_VC_REDIST_DIR bestaat niet: ${handmatig}`);
    return handmatig;
  }
  const programmas = process.env['ProgramFiles(x86)'] ?? 'C:\Program Files (x86)';
  const vswhere = join(programmas, 'Microsoft Visual Studio', 'Installer', 'vswhere.exe');
  if (!existsSync(vswhere)) {
    stop(`vswhere.exe niet gevonden (${vswhere}). Installeer Visual Studio Build Tools of zet MONDIG_VC_REDIST_DIR.`);
  }
  const installatie = execFileSync(
    vswhere,
    [
      '-latest',
      '-products',
      '*',
      '-requires',
      'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
      '-property',
      'installationPath'
    ],
    { encoding: 'utf8' }
  )
    .split(/\r?\n/)[0]
    .trim();
  if (!installatie) stop('Geen Visual Studio met C++-gereedschap gevonden. Zet anders MONDIG_VC_REDIST_DIR.');

  const msvc = join(installatie, 'VC', 'Redist', 'MSVC');
  if (!existsSync(msvc)) stop(`Redist-map ontbreekt: ${msvc}`);
  const versies = readdirSync(msvc)
    .filter((naam) => /^\d+(\.\d+)+$/.test(naam))
    .sort((a, b) => {
      const [pa, pb] = [a, b].map((v) => v.split('.').map(Number));
      for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
        const verschil = (pb[i] ?? 0) - (pa[i] ?? 0);
        if (verschil !== 0) return verschil;
      }
      return 0;
    });
  for (const versie of versies) {
    const x64 = join(msvc, versie, 'x64');
    if (!existsSync(x64)) continue;
    const crt = readdirSync(x64).find((naam) => /^Microsoft\.VC14\d\.CRT$/.test(naam));
    if (crt) return join(x64, crt);
  }
  return stop(`Geen Microsoft.VC14x.CRT gevonden onder ${msvc}\<versie>\x64.`);
}

/** Bestanden in `map` waarvan de naam (kleine letters) aan `test` voldoet. */
function bestanden(map, test) {
  return readdirSync(map).filter((naam) => test(naam.toLowerCase()));
}

/** Leegt een map en maakt hem opnieuw aan. */
function leegMap(map) {
  rmSync(map, { recursive: true, force: true });
  mkdirSync(map, { recursive: true });
}

/** macOS (Apple Silicon): werkproces met Metal bouwen en klaarzetten voor bundle.externalBin. */
function bundelMac() {
  if (process.arch !== 'arm64') {
    stop('Bundelen voor macOS kan alleen op Apple Silicon (arm64). Intel-Macs worden niet ondersteund (ADR 0029).');
  }
  const triple = execFileSync('rustc', ['--print', 'host-tuple'], { encoding: 'utf8' }).trim();
  if (triple !== 'aarch64-apple-darwin') stop(`Onverwachte target-triple: ${triple} (verwacht aarch64-apple-darwin).`);

  // Zelfde minimumversie als de app (tauri.conf.json), zodat C en C++ van de engine voor dezelfde
  // macOS-versie bouwen als de rest van de .app.
  const config = JSON.parse(readFileSync(join(root, 'app', 'src-tauri', 'tauri.conf.json'), 'utf8'));
  const minimum = config?.bundle?.macOS?.minimumSystemVersion;
  if (!minimum) stop('bundle.macOS.minimumSystemVersion ontbreekt in tauri.conf.json.');

  console.log(`Werkproces bouwen (release, Metal, macOS ${minimum} of nieuwer)...`);
  execFileSync('cargo', ['build', '--release', '-p', 'mondig-engine'], {
    cwd: root,
    stdio: 'inherit',
    env: { ...process.env, MACOSX_DEPLOYMENT_TARGET: minimum }
  });
  const enginePad = join(release, 'mondig-engine');
  if (!existsSync(enginePad)) stop(`${enginePad} ontbreekt na de build.`);

  // Controle: alleen systeembibliotheken. Een verwijzing naar @rpath of een andere map zou
  // betekenen dat er een dylib mee moet die de .app niet bevat.
  const regels = execFileSync('otool', ['-L', enginePad], { encoding: 'utf8' }).split(/\r?\n/).slice(1);
  const vreemd = regels
    .map((regel) => regel.trim().split(' ')[0])
    .filter((pad) => pad && !pad.startsWith('/usr/lib/') && !pad.startsWith('/System/Library/'));
  if (vreemd.length > 0) stop(`Het werkproces verwijst naar bibliotheken buiten het systeem: ${vreemd.join(', ')}`);

  leegMap(binMap);
  // Geen engine-libs op de Mac; een restant van een Windows-build mag niet in de .app.
  rmSync(libMap, { recursive: true, force: true });
  copyFileSync(enginePad, join(binMap, `mondig-engine-${triple}`));
  console.log(`binaries/mondig-engine-${triple}`);
}

if (process.platform === 'darwin') {
  bundelMac();
  process.exit(0);
}

// 1. Werkproces bouwen
console.log('Werkproces bouwen (release)...');
execFileSync('cargo', ['build', '--release', '-p', 'mondig-engine'], { cwd: root, stdio: 'inherit' });

const enginePad = join(release, 'mondig-engine.exe');
if (!existsSync(enginePad)) stop(`${enginePad} ontbreekt na de build.`);

// 2. Doelmappen leeg en opnieuw vullen
leegMap(binMap);
leegMap(libMap);

const triple = execFileSync('rustc', ['--print', 'host-tuple'], { encoding: 'utf8' }).trim();
if (!triple) stop('rustc gaf geen target-triple terug.');
copyFileSync(enginePad, join(binMap, `mondig-engine-${triple}.exe`));
console.log(`binaries/mondig-engine-${triple}.exe`);

// 3. Rekenmodules uit de build: een vaste lijst, geen jokerteken op *.dll
const rekenmodules = bestanden(release, (n) => n === 'transcribe.dll' || /^ggml.*\.dll$/.test(n));
const verplichtRekenen = ['transcribe.dll', 'ggml.dll', 'ggml-base.dll', 'ggml-vulkan.dll'];
for (const naam of verplichtRekenen) {
  if (!rekenmodules.includes(naam)) stop(`${naam} ontbreekt in ${release}. Is het werkproces met Vulkan gebouwd?`);
}
if (!rekenmodules.some((n) => n.startsWith('ggml-cpu'))) {
  stop(`Geen ggml-cpu-*.dll in ${release}.`);
}

// 4. C++-runtime uit dezelfde Visual Studio
const redist = zoekRedistMap();
console.log(`C++-runtime uit ${redist}`);
const runtime = bestanden(redist, (n) => /^(msvcp140|vcruntime140).*\.dll$/.test(n));
const verplichtRuntime = ['msvcp140.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'];
for (const naam of verplichtRuntime) {
  if (!runtime.includes(naam)) stop(`${naam} ontbreekt in ${redist}.`);
}

const gekopieerd = [];
for (const naam of rekenmodules) {
  copyFileSync(join(release, naam), join(libMap, naam));
  gekopieerd.push(naam);
}
for (const naam of runtime) {
  // Bestandsnaam uit de map zelf, zodat de schrijfwijze van Microsoft behouden blijft.
  const echt = readdirSync(redist).find((n) => n.toLowerCase() === naam);
  copyFileSync(join(redist, echt), join(libMap, echt));
  gekopieerd.push(echt);
}
console.log(`engine-libs/ (${gekopieerd.length} bestanden):`);
for (const naam of gekopieerd.sort()) console.log(`  ${naam}`);
