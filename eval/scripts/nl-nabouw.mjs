#!/usr/bin/env node
// Bouwt een nagebouwde Nederlandse vergadering van vier sprekers uit de MLS-fragmenten van
// de CI-set (eval/ci-subset.json), als meetmateriaal voor sprekerherkenning (4b-D, D-0).
// Geen afhankelijkheden, wel Node 20+.
//
// Gebruik:
//   node eval/scripts/nl-nabouw.mjs [--bron <map met mls-*.wav>] [--map <doel>] [--seed 1]
//        [--per-spreker 6]
//       Schrijft <doel>/NL-nabouw.wav (16 kHz mono) en <doel>/NL-nabouw.referentie.json in
//       het referentieformaat van het example `sprekers-meting` (beurten, geen woorden).
//
// Opbouw: van sprekers 880 en 3034 komen alle fragmenten erin, van 3798 en 4429 er zes (te
// kiezen met --per-spreker). De volgorde is geseed willekeurig, zonder dat dezelfde spreker
// twee keer achter elkaar komt. Tussen twee fragmenten zit een pauze van 0,2 tot 1,5 s; bij
// een op vijf wissels begint de volgende spreker juist 0,3 tot 0,8 s voor het eind
// van de vorige (overlap, beide stemmen opgeteld).
//
// Netjes voorgelezen audioboeken dus: een optimistische proef, die vooral laat zien of
// Nederlands op zich een probleem is voor het model.
//
// Bron: Multilingual LibriSpeech, Nederlands (Pratap e.a., 2020), CC BY 4.0. De audio komt
// nooit in git (eval/audio/ wordt genegeerd).

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const hier = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(hier, "..", "..");
const STANDAARD_MAP = resolve(REPO, "eval", "audio", "nabouw");
// Zonder audio in deze map (bijvoorbeeld in een worktree) geef je de bronmap mee met --bron.
const BRON_KANDIDATEN = [resolve(REPO, "eval", "audio", "ci-subset")];

const SAMPLERATE = 16000;
// Sprekers in vaste volgorde; bij ALLE komen alle fragmenten erin, anders --per-spreker.
const SPREKERS = ["3798", "4429", "880", "3034"];
const ALLE = new Set(["880", "3034"]);
const OVERLAP_KANS = 0.2;
const PAUZE_MS = [200, 1500];
const OVERLAP_MS = [300, 800];
const BEGIN_STILTE_MS = 300;

function fout(tekst) {
  console.error(`Fout: ${tekst}`);
  process.exit(1);
}

function leesArgs(lijst) {
  const vlaggen = {};
  for (let i = 0; i < lijst.length; i++) {
    const a = lijst[i];
    if (!a.startsWith("--")) fout(`onbekend argument ${a}.`);
    const waarde = lijst[i + 1];
    if (waarde === undefined) fout(`${a} heeft een waarde nodig.`);
    vlaggen[a.slice(2)] = waarde;
    i++;
  }
  return vlaggen;
}

function getal(vlaggen, naam, standaard) {
  if (vlaggen[naam] === undefined) return standaard;
  const n = Number(vlaggen[naam]);
  if (!Number.isFinite(n)) fout(`--${naam} moet een getal zijn.`);
  return n;
}

// ---------- geseed toeval (mulberry32) ----------

function toeval(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function tussen(rnd, [laag, hoog]) {
  return laag + rnd() * (hoog - laag);
}

function schud(lijst, rnd) {
  const uit = [...lijst];
  for (let i = uit.length - 1; i > 0; i--) {
    const j = Math.floor(rnd() * (i + 1));
    [uit[i], uit[j]] = [uit[j], uit[i]];
  }
  return uit;
}

// ---------- wav ----------

function leesWav(pad) {
  const b = readFileSync(pad);
  if (b.toString("latin1", 0, 4) !== "RIFF" || b.toString("latin1", 8, 12) !== "WAVE") {
    fout(`${pad} is geen wav.`);
  }
  let positie = 12;
  let formaat = null;
  while (positie + 8 <= b.length) {
    const id = b.toString("latin1", positie, positie + 4);
    const lengte = b.readUInt32LE(positie + 4);
    const begin = positie + 8;
    if (id === "fmt ") {
      formaat = {
        soort: b.readUInt16LE(begin),
        kanalen: b.readUInt16LE(begin + 2),
        samplerate: b.readUInt32LE(begin + 4),
        bits: b.readUInt16LE(begin + 14),
      };
    } else if (id === "data") {
      if (!formaat || formaat.soort !== 1 || formaat.kanalen !== 1 || formaat.bits !== 16) {
        fout(`${pad} is geen 16 bit mono pcm.`);
      }
      if (formaat.samplerate !== SAMPLERATE) fout(`${pad} is geen 16 kHz.`);
      const aantal = Math.floor(Math.min(lengte, b.length - begin) / 2);
      const uit = new Float32Array(aantal);
      for (let i = 0; i < aantal; i++) uit[i] = b.readInt16LE(begin + i * 2) / 32768;
      return uit;
    }
    positie = begin + lengte + (lengte % 2);
  }
  fout(`${pad} heeft geen data.`);
}

function wavKop(dataBytes) {
  const k = Buffer.alloc(44);
  k.write("RIFF", 0, "latin1");
  k.writeUInt32LE(dataBytes + 36, 4);
  k.write("WAVE", 8, "latin1");
  k.write("fmt ", 12, "latin1");
  k.writeUInt32LE(16, 16);
  k.writeUInt16LE(1, 20);
  k.writeUInt16LE(1, 22);
  k.writeUInt32LE(SAMPLERATE, 24);
  k.writeUInt32LE(SAMPLERATE * 2, 28);
  k.writeUInt16LE(2, 32);
  k.writeUInt16LE(16, 34);
  k.write("data", 36, "latin1");
  k.writeUInt32LE(dataBytes, 40);
  return k;
}

function naarInt16(samples) {
  const b = Buffer.alloc(samples.length * 2);
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    b.writeInt16LE(Math.round(s * 32767), i * 2);
  }
  return b;
}

// Schaalt alles met één factor terug als de piek boven de grens komt (geen clipping).
function begrens(samples, grens = 0.98) {
  let piek = 0;
  for (let i = 0; i < samples.length; i++) piek = Math.max(piek, Math.abs(samples[i]));
  if (piek <= grens) return 1;
  const factor = grens / piek;
  for (let i = 0; i < samples.length; i++) samples[i] *= factor;
  return factor;
}

// ---------- volgorde ----------

// Kiest per stap een spreker die niet de vorige is, gewogen naar wat die nog over heeft (zo
// loopt het zelden vast). Loopt het toch vast, dan opnieuw met het volgende toevalsgetal.
function bepaalVolgorde(perSpreker, rnd) {
  for (let poging = 0; poging < 1000; poging++) {
    const rest = new Map([...perSpreker].map(([s, lijst]) => [s, [...lijst]]));
    const volgorde = [];
    let vorige = null;
    let vast = false;
    while ([...rest.values()].some((l) => l.length > 0)) {
      const keuzes = [...rest].filter(([s, l]) => s !== vorige && l.length > 0);
      if (keuzes.length === 0) {
        vast = true;
        break;
      }
      const totaal = keuzes.reduce((som, [, l]) => som + l.length, 0);
      let worp = rnd() * totaal;
      let gekozen = keuzes[keuzes.length - 1];
      for (const k of keuzes) {
        worp -= k[1].length;
        if (worp < 0) {
          gekozen = k;
          break;
        }
      }
      volgorde.push({ spreker: gekozen[0], fragment: gekozen[1].shift() });
      vorige = gekozen[0];
    }
    if (!vast) return volgorde;
  }
  fout("geen volgorde gevonden zonder twee keer dezelfde spreker achter elkaar.");
}

// ---------- start ----------

const vlaggen = leesArgs(process.argv.slice(2));
const seed = getal(vlaggen, "seed", 1);
const perSpreker = getal(vlaggen, "per-spreker", 6);
const doel = resolve(vlaggen.map ?? STANDAARD_MAP);
const bron = vlaggen.bron
  ? resolve(vlaggen.bron)
  : BRON_KANDIDATEN.find((p) => existsSync(join(p, "mls-0000.wav")));
if (!bron || !existsSync(bron)) fout("geen bronmap met mls-*.wav gevonden; geef --bron <map>.");

const lijst = JSON.parse(readFileSync(join(hier, "..", "ci-subset.json"), "utf8")).fragmenten;
const rnd = toeval(seed);

const gekozen = new Map();
for (const spreker of SPREKERS) {
  const eigen = lijst.filter((f) => f.id.split("_")[0] === spreker);
  if (eigen.length === 0) fout(`spreker ${spreker} komt niet voor in ci-subset.json.`);
  const keuze = ALLE.has(spreker) ? eigen : schud(eigen, rnd).slice(0, perSpreker);
  if (!ALLE.has(spreker) && keuze.length < perSpreker) {
    fout(`spreker ${spreker} heeft maar ${keuze.length} fragmenten.`);
  }
  gekozen.set(spreker, keuze);
}
const volgorde = bepaalVolgorde(gekozen, rnd);

// Precies een op vijf wissels (afgerond) krijgt overlap, op geseed gekozen plekken; zo
// hangt het aantal niet van het toeval af.
const wissels = volgorde.length - 1;
const metOverlap = new Set(
  schud(Array.from({ length: wissels }, (_, i) => i + 1), rnd).slice(0, Math.round(wissels * OVERLAP_KANS)),
);

// Op de tijdlijn leggen.
const plaatsingen = [];
let eindVorige = 0;
let overlappen = 0;
for (const [i, { spreker, fragment }] of volgorde.entries()) {
  const pad = join(bron, `mls-${String(fragment.rij).padStart(4, "0")}.wav`);
  if (!existsSync(pad)) fout(`${pad} ontbreekt.`);
  const samples = leesWav(pad);
  let beginMs;
  let soort = "begin";
  if (i === 0) {
    beginMs = BEGIN_STILTE_MS;
  } else if (metOverlap.has(i)) {
    beginMs = eindVorige - tussen(rnd, OVERLAP_MS);
    soort = "overlap";
    overlappen++;
  } else {
    beginMs = eindVorige + tussen(rnd, PAUZE_MS);
    soort = "pauze";
  }
  beginMs = Math.round(beginMs);
  const eindMs = Math.round(beginMs + (samples.length * 1000) / SAMPLERATE);
  plaatsingen.push({ spreker, fragment: fragment.id, samples, beginMs, eindMs, soort });
  eindVorige = eindMs;
}

const totaalMs = eindVorige + BEGIN_STILTE_MS;
const mix = new Float32Array(Math.ceil((totaalMs * SAMPLERATE) / 1000));
for (const p of plaatsingen) {
  const start = Math.round((p.beginMs * SAMPLERATE) / 1000);
  for (let i = 0; i < p.samples.length; i++) mix[start + i] += p.samples[i];
}
const factor = begrens(mix);

mkdirSync(doel, { recursive: true });
const data = naarInt16(mix);
const wav = join(doel, "NL-nabouw.wav");
writeFileSync(wav, Buffer.concat([wavKop(data.length), data]));
const referentie = join(doel, "NL-nabouw.referentie.json");
writeFileSync(
  referentie,
  JSON.stringify({
    sprekers: SPREKERS,
    beurten: plaatsingen.map((p) => ({
      spreker: p.spreker,
      start_ms: p.beginMs,
      eind_ms: p.eindMs,
      fragment: p.fragment,
      aanloop: p.soort,
    })),
  }),
);

const perTelling = SPREKERS.map((s) => `${s}=${plaatsingen.filter((p) => p.spreker === s).length}`);
console.log(
  `${plaatsingen.length} fragmenten (${perTelling.join(", ")}), ${overlappen} overlappingen, ` +
    `${(totaalMs / 1000).toFixed(0)} s, seed ${seed}, schaal ${factor.toFixed(3)}.`,
);
console.log(`  ${wav}\n  ${referentie}`);
