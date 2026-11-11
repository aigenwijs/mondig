// Bewaakt de afspraken voor de GitHub-workflows, regel voor regel en zonder YAML-bibliotheek.
// Draaien: node --test scripts/workflows.test.mjs
// Elke controle geeft een lijst overtredingen terug, zodat de test ze ook met verzonnen tekst kan proberen.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const map = join(dirname(fileURLToPath(import.meta.url)), '..', '.github', 'workflows');
const bestanden = readdirSync(map).filter((n) => n.endsWith('.yml'));
const lees = (naam) => readFileSync(join(map, naam), 'utf8');
const regelsVan = (tekst) => tekst.split(/\r?\n/);
const inspringing = (regel) => regel.length - regel.trimStart().length;

/** Elke `uses:` staat op een SHA van 40 hex-tekens, met een versiecommentaar (`# v7.0.1`) erachter. */
export function usesZonderSha(tekst) {
  const fout = [];
  regelsVan(tekst).forEach((regel, i) => {
    const m = regel.match(/^\s*(?:-\s+)?uses:\s*(\S+)(.*)$/);
    if (!m) return;
    const goed = /@[0-9a-f]{40}$/.test(m[1]) && /^\s+#\s*v\d/.test(m[2]);
    if (!goed) fout.push(`regel ${i + 1}: ${regel.trim()}`);
  });
  return fout;
}

/** Bovenaan (kolom 0) staat een `permissions:`-blok, en `write` komt alleen binnen een job voor. */
export function rechtenOvertredingen(tekst) {
  const fout = [];
  const regels = regelsVan(tekst);
  if (!regels.some((r) => /^permissions:/.test(r))) fout.push('geen permissions-blok op het hoogste niveau');
  regels.forEach((regel, i) => {
    // Ook met commentaar erachter. Een rechtenblok binnen een job springt minstens 6 spaties in.
    if (/^\s*[\w-]+:\s*write\s*(#.*)?$/.test(regel) && inspringing(regel) < 6) {
      fout.push(`regel ${i + 1}: write buiten een job (${regel.trim()})`);
    }
    // De korte vormen: alle rechten tegelijk, of de rechten op één regel bovenaan.
    if (/^\s*permissions:\s*write-all\b/.test(regel)) fout.push(`regel ${i + 1}: write-all`);
    if (/^permissions:\s*\{.*\bwrite\b/.test(regel)) fout.push(`regel ${i + 1}: write buiten een job (${regel.trim()})`);
  });
  return fout;
}

/**
 * Regels die bij een `run:` horen (de run-regel zelf en, bij een blok met `|` of `>`, de regels
 * eronder) en een uitdrukking `${{ ... }}` bevatten. GitHub vult die in vóór de shell start, dus
 * een invoerwaarde kan zo als opdracht uitgevoerd worden. Waarden horen via `env` in de opdracht.
 */
export function injectieInRun(tekst) {
  const fout = [];
  const regels = regelsVan(tekst);
  let blokInspringing = null;
  regels.forEach((regel, i) => {
    if (blokInspringing !== null) {
      if (regel.trim() === '' || inspringing(regel) > blokInspringing) {
        if (regel.includes('${{')) fout.push(`regel ${i + 1}: ${regel.trim()}`);
        return;
      }
      blokInspringing = null;
    }
    const m = regel.match(/^(\s*)(?:-\s+)?run:\s*(.*)$/);
    if (!m) return;
    if (/^[|>][+-]?\s*$/.test(m[2])) {
      blokInspringing = inspringing(regel);
    } else if (m[2].includes('${{')) {
      fout.push(`regel ${i + 1}: ${regel.trim()}`);
    }
  });
  return fout;
}

// ---- de echte bestanden --------------------------------------------------------------------

test('er staan workflows in .github/workflows', () => {
  assert.ok(bestanden.includes('ci.yml') && bestanden.includes('release.yml') && bestanden.includes('bench.yml'));
});

test('elke actie staat op een commit met een versiecommentaar', () => {
  for (const naam of bestanden) assert.deepEqual(usesZonderSha(lees(naam)), [], naam);
});

test('elke workflow heeft rechten op het hoogste niveau en schrijfrechten alleen per job', () => {
  for (const naam of bestanden) assert.deepEqual(rechtenOvertredingen(lees(naam)), [], naam);
});

test('geen uitdrukking van GitHub rechtstreeks in een run-opdracht', () => {
  for (const naam of bestanden) assert.deepEqual(injectieInRun(lees(naam)), [], naam);
});

test('ci.yml roept het controlescript aan en herhaalt de controles niet', () => {
  const ci = lees('ci.yml');
  assert.match(ci, /run: node scripts\/controle\.mjs/);
  assert.doesNotMatch(ci, /cargo clippy/);
  assert.doesNotMatch(ci, /cargo fmt/);
});

test('release.yml controleert de uitgave en bouwt overal de gekeurde commit van de tag', () => {
  const release = lees('release.yml');
  assert.match(release, /node scripts\/versie\.mjs --uitgave/);
  // De eerste job haalt de tag op en legt de commit vast; elke job daarna bouwt die commit.
  const checkouts = release.match(/uses: actions\/checkout@/g) ?? [];
  const vanTag = release.match(/^\s+ref: refs\/tags\/\$\{\{ inputs\.tag \}\}\s*$/gm) ?? [];
  const vanCommit = release.match(/^\s+ref: \$\{\{ needs\.controle\.outputs\.sha \}\}\s*$/gm) ?? [];
  assert.equal(vanTag.length, 1);
  assert.ok(vanCommit.length >= 2);
  assert.equal(vanTag.length + vanCommit.length, checkouts.length);
  // De controle of de tag op main staat komt vóór het eerste script uit de tag.
  assert.ok(release.indexOf('git merge-base --is-ancestor') < release.indexOf('node scripts/versie.mjs --uitgave'));
  // Het concept wordt één keer aangemaakt; de builds krijgen het nummer ervan.
  assert.match(release, /gh release create "\$TAG" --draft --prerelease --verify-tag/);
  assert.match(release, /releaseId: \$\{\{ needs\.concept\.outputs\.release_id \}\}/);
});

// ---- de controles zelf, met verzonnen tekst ------------------------------------------------

const SHA = 'a'.repeat(40);

test('uses met een los versielabel of een SHA zonder commentaar wordt afgekeurd', () => {
  assert.equal(usesZonderSha(`      - uses: actions/checkout@v7\n`).length, 1);
  assert.equal(usesZonderSha(`      - uses: actions/checkout@${SHA}\n`).length, 1);
  assert.equal(usesZonderSha(`      - uses: actions/checkout@${SHA} # nieuwste\n`).length, 1);
  assert.equal(usesZonderSha(`        uses: actions/checkout@${SHA.slice(1)} # v7.0.1\n`).length, 1);
  assert.deepEqual(usesZonderSha(`      - uses: actions/checkout@${SHA} # v7.0.1\n`), []);
});

test('write op het hoogste niveau wordt afgekeurd, per job mag het', () => {
  assert.equal(rechtenOvertredingen('permissions:\n  contents: write\n').length, 1);
  assert.equal(rechtenOvertredingen('jobs:\n  a:\n    permissions:\n      contents: write\n').length, 1);
  assert.equal(rechtenOvertredingen('permissions: write-all\n').length, 1);
  assert.equal(rechtenOvertredingen('permissions:\n  contents: write # voor de release\n').length, 1);
  assert.equal(rechtenOvertredingen('permissions: { contents: write }\n').length, 1);
  assert.deepEqual(
    rechtenOvertredingen('permissions:\n  contents: read\njobs:\n  a:\n    permissions:\n      contents: write\n'),
    [],
  );
});

test('een workflow zonder permissions-blok bovenaan wordt afgekeurd', () => {
  assert.equal(rechtenOvertredingen('name: X\njobs:\n  a:\n    permissions:\n      contents: read\n').length, 1);
});

test('inputs in een run-opdracht worden afgekeurd, via env mag het', () => {
  assert.equal(injectieInRun('      - run: echo ${{ inputs.tag }}\n').length, 1);
  assert.equal(injectieInRun('        run: |\n          echo ${{ github.ref }}\n').length, 1);
  assert.equal(injectieInRun('        run: gh release upload ${{ needs.controle.outputs.sha }}\n').length, 1);
  assert.deepEqual(
    injectieInRun('        env:\n          TAG: ${{ inputs.tag }}\n        run: |\n          echo "$TAG"\n'),
    [],
  );
});
