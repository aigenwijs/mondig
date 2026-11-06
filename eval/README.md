# Kwaliteitsmeting

Hier meten we hoe goed de modellen Nederlands uitschrijven (woordfouten, WER) en
hoe snel (keer realtime).

## Regels

- **Geen audio en geen transcripten van echte mensen in git.** De testset staat
  lokaal in `eval/audio/` en `eval/transcripten/`; beide mappen worden genegeerd.
- In git staan alleen: deze beschrijving, `testset.md` (welke opnames, zonder inhoud),
  de scripts en `resultaten.md`.
- Toestemming van iedere spreker vastleggen voordat een opname in de testset komt.

## Testset (fase 0)

Twintig opnames met een handmatig gecorrigeerd transcript, verdeeld over: dictaat,
vergadering, telefoon, regionaal accent, vaktaal en namen, en Engelse termen in
Nederlandse zinnen.

## Meten

Vanaf fase 2 via `mondig-cli bench`. In fase 0 met losse scripts per aanbieder.

```
mondig-cli bench --set <map> [--model <id>]... [--json] [--basislijn <bestand>]                  [--drempel 1.0] [--geen-filter] [--max-bestanden <n>]
```

Een set is een map met audiobestanden en per bestand een `.txt` met dezelfde naam
als referentie. Per model komen er woordfouten (WER, gewogen naar aantal
referentiewoorden), tekenfouten (CER), realtime-factor (verwerkingstijd gedeeld door
audioduur, lager is sneller) en laadtijd. Met `--basislijn` eindigt de opdracht met
exitcode 1 als de WER meer dan `--drempel` procentpunt slechter is. Het filter tegen
verzonnen tekst staat aan, behalve met `--geen-filter`.

De tekst wordt voor het vergelijken genormaliseerd door `mondig_core::meten::normaliseer`
(kleine letters, leestekens en streepjes weg, apostrof in woorden en getallen blijven;
de regels staan in de doc-comment). Wijzig die regels alleen samen met een nieuwe basislijn.

## Openbare CI-set (MLS, Nederlands)

`ci-subset.json` is de vaste lijst van 81 fragmenten (elke 38e rij van de testsplit,
ongeveer 21 minuten) met referentietekst en sha256 per audiobestand. Ophalen:

```
node eval/scripts/haal-ci-subset.mjs <doelmap>
```

Het script haalt de audio via de datasets-server van Hugging Face, controleert elke
sha256 en stopt bij een afwijking. Het zet de Opus-bestanden met ffmpeg om naar
16 kHz mono wav (de basislijn is op wav gemeten; Mondig leest Opus sinds fase 4 ook
zelf, maar de bench kiest de wav als die er is) en schrijft de `.txt`-referenties
ernaast. Zet de doelmap buiten git of onder `eval/audio/` (genegeerd). Ophalen duurt
een kwartier door de snelheidsbegrenzing van de server; de CI cachet de map.

`basislijn.json` bevat de metingen van dit moment op de referentie-pc, met datum,
platform en modelcommit. De merge-drempel (plan hoofdstuk 11) vergelijkt daarmee.
De workflow `.github/workflows/bench.yml` draait alleen Parakeet op Linux; de
Linux-uitkomst kan iets afwijken van Windows, zie de kop van die workflow.

Let op: MLS-referenties zijn in oude spelling ("den haan") en Parakeet is mogelijk
op deze data getraind (zie `ci-testset.md`). De score zegt dus iets over verandering,
niet over hoe goed Mondig dicteert.

### Bronvermelding

- Multilingual LibriSpeech (Pratap e.a., 2020, arXiv:2012.03411), Nederlandse
  config, CC BY 4.0, https://huggingface.co/datasets/facebook/multilingual_librispeech,
  revisie 2e83e61823b4. Voorgelezen audioboeken van LibriVox.

## Vergadermeting (AMI, Engels)

`eval/scripts/ami-vergadering.mjs` meet of `mondig-cli vergader uitschrijven` de beurten
bij de juiste kant zet (microfoon = Jij, geluid van de computer = Anderen) en hoeveel
echo er zonder koptelefoon in Jij komt. Er zijn geen eigen testopnames, dus het script
gebruikt vergadering ES2002a van de openbare AMI Meeting Corpus.

```
node eval/scripts/ami-vergadering.mjs bouw
mondig-cli vergader uitschrijven eval/audio/ami/ES2002a-koptelefoon --json --model canary-1b-v2-q8 --taal en > koptelefoon.json
node eval/scripts/ami-vergadering.mjs meet koptelefoon.json
mondig-cli vergader echo eval/audio/ami/ES2002a-luidspreker --model canary-1b-v2-q8 --taal en
```

`bouw` haalt de vier headsetkanalen en de annotaties op naar `eval/audio/ami/` (met cache),
leest in `meetings.xml` welke spreker bij welk kanaal hoort, en maakt twee opnamemappen
in het formaat van sprint 4b-A. Jij is de spreker met de op een na meeste woorden. In
`ES2002a-koptelefoon` is de systeemtrack de som van de andere drie kanalen; de ankers van
de microfoonstukken staan 100 ms te laat, zoals bij een echte microfoon. In
`ES2002a-luidspreker` komt de systeemtrack ook in de microfoon terecht als echo (120 ms
vertraging, -15 dB, galm van ongeveer 0,3 s; alles instelbaar op de opdrachtregel).
Gebruik `--taal en`: zonder die vlag vertaalt Canary het Engels naar het Nederlands.

`meet` vergelijkt de JSON met de referentiewoorden (alleen `<w>` zonder `punc`; geen
vocalsounds) op drie manieren: per woord (midden binnen een beurt van de juiste kant, met
250 ms speling), per beurt (de kant met de meeste referentiewoorden in het tijdvak van de
beurt; het criterium is 90 procent), en het aandeel Jij-beurten dat vooral Anderen is (echo).

Het uitschrijven laat standaard de uitingen van Jij weg die op Anderen lijken (ontdubbelen).
Voor de maat "echte Jij-woorden die wegvielen" schrijf je dezelfde opname ook uit met
`--geen-ontdubbelen` naar een tweede JSON en geef je die aan `meet` mee:

```
mondig-cli vergader uitschrijven eval/audio/ami/ES2002a-luidspreker --json --model canary-1b-v2-q8 --taal en --geen-ontdubbelen > basis.json
mondig-cli vergader uitschrijven eval/audio/ami/ES2002a-luidspreker --json --model canary-1b-v2-q8 --taal en > ontdubbeld.json
node eval/scripts/ami-vergadering.mjs meet ontdubbeld.json --basis basis.json
```

Uitkomst (maat b2, juiste kant per beurt op tekst; verloren Jij-woorden, van alle 985):
koptelefoon 97,2 procent zonder, 97,1 procent met ontdubbelen, 0,2 procent verloren;
luidspreker 66,1 procent zonder, 85,5 procent met ontdubbelen, 9,9 procent verloren.

De audio en alle uitvoer blijven buiten git: `eval/audio/` wordt genegeerd en de JSON
schrijf je buiten de repo.

### Bronvermelding AMI

- AMI Meeting Corpus (Carletta e.a., 2005), vergadering ES2002a, CC BY 4.0,
  https://groups.inf.ed.ac.uk/ami/corpus/license.shtml. Headsetopnames en handmatige
  woordannotaties (versie 1.6.2) van https://groups.inf.ed.ac.uk/ami/download/.
