#!/usr/bin/env node
// Versiescript: de versie staat in [workspace.package] van de root-Cargo.toml.
// Tauri leest die zelf; dit script houdt app/package.json, app/package-lock.json
// en Cargo.lock gelijk. Draait op alle platforms, zonder afhankelijkheden.
//
//   node scripts/versie.mjs           toon de versie en controleer dat alles gelijk is, dat een
//                                     uitgebrachte versie een changelog-kop heeft en dat er niet
//                                     op een al getagde versie doorgebouwd wordt
//   node scripts/versie.mjs 0.2.0     zet de versie overal
//   node scripts/versie.mjs --uitgave v0.2.0
//                                     alles van de aanroep zonder argument, plus: de versie is
//                                     geen ontwikkelversie en de tag is precies v<versie>
//                                     (de release-workflow roept dit aan)

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { controleerChangelog, controleerUitgave, controleerUitgebracht } from './lib/versiecontrole.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const cargoPad = join(root, 'Cargo.toml');
const pkgPad = join(root, 'app', 'package.json');
const lockPad = join(root, 'app', 'package-lock.json');

// Officiele SemVer-regex (semver.org), inclusief pre-release en build-metadata.
const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$/;

const stop = (melding) => {
  console.error(`Fout: ${melding}`);
  process.exit(1);
};

/** Vindt de versieregel binnen [workspace.package] en geeft regelindex en waarde. */
function zoekCargoVersie(tekst) {
  const regels = tekst.split('\n');
  let inSectie = false;
  for (let i = 0; i < regels.length; i++) {
    const kop = regels[i].match(/^\s*\[([^\]]+)\]\s*(#.*)?$/);
    if (kop) {
      inSectie = kop[1].trim() === 'workspace.package';
      continue;
    }
    if (inSectie) {
      const m = regels[i].match(/^(\s*version\s*=\s*)"([^"]*)"(.*)$/);
      if (m) return { regels, index: i, voor: m[1], waarde: m[2], na: m[3] };
    }
  }
  return stop('geen version gevonden in [workspace.package] van Cargo.toml');
}

const leesJson = (pad) => JSON.parse(readFileSync(pad, 'utf8'));
const schrijfJson = (pad, obj) => writeFileSync(pad, JSON.stringify(obj, null, 2) + '\n', 'utf8');

function huidig() {
  const cargo = zoekCargoVersie(readFileSync(cargoPad, 'utf8')).waarde;
  const pkg = leesJson(pkgPad).version;
  const lock = leesJson(lockPad);
  return { cargo, pkg, lockTop: lock.version, lockRoot: lock.packages?.['']?.version };
}

/** De commit achter een ref, of null als de ref niet bestaat of git ontbreekt. */
function gitCommit(ref) {
  try {
    return execFileSync('git', ['rev-parse', '--verify', '--quiet', `${ref}^{commit}`], {
      cwd: root,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  } catch {
    return null;
  }
}

const eerste = process.argv[2];
const uitgave = eerste === '--uitgave';
const nieuw = uitgave ? undefined : eerste;
const tag = process.argv[3];

if (uitgave && (tag === undefined || tag.trim() === '')) {
  stop('--uitgave vraagt een tag, bijvoorbeeld: node scripts/versie.mjs --uitgave v0.6.0');
}

if (nieuw === undefined) {
  const v = huidig();
  console.log(`Cargo.toml (workspace):   ${v.cargo}`);
  console.log(`app/package.json:         ${v.pkg}`);
  console.log(`app/package-lock.json:    ${v.lockTop} (pakket: ${v.lockRoot})`);
  const alle = new Set([v.cargo, v.pkg, v.lockTop, v.lockRoot]);
  if (alle.size !== 1) stop('de versies lopen uit elkaar. Draai: node scripts/versie.mjs <versie>');
  console.log('Alle versies zijn gelijk.');

  // Een versie zonder -dev is een uitgave: die hoort een changelog-kop te hebben en mag niet
  // opnieuw gebruikt worden als de tag al op een andere commit staat.
  const changelog = readFileSync(join(root, 'CHANGELOG.md'), 'utf8');
  const fout =
    controleerChangelog(v.cargo, changelog) ??
    controleerUitgebracht(v.cargo, gitCommit(`v${v.cargo}`), gitCommit('HEAD'));
  if (fout) stop(fout);
  if (uitgave) {
    const uitgaveFout = controleerUitgave(v.cargo, tag);
    if (uitgaveFout) stop(uitgaveFout);
    console.log(`Tag ${tag} hoort bij versie ${v.cargo}.`);
  }
  process.exit(0);
}

if (!SEMVER.test(nieuw)) stop(`"${nieuw}" is geen geldige SemVer-versie (bijvoorbeeld 0.2.0 of 1.0.0-beta.1)`);

// 1. Cargo.toml: alleen de versieregel in [workspace.package].
const cargoTekst = readFileSync(cargoPad, 'utf8');
const gevonden = zoekCargoVersie(cargoTekst);
gevonden.regels[gevonden.index] = `${gevonden.voor}"${nieuw}"${gevonden.na}`;
writeFileSync(cargoPad, gevonden.regels.join('\n'), 'utf8');

// 2. package.json en package-lock.json.
const pkg = leesJson(pkgPad);
pkg.version = nieuw;
schrijfJson(pkgPad, pkg);
const lock = leesJson(lockPad);
lock.version = nieuw;
if (lock.packages?.['']) lock.packages[''].version = nieuw;
schrijfJson(lockPad, lock);

// 3. Cargo.lock bijwerken voor de workspace-crates (geen andere updates).
try {
  execFileSync('cargo', ['update', '--workspace'], { cwd: root, stdio: 'inherit' });
} catch {
  stop('cargo update --workspace mislukt');
}

console.log(`\nVersie staat nu overal op ${nieuw}.`);
console.log('Nog te doen:');
console.log(`  1. CHANGELOG.md bijwerken ("Nog niet uitgebracht" omzetten naar ${nieuw}).`);
console.log(`  2. Committen en taggen: git tag v${nieuw}`);
