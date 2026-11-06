#!/usr/bin/env node
// Maakt de vermeldingen van derden voor in de app (ADR 0016): de Rust-onderdelen uit
// cargo-about plus de npm-pakketten uit scripts/npm-licenties.mjs, met licentietekst en
// eventuele NOTICE-inhoud (Apache-2.0, artikel 4(d)). Lokaal en in CI hetzelfde.
//
//   node scripts/vermeldingen.mjs          schrijft app/src/lib/gegenereerd/vermeldingen.json
//
// De app laadt dat bestand bij het scherm Over Mondig. Het gaat niet in git. De build
// (`npm run bundel`, `tauri build`) draait dit script eerst; ontbreekt cargo-about, dan
// stopt het met een duidelijke melding in plaats van een lege lijst te maken.
//
// Vorm van het bestand:
//   { gegenereerd, teksten: [string], onderdelen: [{ naam, versie, bron, licentie, teksten: [index], notice: index | null, url }] }
// Teksten staan één keer; onderdelen verwijzen ernaar met een index, zodat dubbele
// licentieteksten niet dubbel in de app komen.

import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { zoekNotice } from './lib/notice.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const uitMap = join(root, 'app', 'src', 'lib', 'gegenereerd');
const uitPad = join(uitMap, 'vermeldingen.json');

const stop = (melding) => {
  console.error(`Fout: ${melding}`);
  process.exit(1);
};

/** De datum van het maken (jjjj-mm-dd, UTC); SOURCE_DATE_EPOCH heeft voorrang (herhaalbare build). */
function bouwdatum() {
  const epoch = Number(process.env.SOURCE_DATE_EPOCH);
  const moment = Number.isFinite(epoch) && process.env.SOURCE_DATE_EPOCH ? new Date(epoch * 1000) : new Date();
  return moment.toISOString().slice(0, 10);
}

const tijdelijk = mkdtempSync(join(tmpdir(), 'mondig-vermeldingen-'));
const opruimen = () => rmSync(tijdelijk, { recursive: true, force: true });

// Eén lijst met unieke teksten; geeft de index terug.
const teksten = [];
const index = new Map();
function tekstIndex(tekst) {
  const schoon = tekst.replace(/\r\n/g, '\n').trimEnd();
  if (!index.has(schoon)) {
    index.set(schoon, teksten.length);
    teksten.push(schoon);
  }
  return index.get(schoon);
}

// 1. Rust: cargo-about als JSON, voor de app-crate (en dus alleen wat de app gebruikt).
const aboutUit = join(tijdelijk, 'about.json');
const about = spawnSync(
  'cargo',
  [
    'about',
    'generate',
    '--format',
    'json',
    '-c',
    join(root, 'about.toml'),
    '--manifest-path',
    join(root, 'app', 'src-tauri', 'Cargo.toml'),
    '-o',
    aboutUit
  ],
  { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }
);
if (about.error || about.status !== 0 || !existsSync(aboutUit)) {
  opruimen();
  stop(
    'cargo-about werkt niet, dus de vermeldingen van de Rust-onderdelen kunnen niet worden gemaakt.\n' +
      '  Installeer het met: cargo install cargo-about --version 0.9.3 --locked\n' +
      `  Melding: ${(about.error?.message ?? about.stderr ?? '').toString().trim().split('\n').slice(-3).join(' | ')}`
  );
}
const gegevens = JSON.parse(readFileSync(aboutUit, 'utf8'));

// Welke licentieteksten horen bij welke crate (een crate met "A OR B" staat onder beide).
const teksten_per_crate = new Map();
for (const licentie of gegevens.licenses) {
  const i = tekstIndex(licentie.text);
  for (const gebruik of licentie.used_by) {
    const sleutel = `${gebruik.crate.name}@${gebruik.crate.version}`;
    const lijst = teksten_per_crate.get(sleutel) ?? [];
    if (!lijst.includes(i)) lijst.push(i);
    teksten_per_crate.set(sleutel, lijst);
  }
}

const onderdelen = [];
for (const { package: pak, license } of gegevens.crates) {
  const sleutel = `${pak.name}@${pak.version}`;
  const notice = zoekNotice(dirname(pak.manifest_path));
  onderdelen.push({
    naam: pak.name,
    versie: pak.version,
    bron: 'rust',
    licentie: license ?? pak.license ?? 'onbekend',
    teksten: teksten_per_crate.get(sleutel) ?? [],
    notice: notice === null ? null : tekstIndex(notice),
    url: pak.repository ?? null
  });
}

// 2. npm: dezelfde lijst als de licentiecheck, met licentietekst en NOTICE.
const npmUit = join(tijdelijk, 'npm.json');
try {
  execFileSync(process.execPath, [join(root, 'scripts', 'npm-licenties.mjs'), `--json=${npmUit}`], {
    cwd: root,
    stdio: ['ignore', 'inherit', 'inherit']
  });
} catch {
  opruimen();
  stop('scripts/npm-licenties.mjs faalde (zie hierboven). Zijn de npm-pakketten geinstalleerd (npm ci)?');
}
for (const p of JSON.parse(readFileSync(npmUit, 'utf8'))) {
  onderdelen.push({
    naam: p.naam,
    versie: p.versie,
    bron: 'npm',
    licentie: p.licentie,
    teksten: p.tekst ? [tekstIndex(p.tekst)] : [],
    notice: p.notice ? tekstIndex(p.notice) : null,
    url: null
  });
}

// 3. Wat niet uit een pakketbeheerder komt.
// De lettertypen in de PDF's (ADR 0017): Sora en Geist als TTF in mondig-core, met hun OFL-tekst
// (copyright, licentievermelding en licentietekst moeten mee, OFL-1.1).
for (const [naam, bestand] of [
  ['Sora (lettertype in PDF)', 'OFL-Sora.txt'],
  ['Geist (lettertype in PDF)', 'OFL-Geist.txt']
]) {
  const pad = join(root, 'crates', 'mondig-core', 'lettertypen', bestand);
  if (!existsSync(pad)) {
    opruimen();
    stop(`${pad} ontbreekt; de licentie van het lettertype moet mee in de app.`);
  }
  onderdelen.push({
    naam,
    versie: '',
    bron: 'overig',
    licentie: 'OFL-1.1',
    teksten: [tekstIndex(readFileSync(pad, 'utf8'))],
    notice: null,
    url: naam.startsWith('Sora') ? 'https://github.com/sora-xor/sora-font' : 'https://github.com/vercel/geist-font'
  });
}

// De C++-runtime zit alleen in de Windows-installer.
if (process.platform === 'win32') {
  onderdelen.push({
  naam: 'Microsoft Visual C++-runtime',
  versie: '',
  bron: 'overig',
  licentie: 'Microsoft Distributable Code',
  teksten: [
    tekstIndex(
      'De Windows-installer van Mondig bevat bestanden van de Microsoft Visual C++-runtime ' +
        '(msvcp140*.dll en vcruntime140*.dll). Die vallen onder de licentievoorwaarden van Microsoft ' +
        'voor "Distributable Code" van Visual Studio en worden zonder wijziging meegeleverd. ' +
        'Zie https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files.'
    )
  ],
  notice: null,
  url: 'https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files'
});
}

onderdelen.sort((a, b) => a.naam.localeCompare(b.naam, 'en') || a.versie.localeCompare(b.versie));

mkdirSync(uitMap, { recursive: true });
writeFileSync(
  uitPad,
  JSON.stringify({ gegenereerd: bouwdatum(), teksten, onderdelen }),
  'utf8'
);
opruimen();

const metNotice = onderdelen.filter((o) => o.notice !== null).length;
const zonderTekst = onderdelen.filter((o) => o.teksten.length === 0).length;
console.log(
  `vermeldingen: ${onderdelen.length} onderdelen (${onderdelen.filter((o) => o.bron === 'rust').length} Rust, ` +
    `${onderdelen.filter((o) => o.bron === 'npm').length} npm), ${teksten.length} unieke teksten, ` +
    `${metNotice} met NOTICE, ${zonderTekst} zonder licentietekst.`
);
console.log(`vermeldingen: geschreven naar ${uitPad}`);
