#!/usr/bin/env node
// Haalt de openbare CI-testset op (subset van Multilingual LibriSpeech, Nederlands)
// en zet die klaar voor `mondig-cli bench`. Geen afhankelijkheden, wel Node 20+ en
// ffmpeg (de bron levert Opus; de basislijn is op wav gemeten, dus zetten we het om).
//
// Gebruik:
//   node eval/scripts/haal-ci-subset.mjs <doelmap>
//       Haalt alle fragmenten uit eval/ci-subset.json op, controleert elke sha256
//       van het opgehaalde (Opus-)bestand en schrijft per fragment <naam>.opus,
//       <naam>.wav (16 kHz mono) en <naam>.txt (referentie) naar de doelmap.
//   node eval/scripts/haal-ci-subset.mjs --maak-manifest
//       Bouwt eval/ci-subset.json opnieuw uit de bron (alleen voor onderhoud).
//
// De doelmap hoort buiten git (of onder eval/audio/, die genegeerd wordt).
// Bij een afwijkende hash stopt het script met een duidelijke fout.

import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const hier = dirname(fileURLToPath(import.meta.url));
const manifestPad = resolve(hier, "..", "ci-subset.json");

const API = "https://datasets-server.huggingface.co/rows";
const DATASET = "facebook/multilingual_librispeech";
const CONFIG = "dutch";
const SPLIT = "test";
const STAP = 38; // elke 38e rij vanaf 0: 81 fragmenten uit 3.075

function fout(tekst) {
  console.error(`Fout: ${tekst}`);
  process.exit(1);
}

function sha256(buffer) {
  return createHash("sha256").update(buffer).digest("hex");
}

// Kleine pauze tussen verzoeken: de datasets-server geeft anders HTTP 429.
const pauze = (ms) => new Promise((klaar) => setTimeout(klaar, ms));

async function haalOp(url, probeer = 8) {
  let laatste;
  for (let poging = 1; poging <= probeer; poging++) {
    await pauze(300);
    try {
      const antwoord = await fetch(url);
      if (antwoord.ok) return antwoord;
      laatste = new Error(`HTTP ${antwoord.status}`);
      // Een vaste fout (404 en dergelijke) heeft geen zin om opnieuw te proberen.
      if (antwoord.status < 500 && antwoord.status !== 429) break;
      const wacht = Number(antwoord.headers.get("retry-after"));
      await pauze(Number.isFinite(wacht) && wacht > 0 ? wacht * 1000 : 5000 * poging);
    } catch (e) {
      laatste = e;
      await pauze(2000 * poging);
    }
  }
  throw new Error(`${url.split("?")[0]} is niet op te halen (${laatste?.message})`);
}

async function haalRij(rij) {
  const url = `${API}?dataset=${encodeURIComponent(DATASET)}&config=${CONFIG}&split=${SPLIT}&offset=${rij}&length=1`;
  const json = await (await haalOp(url)).json();
  const r = json.rows?.[0];
  if (!r || r.row_idx !== rij) throw new Error(`rij ${rij} ontbreekt in het antwoord`);
  return { json, r: r.row };
}

async function haalAudio(r) {
  const bron = r.audio?.[0]?.src;
  if (!bron) throw new Error("geen audio-adres in de rij");
  return Buffer.from(await (await haalOp(bron)).arrayBuffer());
}

function naamVan(rij) {
  return `mls-${String(rij).padStart(4, "0")}`;
}

function naarWav(opus, wav) {
  const uit = spawnSync(
    "ffmpeg",
    ["-v", "error", "-y", "-i", opus, "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", wav],
    { encoding: "utf8" },
  );
  if (uit.error) {
    fout("ffmpeg is nodig om de Opus-bestanden om te zetten, maar is niet te starten. Installeer ffmpeg en probeer opnieuw.");
  }
  if (uit.status !== 0) fout(`ffmpeg faalde voor ${opus}: ${uit.stderr}`);
}

async function maakManifest() {
  const eerste = await haalRij(0);
  const totaal = eerste.json.num_rows_total;
  if (!Number.isInteger(totaal)) fout("het totale aantal rijen is niet te bepalen");
  const fragmenten = [];
  for (let rij = 0; rij < totaal; rij += STAP) {
    const { r } = rij === 0 ? eerste : await haalRij(rij);
    const audio = await haalAudio(r);
    fragmenten.push({
      rij,
      id: r.id,
      tekst: r.transcript,
      duur_s: Math.round(r.audio_duration * 100) / 100,
      sha256: sha256(audio),
    });
    process.stderr.write(`\r${fragmenten.length} fragmenten`);
  }
  process.stderr.write("\n");
  const manifest = {
    beschrijving: "Vaste subset van Multilingual LibriSpeech (Nederlands, test) voor de CI-meetlat van Mondig.",
    bron: {
      dataset: DATASET,
      config: CONFIG,
      split: SPLIT,
      revisie: "2e83e61823b4c47dcbcb1980bb88601274127609",
      licentie: "CC-BY-4.0",
      url: "https://huggingface.co/datasets/facebook/multilingual_librispeech",
      bronvermelding:
        "Multilingual LibriSpeech (Pratap e.a., 2020, arXiv:2012.03411), Nederlandse config, CC BY 4.0. Voorgelezen audioboeken van LibriVox.",
    },
    methode: `Deterministisch: elke ${STAP}e rij van de split vanaf rij 0 (rij 0, ${STAP}, ${STAP * 2}, ...). De sha256 hoort bij het Opus-bestand zoals de datasets-server het levert; het wav-bestand wordt daar lokaal met ffmpeg uit gemaakt (16 kHz mono).`,
    totaal_rijen_split: totaal,
    aantal: fragmenten.length,
    totale_duur_s: Math.round(fragmenten.reduce((som, f) => som + f.duur_s, 0) * 100) / 100,
    fragmenten,
  };
  writeFileSync(manifestPad, JSON.stringify(manifest, null, 2) + "\n");
  console.log(`Manifest geschreven: ${fragmenten.length} fragmenten, ${manifest.totale_duur_s} s.`);
}

async function haalSubset(doelmap) {
  const manifest = JSON.parse(readFileSync(manifestPad, "utf8"));
  mkdirSync(doelmap, { recursive: true });
  let bytes = 0;
  let duur = 0;
  let gedaan = 0;
  for (const f of manifest.fragmenten) {
    const naam = naamVan(f.rij);
    const opusPad = join(doelmap, `${naam}.opus`);
    const wavPad = join(doelmap, `${naam}.wav`);
    const txtPad = join(doelmap, `${naam}.txt`);

    let audio = existsSync(opusPad) ? readFileSync(opusPad) : null;
    if (!audio || sha256(audio) !== f.sha256) {
      const { r } = await haalRij(f.rij);
      if (r.id !== f.id) fout(`rij ${f.rij} heeft id ${r.id}, het manifest verwacht ${f.id}. De bron is veranderd.`);
      audio = await haalAudio(r);
    }
    const hash = sha256(audio);
    if (hash !== f.sha256) {
      fout(`sha256 van ${naam} (rij ${f.rij}) klopt niet.\n  verwacht: ${f.sha256}\n  gekregen: ${hash}\nDe bron is veranderd of de download is beschadigd; er is niets bruikbaars klaargezet voor dit fragment.`);
    }
    writeFileSync(opusPad, audio);
    writeFileSync(txtPad, f.tekst + "\n");
    if (!existsSync(wavPad)) naarWav(opusPad, wavPad);
    bytes += audio.length;
    duur += f.duur_s;
    gedaan++;
    process.stderr.write(`\r${gedaan}/${manifest.fragmenten.length}`);
  }
  process.stderr.write("\n");
  console.log(
    `${gedaan} fragmenten in ${doelmap}, hashes kloppen. Totale duur ${Math.round(duur)} s, Opus-grootte ${(bytes / 1e6).toFixed(1)} MB.`,
  );
}

const arg = process.argv[2];
if (!arg || arg === "--help" || arg === "-h") {
  console.log("Gebruik: node eval/scripts/haal-ci-subset.mjs <doelmap>   (of --maak-manifest)");
} else if (arg === "--maak-manifest") {
  await maakManifest().catch((e) => fout(e.message));
} else {
  await haalSubset(resolve(arg)).catch((e) => fout(e.message));
}
