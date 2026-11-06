#!/usr/bin/env node
// Meet het uitschrijven van vergaderingen (Jij en Anderen) op de openbare AMI Meeting
// Corpus-vergadering ES2002a. Geen afhankelijkheden, wel Node 20+ en een `tar` die zip
// kan lezen (Windows 10/11 heeft die; op Windows kiest het script bsdtar in System32).
//
// Gebruik:
//   node eval/scripts/ami-vergadering.mjs bouw [--map <doel>] [--spreker A|B|C|D]
//        [--microfoon-vertraging-ms 100] [--echo-vertraging-ms 120] [--echo-db -15]
//        [--galm-s 0.3] [--galm-mix 0.5]
//       Haalt de audio en de annotaties op (met cache), zoekt in meetings.xml welke
//       spreker bij welk headsetkanaal hoort, en bouwt twee opnamemappen in het formaat
//       van 4b-A: <doel>/ES2002a-koptelefoon en <doel>/ES2002a-luidspreker.
//   node eval/scripts/ami-vergadering.mjs meet <uitschrijving.json> [--map <doel>]
//        [--fouten 5] [--speling-ms 250] [--basis <uitschrijving.json>]
//       Vergelijkt de uitvoer van `mondig-cli vergader uitschrijven --json` met de
//       referentiewoorden. De naam van de opnamemap in de JSON bepaalt welke opname het is.
//       Met --basis (dezelfde opname uitgeschreven met --geen-ontdubbelen) meet het ook
//       hoeveel echte Jij-woorden het ontdubbelen heeft weggehaald. Hebben de beurten van
//       Anderen een sprekernummer (4b-D), dan meet (e) welk deel van de Anderen-woorden bij
//       de juiste spreker staat, na de best passende koppeling tussen gevonden en echte
//       sprekers (zoals het example sprekers-meting).
//   node eval/scripts/ami-vergadering.mjs sprekers [--map <doel>] [--spreker A|B|C|D]
//        [--beurt-pauze-ms 500]
//       Meetmateriaal voor sprekerherkenning (4b-D). Bouwt, met dezelfde cache, twee wavs
//       (16 kHz mono) met een referentie-json ernaast, voor het example
//       `sprekers-meting` van mondig-engine: <doel>/ES2002a-anderen.wav (de som van de drie
//       headsetkanalen zonder de "Jij"-spreker) en <doel>/ES2002a-alle4.wav (alle vier).
//       In de json staan de sprekers (A tot D), de beurten (opeenvolgende woorden van een
//       spreker met minder dan --beurt-pauze-ms ertussen) en de woorden met tijden.
//
// Opnames:
//   koptelefoon: microfoon = het headsetkanaal van een spreker ("Jij"), systeem = de som van
//     de drie andere kanalen ("Anderen"). De ankers van de microfoonstukken staan 100 ms
//     later dan de werkelijke tijd, zoals een echte microfoon achterloopt.
//   luidspreker: hetzelfde, maar de microfoon krijgt er het systeemspoor bij als echo:
//     vertraging, demping en een eenvoudige galm (vier kamfilters en twee allpass-filters).
//
// De spreker is standaard de tweede in aantal woorden, zodat Jij een echte gesprekspartner
// is en niet de dominante of de stille spreker.
//
// Bron: AMI Meeting Corpus (Carletta e.a.), CC BY 4.0,
// https://groups.inf.ed.ac.uk/ami/corpus/license.shtml. De audio komt nooit in git
// (eval/audio/ wordt genegeerd).

import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const hier = dirname(fileURLToPath(import.meta.url));
const STANDAARD_MAP = resolve(hier, "..", "audio", "ami");

const VERGADERING = "ES2002a";
const AUDIO_URL = (n) =>
  `https://groups.inf.ed.ac.uk/ami/AMICorpusMirror/amicorpus/${VERGADERING}/audio/${VERGADERING}.Headset-${n}.wav`;
const ANNOTATIE_URL =
  "https://groups.inf.ed.ac.uk/ami/AMICorpusAnnotations/ami_public_manual_1.6.2.zip";

const SAMPLERATE = 16000;
const STUK_SAMPLES = 60 * SAMPLERATE;

function fout(tekst) {
  console.error(`Fout: ${tekst}`);
  process.exit(1);
}

// ---------- opdrachtregel ----------

function leesArgs(lijst) {
  const vlaggen = {};
  const los = [];
  for (let i = 0; i < lijst.length; i++) {
    const a = lijst[i];
    if (a.startsWith("--")) {
      const waarde = lijst[i + 1];
      if (waarde === undefined) fout(`${a} heeft een waarde nodig.`);
      vlaggen[a.slice(2)] = waarde;
      i++;
    } else {
      los.push(a);
    }
  }
  return { vlaggen, los };
}

function getal(vlaggen, naam, standaard) {
  if (vlaggen[naam] === undefined) return standaard;
  const n = Number(vlaggen[naam]);
  if (!Number.isFinite(n)) fout(`--${naam} moet een getal zijn.`);
  return n;
}

// ---------- ophalen ----------

async function download(url, pad, minimaleBytes) {
  if (existsSync(pad) && readFileSync(pad).length >= minimaleBytes) return;
  let laatste;
  for (let poging = 1; poging <= 4; poging++) {
    try {
      console.error(`Ophalen: ${url}`);
      const antwoord = await fetch(url);
      if (!antwoord.ok) throw new Error(`HTTP ${antwoord.status}`);
      const bytes = Buffer.from(await antwoord.arrayBuffer());
      if (bytes.length < minimaleBytes) throw new Error("bestand te klein");
      writeFileSync(pad, bytes);
      return;
    } catch (e) {
      laatste = e;
      await new Promise((klaar) => setTimeout(klaar, 3000 * poging));
    }
  }
  fout(`ophalen van ${url} mislukt: ${laatste?.message ?? laatste}`);
}

function tarProgramma() {
  // GNU tar (Git Bash) kan geen zip lezen; bsdtar in System32 wel.
  if (process.platform === "win32") {
    const pad = join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe");
    if (existsSync(pad)) return pad;
  }
  return "tar";
}

function pakAnnotatiesUit(map) {
  const doel = join(map, "annotaties");
  const woorden = ["A", "B", "C", "D"].map((s) =>
    join(doel, "words", `${VERGADERING}.${s}.words.xml`),
  );
  const meetings = join(doel, "corpusResources", "meetings.xml");
  if (woorden.every(existsSync) && existsSync(meetings)) return doel;
  mkdirSync(doel, { recursive: true });
  const r = spawnSync(
    tarProgramma(),
    [
      "-xf",
      join(map, "ami_public_manual_1.6.2.zip"),
      "-C",
      doel,
      `words/${VERGADERING}.*.words.xml`,
      "corpusResources/meetings.xml",
    ],
    { encoding: "utf8" },
  );
  if (r.status !== 0 || !woorden.every(existsSync) || !existsSync(meetings)) {
    fout(`uitpakken van de annotaties mislukt: ${r.stderr || r.error}`);
  }
  return doel;
}

// ---------- annotaties ----------

// Welk headsetkanaal hoort bij welke spreker (A tot D), uit meetings.xml.
function kanalen(annotatiemap) {
  const xml = readFileSync(join(annotatiemap, "corpusResources", "meetings.xml"), "latin1");
  const blok = xml.match(
    new RegExp(`<meeting [^>]*observation="${VERGADERING}"[^>]*>([\\s\\S]*?)</meeting>`),
  );
  if (!blok) fout(`${VERGADERING} staat niet in meetings.xml.`);
  const uit = {};
  for (const m of blok[1].matchAll(/<speaker\b[^>]*>/g)) {
    const kanaal = m[0].match(/\bchannel="(\d+)"/);
    const agent = m[0].match(/\bnxt_agent="([A-Z])"/);
    if (kanaal && agent) uit[agent[1]] = Number(kanaal[1]);
  }
  if (Object.keys(uit).length !== 4) fout("meetings.xml geeft niet vier sprekers met een kanaal.");
  return uit;
}

// Echte woorden per spreker: <w> zonder punc. Vocalsounds zijn andere elementen en tellen niet.
function leesWoorden(annotatiemap, spreker) {
  const xml = readFileSync(
    join(annotatiemap, "words", `${VERGADERING}.${spreker}.words.xml`),
    "latin1",
  );
  const woorden = [];
  for (const m of xml.matchAll(/<w\b([^>]*)>([^<]*)<\/w>/g)) {
    const kenmerken = m[1];
    if (/\bpunc=/.test(kenmerken)) continue;
    const begin = Number(kenmerken.match(/\bstarttime="([\d.]+)"/)?.[1]);
    const eind = Number(kenmerken.match(/\bendtime="([\d.]+)"/)?.[1]);
    if (!Number.isFinite(begin) || !Number.isFinite(eind)) continue;
    woorden.push({
      spreker,
      tekst: m[2].replace(/&#39;/g, "'").replace(/&amp;/g, "&"),
      midden_ms: ((begin + eind) / 2) * 1000,
      start_ms: Math.round(begin * 1000),
      eind_ms: Math.round(eind * 1000),
    });
  }
  return woorden;
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

// ---------- signaal ----------

function piek(samples) {
  let p = 0;
  for (let i = 0; i < samples.length; i++) p = Math.max(p, Math.abs(samples[i]));
  return p;
}

// Schaalt alles met één factor terug als de piek boven de grens komt (geen clipping).
function begrens(samples, grens = 0.98) {
  const p = piek(samples);
  if (p <= grens) return { samples, factor: 1 };
  const factor = grens / p;
  for (let i = 0; i < samples.length; i++) samples[i] *= factor;
  return { samples, factor };
}

function vertraag(samples, n) {
  const uit = new Float32Array(samples.length);
  uit.set(samples.subarray(0, Math.max(0, samples.length - n)), n);
  return uit;
}

// Eenvoudige galm van O(n): vier parallelle kamfilters en twee allpass-filters in serie
// (naar het ontwerp van Schroeder/Freeverb). `rt60` is de nagalmtijd in seconden.
function galm(samples, rt60) {
  const kamVertragingen = [405, 431, 463, 492]; // samples bij 16 kHz, ongeveer 25 tot 31 ms
  const allpassVertragingen = [202, 160];
  const uit = new Float32Array(samples.length);
  for (const d of kamVertragingen) {
    const g = 10 ** ((-3 * (d / SAMPLERATE)) / rt60);
    const buf = new Float32Array(d);
    let p = 0;
    for (let i = 0; i < samples.length; i++) {
      const y = samples[i] + g * buf[p];
      buf[p] = y;
      p = (p + 1) % d;
      uit[i] += (y * (1 - g)) / kamVertragingen.length;
    }
  }
  for (const d of allpassVertragingen) {
    const g = 0.5;
    const buf = new Float32Array(d);
    let p = 0;
    for (let i = 0; i < uit.length; i++) {
      const x = uit[i];
      const b = buf[p];
      const y = -g * x + b;
      buf[p] = x + g * b;
      p = (p + 1) % d;
      uit[i] = y;
    }
  }
  return uit;
}

// ---------- opnamemap schrijven ----------

function schrijfSpoor(spoormap, samples, ankerVerschuivingMs) {
  rmSync(spoormap, { recursive: true, force: true });
  mkdirSync(spoormap, { recursive: true });
  const regels = [];
  const stukken = Math.ceil(samples.length / STUK_SAMPLES);
  for (let nr = 0; nr < stukken; nr++) {
    const deel = samples.subarray(nr * STUK_SAMPLES, (nr + 1) * STUK_SAMPLES);
    const data = naarInt16(deel);
    const bestand = `${String(nr).padStart(5, "0")}.wav`;
    writeFileSync(join(spoormap, bestand), Buffer.concat([wavKop(data.length), data]));
    regels.push(
      JSON.stringify({
        stuk: nr,
        bestand,
        samples: deel.length,
        begin_ms: Math.floor((nr * STUK_SAMPLES * 1000) / SAMPLERATE) + ankerVerschuivingMs,
      }),
    );
  }
  writeFileSync(join(spoormap, "stukken.jsonl"), `${regels.join("\n")}\n`);
  return { stukken, samples: samples.length, stil: piek(samples) < 0.001 };
}

function schrijfOpname(map, mic, systeem, ankerMicMs) {
  rmSync(map, { recursive: true, force: true });
  mkdirSync(map, { recursive: true });
  const duurMs = Math.floor((mic.length * 1000) / SAMPLERATE);
  const m = schrijfSpoor(join(map, "microfoon"), mic, ankerMicMs);
  const s = schrijfSpoor(join(map, "systeem"), systeem, 0);
  const samenvatting = (x) => ({
    stukken: x.stukken,
    samples: x.samples,
    verloren: 0,
    stil: x.stil,
    onderbroken: false,
  });
  writeFileSync(
    join(map, "opname.json"),
    JSON.stringify({
      versie: 1,
      begin_unix_ms: Date.UTC(2004, 11, 13),
      samplerate: SAMPLERATE,
      stuk_seconden: 60,
      systeemgeluid: true,
    }),
  );
  writeFileSync(
    join(map, "einde.json"),
    JSON.stringify({
      duur_ms: duurMs,
      hersteld: false,
      microfoon: samenvatting(m),
      systeem: samenvatting(s),
    }),
  );
}

// ---------- subopdracht bouw ----------

async function bouw(vlaggen) {
  const map = resolve(vlaggen.map ?? STANDAARD_MAP);
  mkdirSync(map, { recursive: true });
  const ankerMicMs = getal(vlaggen, "microfoon-vertraging-ms", 100);
  const echoMs = getal(vlaggen, "echo-vertraging-ms", 120);
  const echoDb = getal(vlaggen, "echo-db", -15);
  const galmS = getal(vlaggen, "galm-s", 0.3);
  const galmMix = getal(vlaggen, "galm-mix", 0.5);

  for (let n = 0; n < 4; n++) {
    await download(AUDIO_URL(n), join(map, `${VERGADERING}.Headset-${n}.wav`), 30_000_000);
  }
  await download(ANNOTATIE_URL, join(map, "ami_public_manual_1.6.2.zip"), 20_000_000);
  const annotaties = pakAnnotatiesUit(map);

  const kanaalVan = kanalen(annotaties);
  const woordenPer = {};
  for (const s of ["A", "B", "C", "D"]) woordenPer[s] = leesWoorden(annotaties, s);
  const rangorde = Object.keys(woordenPer).sort(
    (a, b) => woordenPer[b].length - woordenPer[a].length,
  );
  const spreker = vlaggen.spreker ?? rangorde[1];
  if (!kanaalVan[spreker]) fout(`onbekende spreker ${spreker}.`);
  const jijKanaal = kanaalVan[spreker];

  const sporen = [0, 1, 2, 3].map((n) => leesWav(join(map, `${VERGADERING}.Headset-${n}.wav`)));
  const lengte = Math.min(...sporen.map((s) => s.length));
  const jij = sporen[jijKanaal].subarray(0, lengte);
  const som = new Float32Array(lengte);
  for (let n = 0; n < 4; n++) {
    if (n === jijKanaal) continue;
    for (let i = 0; i < lengte; i++) som[i] += sporen[n][i];
  }
  const systeem = begrens(som);

  // Echo van het systeemspoor in de microfoon: vertraagd, gedempt, met galm.
  const wet = galm(systeem.samples, galmS);
  const gemengd = new Float32Array(lengte);
  for (let i = 0; i < lengte; i++) gemengd[i] = systeem.samples[i] + galmMix * wet[i];
  const echo = vertraag(gemengd, Math.round((echoMs * SAMPLERATE) / 1000));
  const gain = 10 ** (echoDb / 20);
  const micLuid = new Float32Array(lengte);
  for (let i = 0; i < lengte; i++) micLuid[i] = jij[i] + gain * echo[i];
  const micLuidBegrensd = begrens(micLuid);

  const koptelefoon = join(map, `${VERGADERING}-koptelefoon`);
  const luidspreker = join(map, `${VERGADERING}-luidspreker`);
  schrijfOpname(koptelefoon, Float32Array.from(jij), systeem.samples, ankerMicMs);
  schrijfOpname(luidspreker, micLuidBegrensd.samples, systeem.samples, ankerMicMs);

  const meta = {
    vergadering: VERGADERING,
    jij_spreker: spreker,
    jij_kanaal: jijKanaal,
    woorden_per_spreker: Object.fromEntries(
      Object.entries(woordenPer).map(([s, w]) => [s, w.length]),
    ),
    kanaal_per_spreker: kanaalVan,
    microfoon_anker_ms: ankerMicMs,
    echo: { vertraging_ms: echoMs, db: echoDb, galm_s: galmS, galm_mix: galmMix },
    schaal_systeem: systeem.factor,
    schaal_microfoon_luidspreker: micLuidBegrensd.factor,
    duur_ms: Math.floor((lengte * 1000) / SAMPLERATE),
  };
  writeFileSync(join(map, `${VERGADERING}.meta.json`), JSON.stringify(meta, null, 2));
  console.log(
    `Jij = spreker ${spreker} (kanaal ${jijKanaal}, ${woordenPer[spreker].length} woorden; ` +
      `volgorde op woorden: ${rangorde.map((s) => `${s}=${woordenPer[s].length}`).join(", ")}).`,
  );
  console.log(`Systeemschaal ${systeem.factor.toFixed(3)}, luidsprekermicrofoon ${micLuidBegrensd.factor.toFixed(3)}.`);
  console.log(`Klaar:\n  ${koptelefoon}\n  ${luidspreker}`);
}

// ---------- subopdracht sprekers ----------

// Voegt opeenvolgende woorden van één spreker samen tot beurten zolang er minder dan
// `pauzeMs` tussen zit.
function beurtenUitWoorden(woorden, pauzeMs) {
  const beurten = [];
  const gesorteerd = [...woorden].sort((a, b) => a.start_ms - b.start_ms);
  for (const w of gesorteerd) {
    const laatste = beurten.at(-1);
    if (laatste && w.start_ms - laatste.eind_ms < pauzeMs) {
      laatste.eind_ms = Math.max(laatste.eind_ms, w.eind_ms);
    } else {
      beurten.push({ spreker: w.spreker, start_ms: w.start_ms, eind_ms: w.eind_ms });
    }
  }
  return beurten;
}

function schrijfSprekersOpname(map, naam, samples, sprekers, woordenPer, pauzeMs) {
  const wav = join(map, `${VERGADERING}-${naam}.wav`);
  const data = naarInt16(samples);
  writeFileSync(wav, Buffer.concat([wavKop(data.length), data]));
  const woorden = sprekers
    .flatMap((s) => woordenPer[s])
    .map((w) => ({ spreker: w.spreker, start_ms: w.start_ms, eind_ms: w.eind_ms }))
    .sort((a, b) => a.start_ms - b.start_ms);
  // Eén lijst per spreker, zodat beurten van verschillende sprekers elkaar mogen overlappen.
  const beurten = sprekers
    .flatMap((s) => beurtenUitWoorden(woordenPer[s], pauzeMs))
    .sort((a, b) => a.start_ms - b.start_ms);
  const json = join(map, `${VERGADERING}-${naam}.referentie.json`);
  writeFileSync(json, JSON.stringify({ sprekers, beurten, woorden }));
  return { wav, json, beurten: beurten.length, woorden: woorden.length };
}

async function sprekersOpdracht(vlaggen) {
  const map = resolve(vlaggen.map ?? STANDAARD_MAP);
  mkdirSync(map, { recursive: true });
  const pauzeMs = getal(vlaggen, "beurt-pauze-ms", 500);

  for (let n = 0; n < 4; n++) {
    await download(AUDIO_URL(n), join(map, `${VERGADERING}.Headset-${n}.wav`), 30_000_000);
  }
  await download(ANNOTATIE_URL, join(map, "ami_public_manual_1.6.2.zip"), 20_000_000);
  const annotaties = pakAnnotatiesUit(map);

  const kanaalVan = kanalen(annotaties);
  const woordenPer = {};
  for (const s of ["A", "B", "C", "D"]) woordenPer[s] = leesWoorden(annotaties, s);
  const rangorde = Object.keys(woordenPer).sort(
    (a, b) => woordenPer[b].length - woordenPer[a].length,
  );
  const jij = vlaggen.spreker ?? rangorde[1];
  if (!kanaalVan[jij]) fout(`onbekende spreker ${jij}.`);

  const sporen = [0, 1, 2, 3].map((n) => leesWav(join(map, `${VERGADERING}.Headset-${n}.wav`)));
  const lengte = Math.min(...sporen.map((s) => s.length));
  const som = (sprekers) => {
    const uit = new Float32Array(lengte);
    for (const s of sprekers) {
      const spoor = sporen[kanaalVan[s]];
      for (let i = 0; i < lengte; i++) uit[i] += spoor[i];
    }
    return begrens(uit).samples;
  };

  const anderen = ["A", "B", "C", "D"].filter((s) => s !== jij);
  const a = schrijfSprekersOpname(map, "anderen", som(anderen), anderen, woordenPer, pauzeMs);
  const b = schrijfSprekersOpname(map, "alle4", som(["A", "B", "C", "D"]), ["A", "B", "C", "D"], woordenPer, pauzeMs);
  console.log(`Anderen = ${anderen.join(", ")} (Jij = ${jij}). Duur ${(lengte / SAMPLERATE).toFixed(0)} s.`);
  for (const [naam, x] of [["anderen", a], ["alle4", b]]) {
    console.log(`  ${naam}: ${x.beurten} beurten, ${x.woorden} woorden
    ${x.wav}
    ${x.json}`);
  }
}

// ---------- subopdracht meet ----------

const pct = (deel, totaal) => (totaal === 0 ? "n.v.t." : `${((100 * deel) / totaal).toFixed(1)}%`);

function mmss(ms) {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

function meet(vlaggen, los) {
  const bestand = los[0];
  if (!bestand) fout("geef het JSON-bestand van `vergader uitschrijven --json`.");
  const map = resolve(vlaggen.map ?? STANDAARD_MAP);
  const speling = getal(vlaggen, "speling-ms", 250);
  const aantalFouten = getal(vlaggen, "fouten", 5);
  const meta = JSON.parse(readFileSync(join(map, `${VERGADERING}.meta.json`), "utf8"));
  const uitschrijving = JSON.parse(readFileSync(resolve(bestand), "utf8"));
  const annotaties = join(map, "annotaties");
  const referentie = ["A", "B", "C", "D"].flatMap((s) => leesWoorden(annotaties, s));
  for (const w of referentie) w.kant = w.spreker === meta.jij_spreker ? "jij" : "anderen";
  const beurten = uitschrijving.beurten ?? [];

  console.log(`Opname: ${uitschrijving.opname}`);
  console.log(`Model: ${uitschrijving.model}, taal ${uitschrijving.taal}; Jij = spreker ${meta.jij_spreker}.`);
  console.log(`Referentiewoorden: ${referentie.length} (Jij ${referentie.filter((w) => w.kant === "jij").length}); beurten van Mondig: ${beurten.length}.`);

  // (a) woordniveau
  let juist = 0;
  let verkeerd = 0;
  let ongedekt = 0;
  const perKantWoord = { jij: [0, 0, 0], anderen: [0, 0, 0] };
  for (const w of referentie) {
    const dekkend = beurten.filter(
      (b) => w.midden_ms >= b.start_ms - speling && w.midden_ms <= b.eind_ms + speling,
    );
    const goed = dekkend.some((b) => b.kant === w.kant);
    const teller = perKantWoord[w.kant];
    if (goed) {
      juist++;
      teller[0]++;
    } else if (dekkend.length > 0) {
      verkeerd++;
      teller[1]++;
    } else {
      ongedekt++;
      teller[2]++;
    }
  }
  const n = referentie.length;
  console.log(`\n(a) Woordniveau (midden binnen een beurt, speling ${speling} ms)`);
  console.log(`  juiste kant    ${juist}/${n}  ${pct(juist, n)}`);
  console.log(`  verkeerde kant ${verkeerd}/${n}  ${pct(verkeerd, n)}`);
  console.log(`  niet gedekt    ${ongedekt}/${n}  ${pct(ongedekt, n)}`);
  for (const kant of ["jij", "anderen"]) {
    const [a, b, c] = perKantWoord[kant];
    const t = a + b + c;
    console.log(`    referentie ${kant}: juist ${pct(a, t)}, verkeerd ${pct(b, t)}, niet gedekt ${pct(c, t)} (${t} woorden)`);
  }

  // (b) beurtniveau: de kant met de meeste referentiewoorden in het tijdvak van de beurt
  const beoordeeld = beurten.map((b) => {
    let jij = 0;
    let anderen = 0;
    for (const w of referentie) {
      if (w.midden_ms >= b.start_ms && w.midden_ms <= b.eind_ms) {
        if (w.kant === "jij") jij++;
        else anderen++;
      }
    }
    const meerderheid = jij > anderen ? "jij" : anderen > jij ? "anderen" : jij === 0 ? "leeg" : "gelijk";
    return { ...b, ref_jij: jij, ref_anderen: anderen, meerderheid };
  });
  const metWoorden = beoordeeld.filter((b) => b.meerderheid !== "leeg");
  const goed = metWoorden.filter((b) => b.meerderheid === b.kant);
  const leeg = beoordeeld.filter((b) => b.meerderheid === "leeg").length;
  const gelijk = beoordeeld.filter((b) => b.meerderheid === "gelijk").length;
  console.log(`\n(b) Beurtniveau (kant met de meeste referentiewoorden in het tijdvak)`);
  console.log(`  beurten met referentiewoorden: ${metWoorden.length}; zonder referentiewoorden: ${leeg}; gelijkstand (telt als fout): ${gelijk}`);
  console.log(`  juiste kant ${goed.length}/${metWoorden.length}  ${pct(goed.length, metWoorden.length)}`);
  for (const kant of ["jij", "anderen"]) {
    const deze = metWoorden.filter((b) => b.kant === kant);
    const g = deze.filter((b) => b.meerderheid === kant).length;
    console.log(`    beurten van Mondig als ${kant}: ${g}/${deze.length}  ${pct(g, deze.length)}`);
  }

  // (b2) tekstmaat: welke kant heeft de woorden die in de beurt staan? Per beurt tellen we
  // hoeveel van haar woorden ook in de referentie van Jij, en in die van Anderen, binnen het
  // tijdvak (met speling) voorkomen. Eerlijker bij korte tussenwerpsels die over een lange
  // uiting van iemand anders heen vallen: daar wint op woordtelling steeds de lange uiting.
  const normaal = (t) =>
    String(t).toLowerCase().replace(/[^a-z0-9']+/g, " ").trim().split(/\s+/).filter(Boolean);
  const overlap = (tokens, woorden) => {
    const voorraad = new Map();
    for (const w of woorden) voorraad.set(w, (voorraad.get(w) ?? 0) + 1);
    let n = 0;
    for (const t of tokens) {
      const rest = voorraad.get(t) ?? 0;
      if (rest > 0) {
        n++;
        voorraad.set(t, rest - 1);
      }
    }
    return n;
  };
  for (const b of beoordeeld) {
    const dekking = referentie.filter(
      (w) => w.midden_ms >= b.start_ms - speling && w.midden_ms <= b.eind_ms + speling,
    );
    const tokens = normaal(b.tekst);
    const van = (kant) => dekking.filter((w) => w.kant === kant).flatMap((w) => normaal(w.tekst));
    b.tekst_jij = overlap(tokens, van("jij"));
    b.tekst_anderen = overlap(tokens, van("anderen"));
    b.tekstmeerderheid =
      b.tekst_jij > b.tekst_anderen
        ? "jij"
        : b.tekst_anderen > b.tekst_jij
          ? "anderen"
          : b.tekst_jij === 0
            ? "leeg"
            : "gelijk";
  }
  const tekstBeoordeeld = beoordeeld.filter((b) => b.tekstmeerderheid !== "leeg");
  const tekstGoed = tekstBeoordeeld.filter((b) => b.tekstmeerderheid === b.kant).length;
  console.log(`\n(b2) Beurtniveau op tekst (welke kant heeft de woorden uit de beurt)`);
  console.log(
    `  juiste kant ${tekstGoed}/${tekstBeoordeeld.length}  ${pct(tekstGoed, tekstBeoordeeld.length)} ` +
      `(zonder overlap met de referentie: ${beoordeeld.length - tekstBeoordeeld.length}; gelijkstand telt als fout)`,
  );
  for (const kant of ["jij", "anderen"]) {
    const deze = tekstBeoordeeld.filter((b) => b.kant === kant);
    const g = deze.filter((b) => b.tekstmeerderheid === kant).length;
    console.log(`    beurten van Mondig als ${kant}: ${g}/${deze.length}  ${pct(g, deze.length)}`);
  }
  const lang = metWoorden.filter((b) => b.ref_jij + b.ref_anderen >= 5);
  const langGoed = lang.filter((b) => b.meerderheid === b.kant).length;
  console.log(`  (b) alleen beurten met minstens 5 referentiewoorden: ${langGoed}/${lang.length}  ${pct(langGoed, lang.length)}`);

  // (c) echo: Jij-beurten waarvan de referentie vooral Anderen is
  const jijBeurten = metWoorden.filter((b) => b.kant === "jij");
  const echoBeurten = jijBeurten.filter((b) => b.meerderheid === "anderen");
  const jijTekst = jijBeurten.reduce((s, b) => s + b.ref_jij + b.ref_anderen, 0);
  const echoWoorden = jijBeurten.reduce((s, b) => s + b.ref_anderen, 0);
  console.log(`\n(c) Echo: Jij-beurten waarvan de referentie vooral Anderen is`);
  console.log(`  ${echoBeurten.length}/${jijBeurten.length}  ${pct(echoBeurten.length, jijBeurten.length)}`);
  console.log(`  referentiewoorden van Anderen binnen alle Jij-beurten: ${echoWoorden}/${jijTekst}  ${pct(echoWoorden, jijTekst)}`);

  // (d) echte Jij-woorden die door geen Jij-beurt gedekt worden. Zonder --basis is dat alles
  // wat niet in een Jij-beurt staat (ook wat het model miste); met --basis alleen wat de
  // uitschrijving zonder ontdubbelen nog wel dekte en nu niet meer.
  const jijWoorden = referentie.filter((w) => w.kant === "jij");
  const gedektDoorJij = (lijst, w) =>
    lijst.some((b) => b.kant === "jij" && w.midden_ms >= b.start_ms - speling && w.midden_ms <= b.eind_ms + speling);
  const nuOngedekt = jijWoorden.filter((w) => !gedektDoorJij(beurten, w));
  console.log(`
(d) Echte Jij-woorden zonder Jij-beurt eromheen`);
  console.log(`  ${nuOngedekt.length}/${jijWoorden.length}  ${pct(nuOngedekt.length, jijWoorden.length)} (weggelaten door ontdubbelen: ${uitschrijving.weggelaten ?? "onbekend"} uitingen)`);
  if (vlaggen.basis) {
    const basis = JSON.parse(readFileSync(resolve(vlaggen.basis), "utf8")).beurten ?? [];
    const eerstGedekt = jijWoorden.filter((w) => gedektDoorJij(basis, w));
    const verloren = eerstGedekt.filter((w) => !gedektDoorJij(beurten, w));
    console.log(`  waarvan verloren ten opzichte van zonder ontdubbelen: ${verloren.length}/${jijWoorden.length}  ${pct(verloren.length, jijWoorden.length)} van alle Jij-woorden (${pct(verloren.length, eerstGedekt.length)} van de gedekte)`);
  }

  meetSprekers(referentie, beurten);

  // Voorbeelden van fout toegewezen beurten
  const fouten = metWoorden.filter((b) => b.meerderheid !== b.kant);
  if (fouten.length > 0 && aantalFouten > 0) {
    console.log(`\nFout toegewezen beurten (${fouten.length}), de langste eerst:`);
    const zwaarste = [...fouten].sort(
      (a, b) => b.ref_jij + b.ref_anderen - (a.ref_jij + a.ref_anderen),
    );
    for (const b of zwaarste.slice(0, aantalFouten)) {
      const tekst = String(b.tekst).split(/\s+/).slice(0, 10).join(" ");
      console.log(
        `  ${mmss(b.start_ms)}-${mmss(b.eind_ms)} als ${b.kant}, referentie jij ${b.ref_jij} / anderen ${b.ref_anderen}, tekstoverlap jij ${b.tekst_jij} / anderen ${b.tekst_anderen}: "${tekst}"`,
      );
    }
  }
}

// (e) Sprekers binnen Anderen (4b-D). Per referentiewoord van Anderen: valt het midden in een
// beurt van Anderen, en heeft die beurt de juiste spreker na de best passende koppeling?
// Zelfde maat als het example sprekers-meting (woord goed als een van de dekkende beurten
// klopt), maar dan op de beurten van Mondig in plaats van op de segmenten van het model.
function besteKoppeling(matrix, nRef) {
  // matrix[g][r]: woorden van echte spreker r in beurten van gevonden spreker g. Elke echte
  // spreker krijgt hooguit een gevonden spreker; brute force (tot 8 tegen 4 is klein).
  let beste = { score: -1, koppeling: [] };
  const zoek = (g, gebruikt, koppeling, score) => {
    if (g === matrix.length) {
      if (score > beste.score) beste = { score, koppeling: [...koppeling] };
      return;
    }
    koppeling.push(null);
    zoek(g + 1, gebruikt, koppeling, score);
    koppeling.pop();
    for (let r = 0; r < nRef; r++) {
      if (gebruikt & (1 << r)) continue;
      koppeling.push(r);
      zoek(g + 1, gebruikt | (1 << r), koppeling, score + matrix[g][r]);
      koppeling.pop();
    }
  };
  zoek(0, 0, [], 0);
  return beste.koppeling;
}

function meetSprekers(referentie, beurten) {
  const anderenBeurten = beurten.filter((b) => b.kant === "anderen");
  const metNummer = anderenBeurten.filter((b) => b.sprekernummer != null);
  console.log(`\n(e) Sprekers binnen Anderen (woord goed als een dekkende beurt de juiste spreker heeft)`);
  if (metNummer.length === 0) {
    console.log("  geen sprekernummers in de uitvoer (sprekerherkenning uit, geen sprekermodel, of één spreker)");
    return;
  }
  const woorden = referentie.filter((w) => w.kant === "anderen");
  const echt = [...new Set(woorden.map((w) => w.spreker))].sort();
  const gevonden = [...new Set(metNummer.map((b) => b.sprekernummer))].sort((a, b) => a - b);
  const dekkend = woorden.map((w) =>
    anderenBeurten.filter((b) => w.midden_ms >= b.start_ms && w.midden_ms <= b.eind_ms),
  );
  const matrix = gevonden.map(() => echt.map(() => 0));
  woorden.forEach((w, i) => {
    const r = echt.indexOf(w.spreker);
    for (const g of new Set(dekkend[i].filter((b) => b.sprekernummer != null).map((b) => b.sprekernummer))) {
      matrix[gevonden.indexOf(g)][r]++;
    }
  });
  const koppeling = besteKoppeling(matrix, echt.length);
  const echtVan = (nummer) => {
    const k = koppeling[gevonden.indexOf(nummer)];
    return k == null ? null : echt[k];
  };
  let goed = 0;
  let verkeerd = 0;
  let zonderNummer = 0;
  let ongedekt = 0;
  const perEcht = Object.fromEntries(echt.map((s) => [s, [0, 0]]));
  woorden.forEach((w, i) => {
    const d = dekkend[i];
    perEcht[w.spreker][1]++;
    if (d.length === 0) ongedekt++;
    else if (d.some((b) => b.sprekernummer != null && echtVan(b.sprekernummer) === w.spreker)) {
      goed++;
      perEcht[w.spreker][0]++;
    } else if (d.every((b) => b.sprekernummer == null)) zonderNummer++;
    else verkeerd++;
  });
  const n = woorden.length;
  console.log(
    `  koppeling: ${gevonden.map((g) => `Spreker ${g} = ${echtVan(g) ?? "geen"}`).join(", ")}`,
  );
  console.log(`  juiste spreker  ${goed}/${n}  ${pct(goed, n)}`);
  console.log(`  andere spreker  ${verkeerd}/${n}  ${pct(verkeerd, n)}`);
  console.log(`  zonder nummer   ${zonderNummer}/${n}  ${pct(zonderNummer, n)}`);
  console.log(`  niet gedekt     ${ongedekt}/${n}  ${pct(ongedekt, n)}`);
  for (const s of echt) {
    const [g, t] = perEcht[s];
    console.log(`    echte spreker ${s}: juist ${pct(g, t)} (${t} woorden)`);
  }
  const gedekt = n - ongedekt;
  console.log(`  van de gedekte woorden: juiste spreker ${pct(goed, gedekt)}`);
  console.log(
    `  beurten van Anderen: ${anderenBeurten.length}, waarvan met nummer ${metNummer.length}`,
  );
}

// ---------- start ----------

const [opdracht, ...rest] = process.argv.slice(2);
const { vlaggen, los } = leesArgs(rest);
if (opdracht === "bouw") await bouw(vlaggen);
else if (opdracht === "sprekers") await sprekersOpdracht(vlaggen);
else if (opdracht === "meet") meet(vlaggen, los);
else fout("gebruik: ami-vergadering.mjs bouw | sprekers | meet <uitschrijving.json> (zie de kop van het script).");
