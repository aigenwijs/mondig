#!/usr/bin/env node
// Schermafbeeldingen voor de UX-meting (nulmeting 4-11-2026).
// Start de frontend met de ontwikkelmock, loopt een vaste set schermen en toestanden langs met
// agent-browser en stopt daarna alles weer. Elke meting gebruikt zo dezelfde schermen, zodat
// metingen met elkaar te vergelijken zijn. Voeg je een scherm toe, zet het dan achteraan
// en noem het in het meetbestand.
//
//   node scripts/ux-schermen.mjs [uitvoermap]    standaard: target/ux-schermen
//
// Nodig: agent-browser (npm i -g agent-browser) en `npm install` in app/.

import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const uit = resolve(process.argv[2] ?? join(root, "target", "ux-schermen"));
// Een ongebruikelijke poort: andere worktrees draaien vaak al een server op 5173 en omstreken.
const poort = Number(process.env.UX_POORT ?? 6847);
const basis = `http://localhost:${String(poort)}`;
const sessie = "mondig-ux-schermen";

// Venster van de app (tauri.conf.json), het kleinste venster en de pil (pil.rs; de pil meet
// zichzelf en wordt met de waarschuwing breder, tot 560).
const VENSTER = [900, 780];
const KLEIN = [480, 400];
const PIL = [560, 60];

// [naam, adres, wachttijd in ms, opties]. Opties: vol (hele pagina), licht, maat, tekst (wacht
// tot deze tekst op het scherm staat, voor toestanden die maar even duren, zoals de pil die
// luistert), klik (namen van knoppen die na elkaar worden aangeklikt, met na elke klik `naKlik`
// ms wachten; een [rol, naam] in plaats van een naam klikt een ander soort element, en
// ['tekst', deel] klikt het element met dat stuk tekst, ['nth', nummer, selector] het
// zoveelste element dat bij de selector past (vanaf 0)), kies ([selector, waarde] paren voor een
// keuzelijst, voor de klikken).
const SCHERMEN = [
  ["01-eerste-start-stap1", "/?mock=snel", 2000],
  [
    "01b-eerste-start-stap2",
    "/?mock=snel",
    2000,
    { klik: ["Verder"], naKlik: 2500 },
  ],
  [
    "01c-eerste-start-stap3",
    "/?mock=traag",
    2000,
    { klik: ["Verder", "Downloaden (1,8 GB)"], naKlik: 2500 },
  ],
  ["02-dicteren-hoofd", "/?mock=compleet"],
  ["03-dicteren-wat-is-nieuw", "/?mock=compleet,nieuw"],
  ["04-dicteren-tijdens-dictaat", "/?mock=compleet,dictaat", 3500],
  // dictaatlang: de pil blijft luisteren. Met het gewone dictaat duurt het luisteren 3,5 s en staat
  // "0:01" maar één seconde in beeld; een trage start van de browser mist dat venster en dan
  // loopt het wachten na 25 s af. "Lokaal" staat er zolang de pil luistert.
  ["06-pil-dictaat", "/pil?mock=dictaatlang", 300, { maat: PIL, tekst: "Lokaal" }],
  [
    "07-pil-waarschuwing",
    "/pil?mock=dictaat&klok=540000",
    300,
    { maat: PIL, tekst: "dan stopt" },
  ],
  ["08-bestanden-leeg", "/bestanden?mock=compleet"],
  ["09-bestanden-wachtrij", "/bestanden?mock=compleet,wachtrij"],
  ["10-bestanden-slepen", "/bestanden?mock=compleet,slepen"],
  ["11-opnemen-start", "/vergadering?mock=compleet"],
  ["12-opnemen-loopt", "/vergadering?mock=compleet,opname"],
  ["13-opnemen-uitschrijven", "/vergadering?mock=compleet,uitschrijven"],
  [
    "14-opnemen-verslag-sprekers",
    "/vergadering?mock=compleet,verslagsprekers",
    1500,
    { vol: true },
  ],
  [
    "15-opnemen-wachtend",
    "/vergadering?mock=compleet,veelwachtend",
    1500,
    { vol: true },
  ],
  ["16-instellingen", "/instellingen?mock=compleet", 1500, { vol: true }],
  ["17-modellen", "/modellen?mock=compleet,drie", 1500, { vol: true }],
  [
    "18-modellen-downloadrij",
    "/modellen?mock=compleet,rij",
    1500,
    { vol: true },
  ],
  ["19-over", "/over?mock=compleet", 1500, { vol: true }],
  [
    "20-opnemen-melden",
    "/vergadering?mock=compleet,startfout",
    1500,
    { klik: ["Opname starten"], naKlik: 1500 },
  ],
  [
    "20b-opnemen-fout-microfoon-bezet",
    "/vergadering?mock=compleet,startfout",
    1500,
    { klik: ["Opname starten", "Ik heb het gemeld, start"], naKlik: 1500 },
  ],
  ["21-licht-dicteren", "/?mock=compleet", 1500, { licht: true }],
  [
    "22-licht-bestanden-wachtrij",
    "/bestanden?mock=compleet,wachtrij",
    1500,
    { licht: true },
  ],
  [
    "23-licht-verslag",
    "/vergadering?mock=compleet,verslagsprekers",
    1500,
    { licht: true },
  ],
  ["24-minimaal-venster-dicteren", "/?mock=compleet", 1500, { maat: KLEIN }],
  [
    "25-minimaal-venster-verslag",
    "/vergadering?mock=compleet,verslag",
    1500,
    { maat: KLEIN },
  ],
  // N-C1: het klaar-moment direct na het uitschrijven, en de lijst met persoonsgegevens.
  [
    "26-opnemen-klaarmoment",
    "/vergadering?mock=compleet,klaarmoment",
    300,
    { tekst: "staat klaar" },
  ],
  [
    "27-verslag-persoonsgegevens",
    "/vergadering?mock=compleet,verslagsprekers",
    1500,
    { klik: ["3 persoonsgegevens"], naKlik: 800 },
  ],
  // N-C1: Dicteren met de microfoontest aan (de microfoon en de meter gaan pas na de klik aan).
  [
    "28-dicteren-microfoontest",
    "/?mock=compleet",
    1500,
    { klik: ["Microfoon testen"], naKlik: 1500 },
  ],
  [
    "29-minimaal-venster-microfoontest",
    "/?mock=compleet",
    1500,
    { maat: KLEIN, klik: ["Microfoon testen"], naKlik: 1500 },
  ],
  // N-C1: Opnemen met veel wachtende opnames op het kleinste venster: de startbalk blijft in beeld.
  [
    "30-minimaal-venster-opnemen-wachtend",
    "/vergadering?mock=compleet,veelwachtend",
    1500,
    { maat: KLEIN },
  ],
  // N-C1 deel 3: Bestanden leeg op het kleinste venster (Map openen is een icoon), Instellingen
  // op het kleinste venster (de bediening onder het label) en de microfoontest in Instellingen.
  [
    "31-minimaal-venster-bestanden-leeg",
    "/bestanden?mock=compleet",
    1500,
    { maat: KLEIN },
  ],
  [
    "32-minimaal-venster-instellingen",
    "/instellingen?mock=compleet",
    1500,
    { maat: KLEIN },
  ],
  [
    "33-instellingen-microfoontest",
    "/instellingen?mock=compleet",
    1500,
    { klik: ["Testen"], naKlik: 1500 },
  ],
  // N-C2: de dictaatgeschiedenis. De lijst met voorbeelden van vandaag en gisteren, de lege
  // staat (eerste keer), Niet bewaren (eerst een dictaat, dan staat het laatste dictaat er) en
  // de nieuwe rij Dictaten bewaren in Instellingen.
  ["34-dicteren-dictaten-lijst", "/?mock=compleet", 1500],
  ["35-dicteren-dictaten-leeg", "/?mock=compleet,dictatenleeg", 1500],
  [
    "36-dicteren-niet-bewaren",
    "/?mock=compleet,dictatenniet,dictaat,snel",
    1500,
    { tekst: "Mondig bewaart je dictaten niet" },
  ],
  [
    "37-instellingen-dictaten-bewaren",
    "/instellingen?mock=compleet",
    1500,
    { vol: true },
  ],
  [
    "38-minimaal-venster-dictaten-lijst",
    "/?mock=compleet",
    1500,
    { maat: KLEIN },
  ],
  // N-C3: verslagen bewaren. Opnemen met Eerder opgenomen (vijf in beeld, Meer tonen, de regel over
  // de bewaartermijn) op beide vensters, met en zonder wachtende opnames, de lijst uitgeklapt, het
  // menu (...) van een regel met de bevestigvraag, een verslag geopend uit de lijst, het menu van het
  // verslag, de titel in bewerking, en Instellingen met de rij Opnames en verslagen wissen na.
  [
    "39-opnemen-eerder-opgenomen",
    "/vergadering?mock=compleet,eerder",
    1500,
    { vol: true },
  ],
  [
    "40-minimaal-venster-opnemen-eerder",
    "/vergadering?mock=compleet,eerder",
    1500,
    { maat: KLEIN },
  ],
  [
    "41-opnemen-eerder-wachtend",
    "/vergadering?mock=compleet,veelwachtend,eerder",
    1500,
    { vol: true },
  ],
  [
    "42-minimaal-venster-opnemen-eerder-wachtend",
    "/vergadering?mock=compleet,driewachtend,eerderweinig",
    1500,
    { maat: KLEIN },
  ],
  [
    "43-opnemen-eerder-alle-tonen",
    "/vergadering?mock=compleet,eerder",
    1500,
    { vol: true, klik: ["Meer tonen"], naKlik: 800 },
  ],
  [
    "44-opnemen-eerder-menu",
    "/vergadering?mock=compleet,eerder",
    1500,
    { vol: true, klik: ["Meer voor Teamoverleg Ruimte en Wonen"], naKlik: 800 },
  ],
  [
    "45-opnemen-eerder-wissen-vraag",
    "/vergadering?mock=compleet,eerder",
    1500,
    {
      vol: true,
      klik: [
        "Meer voor Teamoverleg Ruimte en Wonen",
        ["menuitem", "Verslag verwijderen…"],
      ],
      naKlik: 800,
    },
  ],
  [
    "46-verslag-uit-lijst-geopend",
    "/vergadering?mock=compleet,eerder",
    1500,
    { vol: true, klik: ["Overleg buurtbijeenkomst speelveld"], naKlik: 1500 },
  ],
  [
    "47-verslag-menu-open",
    "/vergadering?mock=compleet,verslagsprekers",
    1500,
    { klik: ["Meer"], naKlik: 800 },
  ],
  [
    "48-verslag-naam-wijzigen",
    "/vergadering?mock=compleet,eerder",
    1500,
    // De knop "Naam wijzigen" bestaat niet meer: de naam maak je met de velden Datum, Klant en
    // Onderwerp bovenaan het verslag. Zelfde schermnaam, zodat de metingen vergelijkbaar blijven.
    { klik: ["Overleg buurtbijeenkomst speelveld"], naKlik: 1500 },
  ],
  [
    "49-instellingen-verslagen-wissen-na",
    "/instellingen?mock=compleet#bewaartermijn",
    1500,
  ],
  [
    "50-minimaal-venster-instellingen-verslagen-wissen-na",
    "/instellingen?mock=compleet#bewaartermijn",
    1500,
    { maat: KLEIN },
  ],
  // N-C3 na de review: een verslag dat niet bewaard kon worden weigert Sluiten, met Toch sluiten.
  [
    "51-verslag-niet-bewaard",
    "/vergadering?mock=compleet,verslagsprekers,bewaarfout",
    1500,
    { klik: ["Sluiten"], naKlik: 800 },
  ],
  // N-C4, opnieuw uitschrijven (ADR 0023): de dialoog, de balk tijdens het lopen, de melding met de
  // gemarkeerde regels, en het grijze menu-item als de opname gewist is.
  [
    "52-verslag-opnieuw-dialoog",
    "/vergadering?mock=compleet,verslagsprekers,snelmodel,verbeterd",
    1500,
    { klik: ["Meer", ["menuitem", "Opnieuw uitschrijven…"]], naKlik: 1200 },
  ],
  [
    "53-verslag-opnieuw-bezig",
    "/vergadering?mock=compleet,verslagsprekers,snelmodel,opnieuwbezig",
    1500,
  ],
  [
    "54-minimaal-venster-verslag-opnieuw-bezig",
    "/vergadering?mock=compleet,verslagsprekers,snelmodel,opnieuwbezig",
    1500,
    { maat: KLEIN },
  ],
  [
    "55-verslag-opnieuw-klaar",
    "/vergadering?mock=compleet,verslagsprekers,snelmodel,verbeterd,opnieuwklaar",
    1500,
    { vol: true },
  ],
  [
    "56-verslag-menu-opname-gewist",
    "/vergadering?mock=compleet,verslagsprekers,gewist",
    1500,
    { klik: ["Meer"], naKlik: 800 },
  ],
  // N-C5, Mondig opnieuw afstellen (Instellingen, groep Deze computer): bezig met de stappen en
  // Stoppen, het advies met vinkjes (ook op het kleinste venster), overgenomen, en Alles staat al
  // goed. De mock heeft een losse kaart, Ultra en geen Canary: dus videokaart en nauwkeurig model.
  [
    "57-instellingen-afstellen-bezig",
    "/instellingen?mock=compleet",
    1500,
    { klik: ["Afstellen"], naKlik: 1200 },
  ],
  [
    "58-instellingen-afstellen-advies",
    "/instellingen?mock=compleet",
    1500,
    { vol: true, klik: ["Afstellen"], naKlik: 5000 },
  ],
  [
    "59-minimaal-venster-instellingen-afstellen-advies",
    "/instellingen?mock=compleet",
    1500,
    { maat: KLEIN, klik: ["Afstellen"], naKlik: 5000 },
  ],
  [
    "60-instellingen-afstellen-overgenomen",
    "/instellingen?mock=compleet",
    1500,
    { vol: true, klik: ["Afstellen", "Advies overnemen"], naKlik: 5000 },
  ],
  [
    "61-instellingen-afstellen-alles-goed",
    "/instellingen?mock=compleet,drie,gpugeen",
    1500,
    { vol: true, klik: ["Afstellen"], naKlik: 3000 },
  ],
  // UX-ronde na de gebruikerstest van N-C.
  ["62-dicteren-eigen-sneltoets", "/?mock=compleet,eigentoets", 1500],
  [
    "63-minimaal-venster-dicteren-eigen-sneltoets",
    "/?mock=compleet,eigentoets",
    1500,
    { maat: KLEIN },
  ],
  [
    "64-minimaal-venster-bestanden-wachtrij",
    "/bestanden?mock=compleet,wachtrij",
    1500,
    { maat: KLEIN },
  ],
  [
    "65-modellen-nieuwe-indeling",
    "/modellen?mock=compleet,drie,sprekermodel",
    1500,
  ],
  [
    "66-modellen-nieuwe-indeling-heel",
    "/modellen?mock=compleet,drie,sprekermodel",
    1500,
    { vol: true },
  ],
  [
    "67-minimaal-venster-modellen",
    "/modellen?mock=compleet,drie",
    1500,
    { maat: KLEIN },
  ],
  [
    "68-modellen-nooit-gecontroleerd",
    "/modellen?mock=compleet,nooitgecontroleerd",
    1500,
  ],
  [
    "69-modellen-wijzigen-dicteren",
    "/modellen?mock=compleet,drie",
    1500,
    { klik: ["Model voor Dicteren wijzigen"], naKlik: 800 },
  ],
  [
    "70-modellen-wijzigen-opnames",
    "/modellen?mock=compleet,drie",
    1500,
    { klik: ["Model voor Opnames en bestanden wijzigen"], naKlik: 800 },
  ],
  ["71-opnemen-wachtend", "/vergadering?mock=compleet,driewachtend", 1500],
  [
    "72-minimaal-venster-opnemen-wachtend",
    "/vergadering?mock=compleet,driewachtend",
    1500,
    { maat: KLEIN },
  ],
  // "Opnieuw controleren" op Modellen start dezelfde ronde als Afstellen in Instellingen.
  [
    "73-modellen-opnieuw-controleren",
    "/modellen?mock=compleet",
    1500,
    { klik: [["link", "Opnieuw controleren"]], naKlik: 5000 },
  ],
  [
    "74-minimaal-venster-modellen-wijzigen",
    "/modellen?mock=compleet,drie",
    1500,
    {
      maat: KLEIN,
      klik: ["Model voor Opnames en bestanden wijzigen"],
      naKlik: 800,
    },
  ],
  [
    "75-modellen-wijzigen-te-weinig-ruimte",
    "/modellen?mock=compleet,krap",
    1500,
    { klik: ["Model voor Opnames en bestanden wijzigen"], naKlik: 800 },
  ],
  // T3, notulen (ADR 0024 tot en met 0026): de groep Notulen in Instellingen (Mistral ingesteld,
  // niets ingesteld, het kleinste venster) en het tabblad Notulen van het verslag: leeg, de
  // vraag waar de tekst heen gaat, bezig, klaar met markeringen, gemaakt bij de vorige versie,
  // de sprong van een bronknop naar het verslag, en het kleinste venster.
  [
    "76-instellingen-notulen-mistral",
    "/instellingen?mock=compleet,notulen#notulen",
    1500,
    { vol: true },
  ],
  [
    "77-instellingen-notulen-niets",
    "/instellingen?mock=compleet#notulen",
    1500,
    { vol: true },
  ],
  [
    "78-minimaal-venster-instellingen-notulen",
    "/instellingen?mock=compleet,notulen#notulen",
    1500,
    { maat: KLEIN, vol: true },
  ],
  [
    "79-verslag-notulen-leeg",
    "/vergadering?mock=compleet,verslag,notulen",
    1500,
    { klik: [["tab", "Notulen"]], naKlik: 800 },
  ],
  [
    "80-verslag-notulen-niet-ingesteld",
    "/vergadering?mock=compleet,verslag",
    1500,
    { klik: [["tab", "Notulen"]], naKlik: 800 },
  ],
  [
    "81-verslag-notulen-vraag",
    "/vergadering?mock=compleet,verslag,notulen",
    1500,
    { klik: [["tab", "Notulen"], "Notulen maken"], naKlik: 800 },
  ],
  [
    "82-verslag-notulen-bezig",
    "/vergadering?mock=compleet,verslag,notulenbezig",
    1500,
    { klik: [["tab", "Notulen"]], naKlik: 800 },
  ],
  [
    "83-verslag-notulen-klaar",
    "/vergadering?mock=compleet,verslag,notulenklaar",
    1500,
    { vol: true, klik: [["tab", "Notulen"]], naKlik: 800 },
  ],
  [
    "84-verslag-notulen-vorige-versie",
    "/vergadering?mock=compleet,verslag,notulenvorige",
    1500,
    { vol: true, klik: [["tab", "Notulen"]], naKlik: 800 },
  ],
  [
    "85-verslag-notulen-bron-sprong",
    "/vergadering?mock=compleet,verslag,notulenklaar",
    1500,
    { klik: [["tab", "Notulen"], "Ga naar 0:09 in het verslag"], naKlik: 600 },
  ],
  [
    "86-minimaal-venster-verslag-notulen-klaar",
    "/vergadering?mock=compleet,verslag,notulenklaar",
    1500,
    { maat: KLEIN, vol: true, klik: [["tab", "Notulen"]], naKlik: 800 },
  ],
  [
    "87-minimaal-venster-verslag-notulen-vraag",
    "/vergadering?mock=compleet,verslag,notulen",
    1500,
    { maat: KLEIN, klik: [["tab", "Notulen"], "Notulen maken"], naKlik: 800 },
  ],
  [
    "88-minimaal-venster-verslag-notulen-bezig",
    "/vergadering?mock=compleet,verslag,notulenbezig",
    1500,
    { maat: KLEIN, klik: [["tab", "Notulen"]], naKlik: 800 },
  ],
  // O2, dictaten opschonen (ADR 0027): de keuzelijst onder Dicteren (met een eigen sneltoets, bij
  // elk dictaat, zonder aanbieder, met Ollama), de vraag waar de tekst heen gaat, Dicteren met de
  // tweede sneltoets, de pil tijdens het opschonen en bij terugval, een geopend opgeschoond
  // dictaat, en het kleinste venster met de instelling.
  [
    "89-instellingen-opschonen-eigen-sneltoets",
    "/instellingen?mock=compleet,notulen,opschonen#opschonen",
    1500,
  ],
  [
    "90-instellingen-opschonen-elk-dictaat",
    "/instellingen?mock=compleet,notulen,opschonenelk#opschonen",
    1500,
  ],
  [
    "91-instellingen-opschonen-zonder-aanbieder",
    "/instellingen?mock=compleet#opschonen",
    1500,
  ],
  [
    "92-instellingen-opschonen-ollama",
    "/instellingen?mock=compleet,notulenollama#opschonen",
    1500,
  ],
  [
    "93-instellingen-opschonen-vraag",
    "/instellingen?mock=compleet,notulen#opschonen",
    1500,
    { kies: [["#opschonen-keuze", "eigenSneltoets"]], naKlik: 800 },
  ],
  ["94-dicteren-tweede-sneltoets", "/?mock=compleet,notulen,opschonen", 1500],
  [
    "95-pil-opschonen",
    "/pil?mock=dictaat,notulen,opschonenelk",
    300,
    { maat: PIL, tekst: "Opschonen" },
  ],
  [
    "96-pil-opschonen-terugval",
    "/pil?mock=dictaat,notulen,opschonenelk,opschoonterugval",
    100,
    { maat: PIL, tekst: "Niet opgeschoond" },
  ],
  [
    "97-dicteren-opgeschoond-dictaat",
    "/?mock=compleet,dictaatopgeschoond",
    1500,
    { klik: [["tekst", "Ik stuur de stukken voor donderdag"]], naKlik: 800 },
  ],
  [
    "98-minimaal-venster-instellingen-opschonen",
    "/instellingen?mock=compleet,notulen,opschonen#opschonen",
    1500,
    { maat: KLEIN },
  ],
  [
    "99-instellingen-stopwoorden",
    "/instellingen?mock=compleet",
    1500,
    // De tweede "Meer opties" is die van Dicteren; de schakelaar staat daarachter. Hele pagina,
    // want de rij staat onder de vouw.
    { klik: [["nth", "1", "button[aria-controls^='meer-']"]], naKlik: 800, vol: true },
  ],
];

// agent-browser rechtstreeks via Node: de npm-shim zou op Windows een shell nodig hebben, en die
// plakt argumenten met een spatie ("Opname starten") aan elkaar.
const agentBrowser = join(
  execFileSync(
    process.platform === "win32" ? "npm.cmd" : "npm",
    ["root", "-g"],
    {
      encoding: "utf8",
      shell: process.platform === "win32",
    },
  ).trim(),
  "agent-browser",
  "bin",
  "agent-browser.js",
);

function browser(...args) {
  // Uitvoer niet opvangen: de eerste aanroep start de browser op de achtergrond, die de pijp
  // openhoudt, en dan zou execFileSync nooit terugkomen.
  execFileSync(process.execPath, [agentBrowser, "--session", sessie, ...args], {
    stdio: ["ignore", "ignore", "inherit"],
  });
}

async function wachtOpServer() {
  for (let i = 0; i < 60; i++) {
    try {
      if ((await fetch(basis)).ok) return;
    } catch {
      // nog niet klaar
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`De ontwikkelserver op ${basis} start niet.`);
}

rmSync(uit, { recursive: true, force: true });
mkdirSync(uit, { recursive: true });

const vite = spawn(
  process.execPath,
  [
    join(root, "app", "node_modules", "vite", "bin", "vite.js"),
    "dev",
    "--port",
    String(poort),
    "--strictPort",
  ],
  { cwd: join(root, "app"), stdio: "ignore" },
);

// UX_ALLEEN=<deel van een naam> loopt alleen de schermen langs waarvan de naam daarmee begint.
const alleen = process.env.UX_ALLEEN ?? "";
const gekozen = SCHERMEN.filter(([naam]) => naam.startsWith(alleen));

let mislukt = 0;
try {
  await wachtOpServer();
  for (const [naam, adres, wacht = 1500, opties = {}] of gekozen) {
    try {
      const [b, h] = opties.maat ?? VENSTER;
      browser("set", "viewport", String(b), String(h));
      browser("set", "media", opties.licht ? "light" : "dark");
      browser("open", basis + adres);
      if (opties.tekst) browser("wait", "--text", opties.tekst);
      browser("wait", String(wacht));
      for (const [selector, waarde] of opties.kies ?? []) {
        browser("select", selector, waarde);
        browser("wait", String(opties.naKlik ?? 1500));
      }
      for (const knop of opties.klik ?? []) {
        // Een tekst is een knop; [rol, naam] kiest een andere rol, zoals een menu-item, en
        // ['tekst', deel] het element met dat stuk tekst.
        const [rol, naam, selector] =
          typeof knop === "string" ? ["button", knop] : knop;
        if (rol === "tekst") browser("find", "text", naam, "click");
        else if (rol === "nth") browser("find", "nth", naam, selector, "click");
        else browser("find", "role", rol, "click", "--name", naam);
        browser("wait", String(opties.naKlik ?? 1500));
      }
      browser(
        "screenshot",
        ...(opties.vol ? ["--full"] : []),
        join(uit, `${naam}.png`),
      );
      console.log(`ok   ${naam}`);
    } catch (fout) {
      mislukt++;
      console.error(`FOUT ${naam}: ${String(fout.message).split("\n")[0]}`);
    }
  }
} finally {
  try {
    browser("close");
  } catch {
    // de browser was al dicht
  }
  vite.kill();
}

console.log(
  `\n${String(gekozen.length - mislukt)} van ${String(gekozen.length)} schermen in ${uit}`,
);
process.exitCode = mislukt ? 1 : 0;
