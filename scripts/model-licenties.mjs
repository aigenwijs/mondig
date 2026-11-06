#!/usr/bin/env node
// Maakt MODEL_LICENSES.md uit de modelcatalogus (ADR 0016): de naamsvermelding van de
// spraakmodellen komt uit een bron, crates/mondig-core/src/modellen.json. De app leest
// dezelfde velden, dus repo en app lopen nooit uiteen.
//
//   node scripts/model-licenties.mjs           schrijf MODEL_LICENSES.md
//   node scripts/model-licenties.mjs --check   faal als het bestand niet bij de catalogus past

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const cataloguspad = join(root, 'crates', 'mondig-core', 'src', 'modellen.json');
const doelpad = join(root, 'MODEL_LICENSES.md');

const stop = (melding) => {
  console.error(`Fout: ${melding}`);
  process.exit(1);
};

// Dezelfde regels als in Rust (mondig_core::model::vermeldingen): één bestand, twee lezers.
const regelspad = join(root, 'crates', 'mondig-core', 'src', 'vermelding-regels.json');
const REGELS = JSON.parse(readFileSync(regelspad, 'utf8'));

const catalogus = JSON.parse(readFileSync(cataloguspad, 'utf8'));
for (const m of catalogus.modellen) {
  const fout = (reden) => stop(`model ${m.id}: ${reden}`);
  for (const veld of REGELS.tekstvelden) {
    if (typeof m[veld] !== 'string' || m[veld].trim() === '') fout(`veld "${veld}" ontbreekt of is leeg`);
  }
  for (const veld of REGELS.adresvelden) {
    if (typeof m[veld] !== 'string' || !m[veld].startsWith(REGELS.adres_begint_met)) {
      fout(`veld "${veld}" ontbreekt of begint niet met ${REGELS.adres_begint_met}`);
    }
  }
  for (const veld of REGELS.nul_of_tekst) {
    const w = m[veld];
    if (w !== null && (typeof w !== 'string' || w.trim() === '')) {
      fout(`veld "${veld}" ontbreekt (gebruik null als de bron er geen geeft) of is leeg`);
    }
  }
}

const regels = [
  '# Spraakmodellen en licenties',
  '',
  'Dit bestand wordt gemaakt door `scripts/model-licenties.mjs` uit',
  '`crates/mondig-core/src/modellen.json`. Pas het niet met de hand aan.',
  '',
  'Mondig bevat de spraakmodellen zelf niet. De app laat je ze downloaden van Hugging Face,',
  'in een omgezette vorm (GGUF) van handy-computer. Hieronder staat per model van wie het',
  'oorspronkelijk is, onder welke licentie, en wat er aan is gewijzigd.',
  ''
];

for (const m of catalogus.modellen) {
  regels.push(`## ${m.naam}`, '');
  regels.push(`- **Titel:** ${m.titel}`);
  regels.push(`- **Maker:** ${m.maker}`);
  regels.push(`- **Copyright:** ${m.copyright ?? 'de bron geeft geen copyrightregel'}`);
  regels.push(`- **Oorspronkelijk model:** <${m.bron_url}>`);
  regels.push(`- **Licentie:** ${m.licentie}, <${m.licentie_url}>`);
  regels.push(`- **Garantiedisclaimer van de licentie:** <${m.disclaimer_url}>`);
  regels.push(`- **Gewijzigd:** ${m.wijziging}`);
  regels.push(`- **Omgezette versie die Mondig downloadt:** <${m.gguf_url}>`);
  if (m.licentie === 'CC-BY-4.0') {
    regels.push(
      '',
      `Dit materiaal valt onder de licentie ${m.licentie} (Creative Commons Naamsvermelding 4.0).`,
      'De vermelding volgt artikel 3(a)(1) van die licentie: de maker, de copyrightregel',
      '(voor zover de maker die geeft), de licentie, de garantiedisclaimer en een link naar het',
      'materiaal, plus de aanduiding dat het is gewijzigd.'
    );
  }
  if (m.licentie === 'NVIDIA Open Model License') {
    regels.push(
      '',
      'Licensed by NVIDIA Corporation under the NVIDIA Open Model License. De licentie (`LICENSE`)',
      'en het Notice-bestand (`NOTICE`) komen van dezelfde commit mee in de modelmap. Mondig',
      'levert dit model niet mee: wie sprekers wil herkennen, downloadt het zelf (ADR 0018).'
    );
  }
  regels.push('');
}

const tekst = regels.join('\n');

if (process.argv.includes('--check')) {
  let huidig = '';
  try {
    huidig = readFileSync(doelpad, 'utf8').replace(/\r\n/g, '\n');
  } catch {
    stop('MODEL_LICENSES.md ontbreekt. Draai: node scripts/model-licenties.mjs');
  }
  if (huidig !== tekst) {
    stop('MODEL_LICENSES.md past niet bij modellen.json. Draai: node scripts/model-licenties.mjs');
  }
  console.log('MODEL_LICENSES.md past bij modellen.json.');
} else {
  writeFileSync(doelpad, tekst, 'utf8');
  console.log(`MODEL_LICENSES.md geschreven (${catalogus.modellen.length} modellen).`);
}
