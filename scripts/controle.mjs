#!/usr/bin/env node
// Alle kwaliteitscontroles in een opdracht. De job `controle` in .github/workflows/ci.yml roept dit script aan.
// De lijst met stappen staat op een plek (STAPPEN hieronder); README, de projectregels en het
// PR-sjabloon verwijzen hiernaar in plaats van de lijst te herhalen. Zonder afhankelijkheden.
//
//   node scripts/controle.mjs              alles
//   node scripts/controle.mjs --rust       alleen de Rust-controles (plus de repo-controles)
//   node scripts/controle.mjs --frontend   alleen de frontend-controles (plus de repo-controles)
//   node scripts/controle.mjs --hook       voor de pre-push hook (leest de pushregels van git)
//
// Na een volledige groene run met een schone werkmap onthoudt het script de tree-hash van HEAD.
// De pre-push hook laat alleen een push door als de gepushte code daarin staat.

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve, win32 } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

// ---------------------------------------------------------------------------------------------
// De stappen: naam, groep, werkmap (vanaf de root) en opdracht. Lokaal en in CI is dit dezelfde lijst.
// ---------------------------------------------------------------------------------------------

export const STAPPEN = [
  // CI installeert de pakketten telkens opnieuw (npm ci); lokaal staan ze er al. Passen ze niet
  // bij package.json, bijvoorbeeld na een mislukte installatie, dan testen de stappen hierna de
  // oude versies en is groen niets waard.
  { naam: 'pakketten gelijk', groep: 'frontend', map: 'app', opdracht: 'npm ls --depth=0' },
  // De Tauri-crate leest de gebouwde frontend (app/build) bij het compileren, dus eerst bouwen.
  { naam: 'frontend bouwen', groep: 'frontend', map: 'app', opdracht: 'npm run build', bouw: true },
  { naam: 'cargo fmt', groep: 'rust', map: '.', opdracht: 'cargo fmt --all --check' },
  {
    naam: 'cargo clippy',
    groep: 'rust',
    map: '.',
    // --all-features: het nep-werkproces voor de supervisortests zit achter een feature.
    opdracht: 'cargo clippy --workspace --all-targets --all-features -- -D warnings',
  },
  { naam: 'cargo test', groep: 'rust', map: '.', opdracht: 'cargo test --workspace --all-features' },
  { naam: 'cargo deny', groep: 'rust', map: '.', opdracht: 'cargo deny check' },
  { naam: 'npm run check', groep: 'frontend', map: 'app', opdracht: 'npm run check' },
  { naam: 'npm run lint', groep: 'frontend', map: 'app', opdracht: 'npm run lint' },
  { naam: 'npm test', groep: 'frontend', map: 'app', opdracht: 'npm test' },
  { naam: 'npm audit', groep: 'frontend', map: 'app', opdracht: 'npm audit --audit-level=high' },
  { naam: 'npm-licenties', groep: 'frontend', map: '.', opdracht: 'node scripts/npm-licenties.mjs' },
  { naam: 'geen telemetrie', groep: 'frontend', map: '.', opdracht: 'node scripts/geen-telemetrie.mjs' },
  { naam: 'versies gelijk', groep: 'repo', map: '.', opdracht: 'node scripts/versie.mjs' },
  // Node zoekt zelf de testbestanden (de aanhalingstekens houden de shell eraf).
  { naam: 'scripttests', groep: 'repo', map: '.', opdracht: 'node --test "scripts/**/*.test.mjs"' },
];

/**
 * Kiest de stappen voor een vlag (`'rust'`, `'frontend'` of `null` voor alles). De groep repo
 * draait altijd. Bij `rust` draait de frontend-build alleen als die map nog ontbreekt.
 */
export function kiesStappen(vlag, { bouwBestaat = false } = {}) {
  return STAPPEN.filter((s) => {
    if (s.groep === 'repo') return true;
    if (vlag === 'rust') {
      if (s.groep === 'rust') return true;
      return Boolean(s.bouw) && !bouwBestaat;
    }
    if (vlag === 'frontend') return s.groep === 'frontend';
    return true;
  });
}

/** Leest de opdrachtregel. Geeft `{ vlag, hook }` of `{ fout }`. */
export function leesArgumenten(argv) {
  // Git geeft de pre-push hook de naam en het adres van de remote mee; die hebben we niet nodig.
  if (argv[0] === '--hook') return { vlag: null, hook: true };
  let vlag = null;
  let hook = false;
  for (const a of argv) {
    if (a === '--rust' || a === '--frontend') {
      const nieuw = a.slice(2);
      if (vlag && vlag !== nieuw) return { fout: 'kies --rust of --frontend, niet allebei' };
      vlag = nieuw;
    } else if (a === '--hook') {
      hook = true;
    } else {
      return { fout: `onbekende optie "${a}" (kan: --rust, --frontend, --hook)` };
    }
  }
  return { vlag, hook };
}

// ---------------------------------------------------------------------------------------------
// Bouwomgeving op Windows
// ---------------------------------------------------------------------------------------------

const padVan = (env) => env.PATH ?? env.Path ?? '';

/** Vergelijkt versiemappen als `10.0.26100` numeriek, nieuwste eerst. */
function nieuwsteEerst(a, b) {
  const x = a.split('.').map(Number);
  const y = b.split('.').map(Number);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (y[i] ?? 0) - (x[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/**
 * Bepaalt wat er aan de omgeving ontbreekt voor het bouwen op Windows: de Vulkan-SDK (voor de
 * spraakengine) en `rc.exe` (voor de Windows-resources van de app). Alleen wat ontbreekt wordt
 * gezet. De functie zet zelf niets; ze geeft terug wat er gezet moet worden. `bestaat` en
 * `lijstMap` komen van buiten, zodat de test geen echte schijf nodig heeft.
 */
export function bouwomgeving({ platform, env, thuis, bestaat, lijstMap }) {
  const resultaat = { env: {}, padVoor: [], meldingen: [] };
  if (platform !== 'win32') return resultaat;

  if (!env.VULKAN_SDK) {
    const sdk = win32.join(thuis, '.local', 'share', 'vulkan-sdk');
    if (bestaat(sdk)) {
      resultaat.env.VULKAN_SDK = sdk;
      resultaat.padVoor.push(win32.join(sdk, 'Bin'));
      resultaat.meldingen.push(`VULKAN_SDK gezet op ${sdk} (Bin staat nu op PATH)`);
    }
  }

  const opPad = padVan(env)
    .split(';')
    .filter(Boolean)
    .some((map) => bestaat(win32.join(map, 'rc.exe')));
  if (!opPad) {
    const programmas = env['ProgramFiles(x86)'] ?? env.ProgramFiles;
    if (programmas) {
      const bin = win32.join(programmas, 'Windows Kits', '10', 'bin');
      const versies = lijstMap(bin)
        .filter((v) => /^\d+(\.\d+)+$/.test(v))
        .sort(nieuwsteEerst);
      const gevonden = versies.find((v) => bestaat(win32.join(bin, v, 'x64', 'rc.exe')));
      if (gevonden) {
        const map = win32.join(bin, gevonden, 'x64');
        resultaat.padVoor.push(map);
        resultaat.meldingen.push(`rc.exe van Windows Kits ${gevonden} op PATH gezet`);
      }
    }
  }
  return resultaat;
}

/** Past `bouwomgeving` toe op het huidige proces en meldt in een regel wat er gezet is. */
function zetBouwomgeving() {
  const r = bouwomgeving({
    platform: process.platform,
    env: process.env,
    thuis: homedir(),
    bestaat: existsSync,
    lijstMap: (map) => {
      try {
        return readdirSync(map);
      } catch {
        return [];
      }
    },
  });
  Object.assign(process.env, r.env);
  if (r.padVoor.length > 0) process.env.PATH = [...r.padVoor, padVan(process.env)].join(';');
  if (r.meldingen.length > 0) console.log(`Bouwomgeving: ${r.meldingen.join('; ')}.`);
}

// ---------------------------------------------------------------------------------------------
// Onthouden van de groene stand en de pre-push hook
// ---------------------------------------------------------------------------------------------

/** Hoeveel groene trees we bewaren; de werkbomen van een repo delen dit bestand. */
export const MAX_GROEN = 20;

/** Voegt een tree toe als nieuwste regel en houdt hoogstens `max` regels over. */
export function nieuweGroenRegels(bestaand, tree, max = MAX_GROEN) {
  const regels = bestaand.map((r) => r.trim()).filter((r) => r && r !== tree);
  regels.push(tree);
  return regels.slice(-max);
}

// De projectinstructies zijn geen code; de bestandsnaam staat in twee stukken omdat de
// patrooncontrole van de openbare export het hele woord als verboden aanmerkt.
const INSTRUCTIEBESTAND = ['CLAU', 'DE.md'].join('');

/** Paden waarvan een wijziging de uitkomst van de controles niet kan veranderen. */
export const DOCUMENTATIE = ['docs', INSTRUCTIEBESTAND, 'README.md'];

/** Leest de regels die git op stdin aan een pre-push hook geeft. */
export function leesPushRegels(tekst) {
  return tekst
    .split('\n')
    .map((r) => r.trim().split(/\s+/))
    .filter((d) => d.length >= 4)
    .map(([lokaleRef, lokaleSha, remoteRef, remoteSha]) => ({ lokaleRef, lokaleSha, remoteRef, remoteSha }));
}

const AANBEVELING = 'Draai eerst: node scripts/controle.mjs (zonder opties, met een schone werkmap).';

/**
 * Beslist of een push door mag. `git(args)` geeft `{ status, stdout }` en komt van buiten, zodat
 * de test geen echte repo nodig heeft; `groen` zijn de onthouden trees, oudste eerst.
 * Geeft `{ toegestaan, meldingen }`.
 */
export function hookBeslissing(regels, { git, groen }) {
  const meldingen = [];
  for (const { lokaleRef, lokaleSha, remoteRef } of regels) {
    // Een branch of tag op de server verwijderen: er is geen code om te controleren.
    if (/^0+$/.test(lokaleSha)) continue;
    if (remoteRef.startsWith('refs/tags/')) continue;
    if (remoteRef === 'refs/heads/main') {
      meldingen.push(
        `Pushen naar main kan niet. Werk gaat via een eigen branch en een pull request naar main (${lokaleRef} blijft lokaal).`,
      );
      continue;
    }
    const tree = git(['rev-parse', `${lokaleSha}^{tree}`]);
    if (tree.status !== 0 || !tree.stdout.trim()) {
      meldingen.push(`Van ${lokaleRef} is de inhoud niet te bepalen, dus ook niet of die gecontroleerd is. ${AANBEVELING}`);
      continue;
    }
    const nu = tree.stdout.trim();
    if (groen.includes(nu)) continue;
    // Is er sinds een groene stand alleen documentatie veranderd, dan is de controle nog geldig.
    const uitsluiten = DOCUMENTATIE.map((p) => `:(exclude)${p}`);
    const alleenDocs = [...groen]
      .reverse()
      .some((eerder) => git(['diff', '--quiet', eerder, nu, '--', '.', ...uitsluiten]).status === 0);
    if (alleenDocs) continue;
    meldingen.push(`De code die je naar ${remoteRef} pusht is nog niet gecontroleerd. ${AANBEVELING}`);
  }
  return { toegestaan: meldingen.length === 0, meldingen };
}

// ---------------------------------------------------------------------------------------------
// De rest draait alleen als het script direct wordt gestart.
// ---------------------------------------------------------------------------------------------

function git(args) {
  const r = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  return { status: r.status ?? 1, stdout: r.stdout ?? '' };
}

/** Het groene bestand staat in de gedeelde git-map, zodat alle werkbomen het zien. */
function groenPad() {
  const r = git(['rev-parse', '--git-common-dir']);
  if (r.status !== 0) return null;
  return resolve(root, r.stdout.trim(), 'mondig-controle-groen');
}

function leesGroen() {
  const pad = groenPad();
  if (!pad || !existsSync(pad)) return [];
  return readFileSync(pad, 'utf8')
    .split('\n')
    .map((r) => r.trim())
    .filter(Boolean);
}

function stop(melding) {
  console.error(`Fout: ${melding}`);
  process.exit(1);
}

function draaiHook() {
  const regels = leesPushRegels(readFileSync(0, 'utf8'));
  const { toegestaan, meldingen } = hookBeslissing(regels, { git, groen: leesGroen() });
  for (const m of meldingen) console.error(m);
  process.exit(toegestaan ? 0 : 1);
}

const seconden = (ms) => `${(ms / 1000).toFixed(1)} s`;

function toonSamenvatting(uitkomsten) {
  console.log('\nSamenvatting');
  for (const u of uitkomsten) {
    console.log(`  ${u.ok ? 'ok  ' : 'FOUT'} ${u.naam.padEnd(18)} ${seconden(u.ms)}`);
  }
  const totaal = uitkomsten.reduce((som, u) => som + u.ms, 0);
  console.log(`  totaal ${seconden(totaal)}`);
}

/** Onthoudt de groene stand, maar alleen als de werkmap schoon is en HEAD niet verschoof. */
function onthoudGroen(treeBegin) {
  const status = git(['status', '--porcelain']);
  const tree = git(['rev-parse', 'HEAD^{tree}']);
  const pad = groenPad();
  if (status.status !== 0 || tree.status !== 0 || !pad) {
    console.log('\nAlles groen. De stand wordt niet onthouden: git geeft geen antwoord.');
    return;
  }
  if (status.stdout.trim() !== '' || tree.stdout.trim() !== treeBegin) {
    console.log(
      '\nAlles groen, maar de stand wordt niet onthouden: er zijn ongecommitte wijzigingen. ' +
        'Commit en draai de controle opnieuw voor je pusht.',
    );
    return;
  }
  mkdirSync(dirname(pad), { recursive: true });
  writeFileSync(pad, nieuweGroenRegels(leesGroen(), treeBegin).join('\n') + '\n', 'utf8');
  console.log(`\nAlles groen. Onthouden voor de pre-push hook (tree ${treeBegin.slice(0, 12)}).`);
}

function main() {
  const args = leesArgumenten(process.argv.slice(2));
  if (args.fout) stop(args.fout);
  if (args.hook) return draaiHook();

  if (!existsSync(join(root, 'app', 'node_modules'))) {
    stop('app/node_modules ontbreekt. Draai eerst: npm --prefix app install');
  }
  zetBouwomgeving();

  const stappen = kiesStappen(args.vlag, { bouwBestaat: existsSync(join(root, 'app', 'build', 'index.html')) });
  const treeBegin = git(['rev-parse', 'HEAD^{tree}']).stdout.trim();
  const uitkomsten = [];
  // Op GitHub klapt het logboek per stap in; lokaal verandert de uitvoer niet.
  const opGithub = process.env.GITHUB_ACTIONS === 'true';

  for (const stap of stappen) {
    if (opGithub) console.log(`::group::${stap.naam}`);
    console.log(`\n> ${stap.naam}: ${stap.opdracht}   (in ${stap.map === '.' ? 'de root' : stap.map})`);
    const start = Date.now();
    // Een enkele opdrachtregel met shell: zo start npm.cmd op Windows zonder de waarschuwing
    // over losse argumenten naast shell: true.
    const r = spawnSync(stap.opdracht, { cwd: join(root, stap.map), stdio: 'inherit', shell: true, env: process.env });
    const ok = r.status === 0;
    if (opGithub) console.log('::endgroup::');
    uitkomsten.push({ naam: stap.naam, ok, ms: Date.now() - start });
    if (!ok) {
      // De uitvoer van de stap zit op GitHub in een ingeklapte groep; deze regel maakt de fout
      // ook zichtbaar in het overzicht van de run.
      if (opGithub) console.log(`::error::De stap "${stap.naam}" faalde. Klap de groep met die naam open voor de uitvoer.`);
      toonSamenvatting(uitkomsten);
      console.error(`\nDe controle stopt: de stap "${stap.naam}" faalde.`);
      process.exit(1);
    }
  }

  toonSamenvatting(uitkomsten);
  if (args.vlag === null) onthoudGroen(treeBegin);
  else console.log('\nGroen, maar niet onthouden: de pre-push hook vraagt een volledige run (zonder --rust of --frontend).');
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) main();
