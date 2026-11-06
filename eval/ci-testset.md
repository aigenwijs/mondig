# Openbare testset voor CI

Onderzocht op 31 oktober 2026. De
privé-testset uit fase 0 draait alleen lokaal; in CI draait een openbare set. Die
meet of een wijziging achteruitgang geeft, niet hoe goed Mondig echt dicteert.

## Keuze

**Primair: Multilingual LibriSpeech (MLS), config `dutch`, split `test`.**
- Licentie CC-BY-4.0, download zonder account (Hugging Face-API: `gated: false`,
  revisie `2e83e61823b4`).
- Test-split: 3.075 fragmenten, 198 MB (datasets-server van Hugging Face).
- Voorgelezen audioboeken van LibriVox, gemiddeld ongeveer 15 seconden per fragment.
- Subset voor CI: ongeveer 80 vaste fragmenten (ongeveer 20 minuten), gekozen op
  id, opgehaald op een vaste revisie met checksumcontrole. Audio niet in git.

**Aanvullend: VoxPopuli, config `nl`.**
- Licentie CC0-1.0 voor de transcripten; de opnames vallen onder de juridische
  kennisgeving van het Europees Parlement (Hugging Face-tags: `cc0-1.0`, `other`;
  revisie `42f01879c780`). Download zonder account.
- Plenaire debatten van het Europees Parlement, een ander genre dan MLS.

**Reserve: FLEURS `nl_nl`** (CC-BY-4.0, korte voorgelezen zinnen).

**Afgevallen:** Common Voice NL (sinds oktober 2025 via Mozilla Data Collective,
voorwaarden niet bevestigd) en het Corpus Gesproken Nederlands (licentie per
gebruiker, niet vrij te verspreiden in CI).

## Kanttekeningen

- Parakeet V3 is getraind op onder meer Multilingual LibriSpeech, Mozilla Common
  Voice v7 en Europarl-ASR (modelkaart nvidia/parakeet-tdt-0.6b-v3, sectie Training
  Dataset, geraadpleegd 31-10-2026). MLS en VoxPopuli zijn dus bekend terrein; de
  scores vallen te mooi uit. Of de test-splits zijn uitgesloten, staat niet in de
  kaart. Voor Whisper is de overlap onbekend.
- Allemaal voorgelezen of voorbereide spraak; spontane spraak, telefoon en
  vergaderingen zitten alleen in de privé-testset.
- Normalisatie: `mondig-cli bench` normaliseert referentie en uitvoer op dezelfde,
  vastgelegde manier (kleine letters, leestekens weg, getallen consequent), anders
  kost een komma al punten. Het precieze formaat van de transcripten per set is nog
  niet bekeken.
- Ruis: meet in fase 0 of 2 hoeveel de score op deze 20 minuten schommelt tussen
  runs en tussen vergelijkbare versies, voordat de drempel van 1 procentpunt hard
  wordt (plan hoofdstuk 11).

## Bronvermelding (in README en THIRD_PARTY_NOTICES zodra de set gebruikt wordt)

- Multilingual LibriSpeech (Pratap e.a., 2020, arXiv:2012.03411), Nederlandse
  config, CC BY 4.0, https://huggingface.co/datasets/facebook/multilingual_librispeech,
  revisie vastleggen. Voorgelezen audioboeken van LibriVox.
- VoxPopuli (Wang e.a., ACL 2021, arXiv:2101.00390). "We acknowledge the European
  Parliament for creating and sharing these materials."

## Bronnen (geraadpleegd 31-10-2026)

- https://huggingface.co/datasets/facebook/multilingual_librispeech
- https://huggingface.co/datasets/facebook/voxpopuli
- https://huggingface.co/datasets/google/fleurs
- https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3
- https://arxiv.org/abs/2012.03411
