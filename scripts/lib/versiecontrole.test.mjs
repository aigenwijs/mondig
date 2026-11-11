// Tests van de versiecontroles. Draaien: node --test scripts/lib/versiecontrole.test.mjs
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { controleerChangelog, controleerUitgave, controleerUitgebracht, isDevVersie } from './versiecontrole.mjs';

const CHANGELOG = `# Changelog

## [Nog niet uitgebracht]

## [0.5.0] - 2026-11-11

### Toegevoegd
`;

test('een ontwikkelversie hoeft geen changelog-kop te hebben', () => {
  assert.equal(isDevVersie('0.6.0-dev'), true);
  assert.equal(isDevVersie('0.6.0'), false);
  assert.equal(controleerChangelog('0.6.0-dev', CHANGELOG), null);
});

test('een uitgebrachte versie met een kop met datum slaagt', () => {
  assert.equal(controleerChangelog('0.5.0', CHANGELOG), null);
});

test('een uitgebrachte versie zonder kop faalt met een melding die zegt wat te doen', () => {
  const melding = controleerChangelog('0.6.0', CHANGELOG);
  assert.match(melding, /0\.6\.0/);
  assert.match(melding, /## \[0\.6\.0\] - JJJJ-MM-DD/);
});

test('een kop zonder datum of voor een andere versie telt niet', () => {
  assert.notEqual(controleerChangelog('0.5.0', '## [0.5.0]\n'), null);
  assert.notEqual(controleerChangelog('0.5.0', '## [0.5.0] - gisteren\n'), null);
  // De punten in de versie zijn geen jokertekens.
  assert.notEqual(controleerChangelog('0.5.0', '## [0x5x0] - 2026-11-11\n'), null);
  assert.notEqual(controleerChangelog('0.5', '## [0.5.0] - 2026-11-11\n'), null);
});

test('een tag op een andere commit dan HEAD betekent: al uitgebracht', () => {
  const melding = controleerUitgebracht('0.5.0', 'aaa111', 'bbb222');
  assert.match(melding, /v0\.5\.0/);
  assert.match(melding, /versie\.mjs <volgende>-dev/);
});

test('een tag op HEAD, een ontbrekende tag of ontbrekend git geeft geen fout', () => {
  assert.equal(controleerUitgebracht('0.5.0', 'aaa111', 'aaa111'), null);
  assert.equal(controleerUitgebracht('0.5.0', null, 'bbb222'), null);
  assert.equal(controleerUitgebracht('0.5.0', 'aaa111', null), null);
});

test('een ontwikkelversie is nooit uitgebracht, ook niet als de tag bestaat', () => {
  assert.equal(controleerUitgebracht('0.6.0-dev', 'aaa111', 'bbb222'), null);
});

test('een uitgave met een ontwikkelversie wordt geweigerd, met wat te doen', () => {
  const melding = controleerUitgave('0.6.0-dev', 'v0.6.0-dev');
  assert.match(melding, /ontwikkelversie/);
  assert.match(melding, /versie\.mjs <versie>/);
  assert.match(melding, /CHANGELOG\.md/);
});

test('een tag die niet bij de versie hoort wordt geweigerd, ook zonder v', () => {
  const melding = controleerUitgave('0.6.0', 'v0.7.0');
  assert.match(melding, /v0\.7\.0/);
  assert.match(melding, /Verwacht: v0\.6\.0/);
  assert.notEqual(controleerUitgave('0.6.0', '0.6.0'), null);
});

test('een lege of ontbrekende tag wordt geweigerd', () => {
  assert.notEqual(controleerUitgave('0.6.0', ''), null);
  assert.notEqual(controleerUitgave('0.6.0', '  '), null);
  assert.notEqual(controleerUitgave('0.6.0', undefined), null);
});

test('de juiste tag slaagt, ook bij een andere voorlopige versie', () => {
  assert.equal(controleerUitgave('0.6.0', 'v0.6.0'), null);
  assert.equal(controleerUitgave('1.0.0-beta.1', 'v1.0.0-beta.1'), null);
});
