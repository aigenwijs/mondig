// Tests van scripts/controle.mjs. Draaien: node --test scripts/controle.test.mjs
import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  DOCUMENTATIE,
  STAPPEN,
  bouwomgeving,
  hookBeslissing,
  kiesStappen,
  leesArgumenten,
  leesPushRegels,
  nieuweGroenRegels,
} from './controle.mjs';

const namen = (stappen) => stappen.map((s) => s.naam);
const NUL = '0'.repeat(40);

// ---- stappen per vlag ----------------------------------------------------------------------

test('zonder vlag draaien alle stappen, in de volgorde van de tabel', () => {
  assert.deepEqual(namen(kiesStappen(null)), namen(STAPPEN));
  // Eerst nagaan dat de geïnstalleerde pakketten bij package.json passen, dan pas bouwen.
  assert.equal(STAPPEN[0].opdracht, 'npm ls --depth=0');
  assert.equal(STAPPEN[1].opdracht, 'npm run build');
});

test('--frontend: de frontend-groep plus de repo-groep, geen cargo', () => {
  const gekozen = kiesStappen('frontend');
  assert.ok(gekozen.every((s) => s.groep === 'frontend' || s.groep === 'repo'));
  assert.equal(namen(gekozen)[0], 'pakketten gelijk');
  assert.ok(namen(gekozen).includes('frontend bouwen'));
  assert.ok(namen(gekozen).includes('versies gelijk'));
  assert.ok(!namen(gekozen).includes('cargo test'));
});

test('--rust: de rust-groep plus de repo-groep, en de build alleen als die ontbreekt', () => {
  const zonderBouw = kiesStappen('rust', { bouwBestaat: false });
  assert.equal(namen(zonderBouw)[0], 'frontend bouwen');
  assert.ok(namen(zonderBouw).includes('cargo clippy'));
  assert.ok(!namen(zonderBouw).includes('npm test'));

  const metBouw = kiesStappen('rust', { bouwBestaat: true });
  assert.ok(!namen(metBouw).includes('frontend bouwen'));
  assert.ok(namen(metBouw).includes('cargo deny'));
  assert.ok(namen(metBouw).includes('scripttests'));
});

test('de opdrachten houden de vlaggen --all-features en --audit-level=high', () => {
  const opdracht = (naam) => STAPPEN.find((s) => s.naam === naam).opdracht;
  assert.equal(opdracht('cargo clippy'), 'cargo clippy --workspace --all-targets --all-features -- -D warnings');
  assert.equal(opdracht('cargo test'), 'cargo test --workspace --all-features');
  assert.equal(opdracht('npm audit'), 'npm audit --audit-level=high');
});

test('de opdrachtregel wordt gelezen', () => {
  assert.deepEqual(leesArgumenten([]), { vlag: null, hook: false });
  assert.deepEqual(leesArgumenten(['--rust']), { vlag: 'rust', hook: false });
  assert.deepEqual(leesArgumenten(['--hook']), { vlag: null, hook: true });
  // Git geeft de hook de remote mee.
  assert.deepEqual(leesArgumenten(['--hook', 'origin', 'https://example.com/x.git']), { vlag: null, hook: true });
  assert.ok(leesArgumenten(['--rust', '--frontend']).fout);
  assert.ok(leesArgumenten(['--onzin']).fout);
});

// ---- groene bestand ------------------------------------------------------------------------

test('de nieuwste groene tree komt achteraan en er blijven er hoogstens 20 over', () => {
  assert.deepEqual(nieuweGroenRegels(['a', 'b'], 'c'), ['a', 'b', 'c']);
  assert.deepEqual(nieuweGroenRegels(['a', 'b', 'c'], 'a'), ['b', 'c', 'a']);
  const veel = Array.from({ length: 25 }, (_, i) => `t${i}`);
  const na = nieuweGroenRegels(veel, 'nieuw');
  assert.equal(na.length, 20);
  assert.equal(na.at(-1), 'nieuw');
  assert.equal(na[0], 't6');
});

test('pushregels van git worden gelezen', () => {
  const tekst = `refs/heads/a ${'1'.repeat(40)} refs/heads/a ${NUL}\n\n`;
  assert.deepEqual(leesPushRegels(tekst), [
    { lokaleRef: 'refs/heads/a', lokaleSha: '1'.repeat(40), remoteRef: 'refs/heads/a', remoteSha: NUL },
  ]);
  assert.deepEqual(leesPushRegels(''), []);
});

// ---- de hookbeslissing ---------------------------------------------------------------------

const regel = (remoteRef, lokaleSha = 'abc123') => ({
  lokaleRef: 'refs/heads/x',
  lokaleSha,
  remoteRef,
  remoteSha: NUL,
});

/** Een nepgit: `tree` is de tree van de gepushte commit, `docsVerschil` de groene trees die alleen in documentatie verschillen. */
function nepGit({ tree = 'treeNu', docsVerschil = [] } = {}) {
  const aanroepen = [];
  const git = (args) => {
    aanroepen.push(args);
    if (args[0] === 'rev-parse') return { status: tree ? 0 : 1, stdout: `${tree ?? ''}\n` };
    if (args[0] === 'diff') return { status: docsVerschil.includes(args[2]) ? 0 : 1, stdout: '' };
    return { status: 1, stdout: '' };
  };
  return { git, aanroepen };
}

test('verwijderen van een branch mag zonder controle', () => {
  const { git, aanroepen } = nepGit();
  const r = hookBeslissing([regel('refs/heads/oud', NUL)], { git, groen: [] });
  assert.equal(r.toegestaan, true);
  assert.equal(aanroepen.length, 0);
});

test('een tag mag zonder controle', () => {
  const { git } = nepGit();
  assert.equal(hookBeslissing([regel('refs/tags/v1.0.0')], { git, groen: [] }).toegestaan, true);
});

test('pushen naar main wordt geweigerd, ook met een groene tree', () => {
  const { git } = nepGit();
  const r = hookBeslissing([regel('refs/heads/main')], { git, groen: ['treeNu'] });
  assert.equal(r.toegestaan, false);
  assert.match(r.meldingen[0], /branch en een pull request/);
});

test('een tree die groen was mag door', () => {
  const { git } = nepGit();
  assert.equal(hookBeslissing([regel('refs/heads/feat/x')], { git, groen: ['oud', 'treeNu'] }).toegestaan, true);
});

test('een tree die niet groen was wordt geweigerd met de opdracht om te controleren', () => {
  const { git } = nepGit();
  const r = hookBeslissing([regel('refs/heads/feat/x')], { git, groen: ['oud'] });
  assert.equal(r.toegestaan, false);
  assert.match(r.meldingen[0], /node scripts\/controle\.mjs/);
  assert.doesNotMatch(r.meldingen.join(' '), /no-verify/);
});

test('zonder groene stand wordt een gewone branch geweigerd', () => {
  const { git } = nepGit();
  assert.equal(hookBeslissing([regel('refs/heads/feat/x')], { git, groen: [] }).toegestaan, false);
});

test('als sinds een groene tree alleen documentatie veranderde, mag de push door', () => {
  const { git, aanroepen } = nepGit({ docsVerschil: ['groenB'] });
  const r = hookBeslissing([regel('refs/heads/feat/x')], { git, groen: ['groenA', 'groenB'] });
  assert.equal(r.toegestaan, true);
  // Nieuwste eerst: groenB is als eerste vergeleken, en de documentatiepaden zijn uitgesloten.
  const diff = aanroepen.find((a) => a[0] === 'diff');
  assert.equal(diff[2], 'groenB');
  assert.equal(diff[3], 'treeNu');
  for (const pad of DOCUMENTATIE) assert.ok(diff.includes(`:(exclude)${pad}`), pad);
  assert.ok(DOCUMENTATIE.includes('docs') && DOCUMENTATIE.includes('README.md'));
});

test('een verschil buiten de documentatie telt als nieuwe code', () => {
  const { git } = nepGit({ docsVerschil: [] });
  assert.equal(hookBeslissing([regel('refs/heads/feat/x')], { git, groen: ['groenA'] }).toegestaan, false);
});

test('een push met een geweigerde en een toegestane regel wordt geweigerd', () => {
  const { git } = nepGit();
  const r = hookBeslissing([regel('refs/tags/v1'), regel('refs/heads/main')], { git, groen: ['treeNu'] });
  assert.equal(r.toegestaan, false);
  assert.equal(r.meldingen.length, 1);
});

test('als git de tree niet kan bepalen, wordt de push geweigerd', () => {
  const { git } = nepGit({ tree: null });
  assert.equal(hookBeslissing([regel('refs/heads/feat/x')], { git, groen: ['treeNu'] }).toegestaan, false);
});

// ---- bouwomgeving --------------------------------------------------------------------------

const THUIS = 'D:\\gebruikers\\jan';
const SDK = 'D:\\gebruikers\\jan\\.local\\share\\vulkan-sdk';
const KITS = 'C:\\Program Files (x86)\\Windows Kits\\10\\bin';

test('buiten Windows verandert er niets', () => {
  const r = bouwomgeving({ platform: 'linux', env: {}, thuis: '/home/jan', bestaat: () => true, lijstMap: () => [] });
  assert.deepEqual(r, { env: {}, padVoor: [], meldingen: [] });
});

test('de Vulkan-SDK wordt gezet als die ontbreekt en in de thuismap staat', () => {
  const r = bouwomgeving({
    platform: 'win32',
    env: { PATH: 'C:\\x' },
    thuis: THUIS,
    bestaat: (p) => p === SDK || p === 'C:\\x\\rc.exe',
    lijstMap: () => [],
  });
  assert.equal(r.env.VULKAN_SDK, SDK);
  assert.deepEqual(r.padVoor, [`${SDK}\\Bin`]);
  assert.equal(r.meldingen.length, 1);
});

test('een bestaande VULKAN_SDK blijft staan', () => {
  const r = bouwomgeving({
    platform: 'win32',
    env: { VULKAN_SDK: 'D:\\sdk', PATH: 'C:\\x' },
    thuis: THUIS,
    bestaat: (p) => p === SDK || p === 'C:\\x\\rc.exe',
    lijstMap: () => [],
  });
  assert.deepEqual(r, { env: {}, padVoor: [], meldingen: [] });
});

test('rc.exe: de nieuwste versiemap waar het bestand in staat komt op PATH', () => {
  const r = bouwomgeving({
    platform: 'win32',
    env: { VULKAN_SDK: 'D:\\sdk', PATH: 'C:\\x', 'ProgramFiles(x86)': 'C:\\Program Files (x86)' },
    thuis: THUIS,
    // 10.0.26100 is het nieuwste maar heeft geen rc.exe; 10.0.9 moet numeriek onder 10.0.17134 blijven.
    bestaat: (p) => p === `${KITS}\\10.0.17134\\x64\\rc.exe` || p === `${KITS}\\10.0.9\\x64\\rc.exe`,
    lijstMap: (p) => (p === KITS ? ['10.0.9', '10.0.17134', '10.0.26100', 'x64', 'arm64'] : []),
  });
  assert.deepEqual(r.padVoor, [`${KITS}\\10.0.17134\\x64`]);
  assert.equal(r.meldingen.length, 1);
});

test('rc.exe staat al op PATH: niets zetten', () => {
  const r = bouwomgeving({
    platform: 'win32',
    env: { VULKAN_SDK: 'D:\\sdk', Path: 'C:\\x;C:\\kits', 'ProgramFiles(x86)': 'C:\\Program Files (x86)' },
    thuis: THUIS,
    bestaat: (p) => p === 'C:\\kits\\rc.exe',
    lijstMap: () => ['10.0.26100'],
  });
  assert.deepEqual(r.padVoor, []);
});
