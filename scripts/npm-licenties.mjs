#!/usr/bin/env node
// Licentiecheck voor de npm-pakketten van de app (cargo-deny ziet de frontend niet).
// Leest app/package-lock.json, geen afhankelijkheden nodig.
//
//   node scripts/npm-licenties.mjs                    controleer alleen
//   node scripts/npm-licenties.mjs --notices          controle, daarna lijst naar stdout
//   node scripts/npm-licenties.mjs --notices=pad.md   controle, daarna lijst naar bestand
//   node scripts/npm-licenties.mjs --json=pad.json    controle, daarna lijst als JSON (voor de app)
//
// Alle pakketten uit de lockfile moeten een toegestane licentie hebben, ook de
// dev-pakketten: de bundler neemt runtimecode daarvan (svelte, @sveltejs/kit)
// op in de app. De toegestane lijst komt uit deny.toml; about.toml
// moet dezelfde lijst hebben, anders stopt het script.

import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { zoekNotice } from './lib/notice.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const lockPad = join(root, 'app', 'package-lock.json');

/**
 * Leest een lijst met aanhalingstekens (`sleutel = [ ... ]`) uit een toml-bestand.
 * Met `sectie` zoekt hij alleen binnen die `[sectie]`. Simpele parse, geen toml-pakket.
 */
function leesLijst(bestand, sectie, sleutel) {
  const pad = join(root, bestand);
  if (!existsSync(pad)) throw new Error(`${bestand} ontbreekt`);
  // Commentaar eraf, zodat quotes in commentaar niet meetellen.
  let tekst = readFileSync(pad, 'utf8')
    .split(/\r?\n/)
    .map((r) => r.replace(/#.*$/, ''))
    .join('\n');
  if (sectie) {
    const start = tekst.search(new RegExp(`^\\[${sectie}\\]\\s*$`, 'm'));
    if (start < 0) throw new Error(`${bestand}: sectie [${sectie}] niet gevonden`);
    const rest = tekst.slice(start).replace(/^\[.*\]\s*/, '');
    const volgende = rest.search(/^\[/m);
    tekst = volgende < 0 ? rest : rest.slice(0, volgende);
  }
  const m = tekst.match(new RegExp(`^\\s*${sleutel}\\s*=\\s*\\[([\\s\\S]*?)\\]`, 'm'));
  if (!m) throw new Error(`${bestand}: lijst "${sleutel}" niet gevonden`);
  return [...m[1].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
}

let TOEGESTAAN;
try {
  const deny = leesLijst('deny.toml', 'licenses', 'allow');
  const about = leesLijst('about.toml', null, 'accepted');
  TOEGESTAAN = new Set(deny);
  const alleenDeny = deny.filter((l) => !about.includes(l));
  const alleenAbout = about.filter((l) => !deny.includes(l));
  if (alleenDeny.length > 0 || alleenAbout.length > 0) {
    console.error('Licentielijsten lopen uiteen: deny.toml en about.toml moeten gelijk zijn.');
    if (alleenDeny.length > 0) console.error(`  Alleen in deny.toml: ${alleenDeny.join(', ')}`);
    if (alleenAbout.length > 0) console.error(`  Alleen in about.toml: ${alleenAbout.join(', ')}`);
    process.exit(1);
  }
} catch (e) {
  console.error(`Licentielijst niet te lezen: ${e.message}`);
  process.exit(1);
}

/** Zet een SPDX-expressie om in tokens: haakjes en woorden. */
function tokens(expr) {
  return expr.match(/\(|\)|[^\s()]+/g) ?? [];
}

/**
 * Beoordeelt een SPDX-expressie met OR, AND, WITH en haakjes.
 * `ok(licentie)` bepaalt of één licentie telt; OR is geslaagd als één optie
 * slaagt, AND als alle opties slagen.
 */
function beoordeel(expr, ok) {
  const t = tokens(expr);
  let i = 0;
  const atoom = () => {
    if (t[i] === '(') {
      i++;
      const r = of();
      if (t[i] !== ')') throw new Error('Onbalans in haakjes');
      i++;
      return r;
    }
    if (i >= t.length) throw new Error('Onverwacht einde');
    let naam = t[i++];
    if (t[i] === 'WITH') {
      naam += ` WITH ${t[i + 1]}`;
      i += 2;
    }
    return ok(naam);
  };
  const en = () => {
    let r = atoom();
    while (t[i] === 'AND') {
      i++;
      const v = atoom();
      r = r && v;
    }
    return r;
  };
  const of = () => {
    let r = en();
    while (t[i] === 'OR') {
      i++;
      const v = en();
      r = r || v;
    }
    return r;
  };
  const uitkomst = of();
  if (i < t.length) throw new Error(`Onverwacht token "${t[i]}"`);
  return uitkomst;
}

// Pakketten zonder licentieveld in package-lock.json, waarvan het LICENSE-bestand met de hand
// is nagekeken. Alleen exacte pakketnamen; elke regel met reden en datum.
const HANDMATIG = {
  // MIT volgens het LICENSE-bestand; afhankelijkheid van bits-ui (ADR 0010). Nagekeken 01-11-2026.
  'svelte-toolbelt': 'MIT'
};

function licentieTekst(pkg) {
  if (typeof pkg.license === 'string') return pkg.license;
  // Oud formaat: { type: "MIT" } of een lijst daarvan.
  if (pkg.license && typeof pkg.license.type === 'string') return pkg.license.type;
  if (Array.isArray(pkg.licenses)) return pkg.licenses.map((l) => l.type).join(' OR ');
  return null;
}

function naamUitPad(pad) {
  const delen = pad.split('node_modules/');
  return delen[delen.length - 1];
}

const lock = JSON.parse(readFileSync(lockPad, 'utf8'));
const pakketten = [];
for (const [pad, pkg] of Object.entries(lock.packages ?? {})) {
  if (pad === '' || pkg.link) continue;
  pakketten.push({
    naam: pkg.name ?? naamUitPad(pad),
    versie: pkg.version ?? '?',
    licentie: licentieTekst(pkg) ?? HANDMATIG[pkg.name ?? naamUitPad(pad)] ?? null,
    pad
  });
}

const fouten = [];
for (const p of pakketten) {
  const label = `${p.naam}@${p.versie}`;
  if (!p.licentie) {
    fouten.push(`${label}: geen licentie opgegeven in package-lock.json`);
    continue;
  }
  try {
    if (!beoordeel(p.licentie, (l) => TOEGESTAAN.has(l))) {
      fouten.push(`${label}: licentie "${p.licentie}" staat niet op de toegestane lijst`);
    }
  } catch (e) {
    fouten.push(`${label}: licentie "${p.licentie}" is niet te lezen (${e.message})`);
  }
}

console.log(`npm-licenties: ${pakketten.length} pakketten gecontroleerd (productie en dev).`);

/** Zoekt het licentiebestand van een pakket in app/node_modules en geeft de tekst terug. */
function licentieBestand(pad) {
  const map = join(root, 'app', pad);
  if (!existsSync(map)) return null;
  const namen = readdirSync(map);
  for (const basis of ['license', 'licence', 'copying']) {
    const naam = namen.find((n) => new RegExp(`^${basis}([-.].*)?$`, 'i').test(n));
    if (naam) {
      try {
        return readFileSync(join(map, naam), 'utf8').replace(/\r\n/g, '\n').trimEnd();
      } catch {
        // Map met die naam of onleesbaar: volgende kandidaat.
      }
    }
  }
  return null;
}

/** Zoekt een NOTICE-bestand van een pakket (Apache-2.0, artikel 4(d)). */
const noticeBestand = (pad) => zoekNotice(join(root, 'app', pad));

const jsonArg = process.argv.find((a) => a.startsWith('--json='));
if (jsonArg) {
  const lijst = pakketten.map((p) => ({
    naam: p.naam,
    versie: p.versie,
    licentie: p.licentie ?? 'onbekend',
    tekst: licentieBestand(p.pad),
    notice: noticeBestand(p.pad)
  }));
  writeFileSync(jsonArg.slice('--json='.length), JSON.stringify(lijst), 'utf8');
  console.log(
    `npm-licenties: ${lijst.length} pakketten naar JSON, met NOTICE: ${lijst.filter((x) => x.notice).length}.`
  );
}

const noticesArg = process.argv.find((a) => a === '--notices' || a.startsWith('--notices='));
if (noticesArg) {
  // Alle pakketten, ook dev: de bundler neemt runtimecode van dev-pakketten op.
  const regels = [
    '# Npm-afhankelijkheden',
    '',
    'Alle pakketten uit app/package-lock.json, inclusief bouwgereedschap.',
    ''
  ];
  let gevonden = 0;
  let nietGevonden = 0;
  for (const p of [...pakketten].sort((a, b) => a.naam.localeCompare(b.naam))) {
    regels.push(`## ${p.naam} ${p.versie}`, '', `Licentie: ${p.licentie ?? 'onbekend'}`, '');
    const tekst = licentieBestand(p.pad);
    if (tekst) {
      gevonden++;
      regels.push('```text', tekst, '```', '');
    } else {
      nietGevonden++;
      regels.push(`Licentiebestand niet gevonden (licentie: ${p.licentie ?? 'onbekend'}).`, '');
    }
    const notice = noticeBestand(p.pad);
    if (notice) regels.push('NOTICE:', '', '```text', notice, '```', '');
  }
  console.log(`npm-licenties: licentieteksten gevonden: ${gevonden}, niet gevonden: ${nietGevonden}.`);
  const tekst = regels.join('\n') + '\n';
  if (noticesArg.includes('=')) {
    writeFileSync(noticesArg.split('=')[1], tekst, 'utf8');
  } else {
    process.stdout.write('\n' + tekst);
  }
}

if (fouten.length > 0) {
  console.error('\nLicentiecheck mislukt:');
  for (const f of fouten) console.error(`  - ${f}`);
  console.error(
    '\nPas de toegestane lijst alleen aan na overleg (de bron is deny.toml; about.toml houdt dezelfde lijst).'
  );
  process.exit(1);
}
console.log('npm-licenties: alles op de toegestane lijst.');
