# Spraakmodellen en licenties

Dit bestand wordt gemaakt door `scripts/model-licenties.mjs` uit
`crates/mondig-core/src/modellen.json`. Pas het niet met de hand aan.

Mondig bevat de spraakmodellen zelf niet. De app laat je ze downloaden van Hugging Face,
in een omgezette vorm (GGUF) van handy-computer. Hieronder staat per model van wie het
oorspronkelijk is, onder welke licentie, en wat er aan is gewijzigd.

## Parakeet Ultra (standaard, snel, 25 Europese talen)

- **Titel:** Moondream Parakeet Ultra
- **Maker:** Moondream
- **Copyright:** de bron geeft geen copyrightregel
- **Oorspronkelijk model:** <https://huggingface.co/moondream/parakeet-ultra>
- **Licentie:** CC-BY-4.0, <https://creativecommons.org/licenses/by/4.0/legalcode.en>
- **Garantiedisclaimer van de licentie:** <https://creativecommons.org/licenses/by/4.0/legalcode.en#s5>
- **Gewijzigd:** Nagetraind door Moondream op parakeet-tdt-0.6b-v3 van NVIDIA (https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3, CC-BY-4.0). Daarna omgezet naar GGUF en gekwantiseerd (Q8_0) door handy-computer; die omzetting hertraint de gewichten niet.
- **Omgezette versie die Mondig downloadt:** <https://huggingface.co/handy-computer/parakeet-ultra-gguf>

Dit materiaal valt onder de licentie CC-BY-4.0 (Creative Commons Naamsvermelding 4.0).
De vermelding volgt artikel 3(a)(1) van die licentie: de maker, de copyrightregel
(voor zover de maker die geeft), de licentie, de garantiedisclaimer en een link naar het
materiaal, plus de aanduiding dat het is gewijzigd.

## Canary 1b-v2 (25 Europese talen)

- **Titel:** Canary 1B v2: Multitask Speech Transcription and Translation Model
- **Maker:** NVIDIA
- **Copyright:** de bron geeft geen copyrightregel
- **Oorspronkelijk model:** <https://huggingface.co/nvidia/canary-1b-v2>
- **Licentie:** CC-BY-4.0, <https://creativecommons.org/licenses/by/4.0/legalcode.en>
- **Garantiedisclaimer van de licentie:** <https://creativecommons.org/licenses/by/4.0/legalcode.en#s5>
- **Gewijzigd:** Omgezet naar GGUF en gekwantiseerd (Q8_0) door handy-computer. De gewichten zijn niet opnieuw getraind.
- **Omgezette versie die Mondig downloadt:** <https://huggingface.co/handy-computer/canary-1b-v2-gguf>

Dit materiaal valt onder de licentie CC-BY-4.0 (Creative Commons Naamsvermelding 4.0).
De vermelding volgt artikel 3(a)(1) van die licentie: de maker, de copyrightregel
(voor zover de maker die geeft), de licentie, de garantiedisclaimer en een link naar het
materiaal, plus de aanduiding dat het is gewijzigd.

## Whisper large-v3 turbo (99 talen)

- **Titel:** whisper-large-v3-turbo
- **Maker:** OpenAI
- **Copyright:** Copyright (c) 2022 OpenAI
- **Oorspronkelijk model:** <https://huggingface.co/openai/whisper-large-v3-turbo>
- **Licentie:** MIT, <https://opensource.org/license/mit>
- **Garantiedisclaimer van de licentie:** <https://opensource.org/license/mit>
- **Gewijzigd:** Omgezet naar GGUF en gekwantiseerd (Q8_0) door handy-computer. De gewichten zijn niet opnieuw getraind.
- **Omgezette versie die Mondig downloadt:** <https://huggingface.co/handy-computer/whisper-large-v3-turbo-gguf>

## Silero VAD v6.2 (stiltedetectie)

- **Titel:** Silero VAD
- **Maker:** Silero Team
- **Copyright:** Copyright (c) 2020-present Silero Team
- **Oorspronkelijk model:** <https://github.com/snakers4/silero-vad>
- **Licentie:** MIT, <https://opensource.org/license/mit>
- **Garantiedisclaimer van de licentie:** <https://opensource.org/license/mit>
- **Gewijzigd:** Omgezet naar GGUF (F32, niet gekwantiseerd) door handy-computer. De gewichten zijn niet opnieuw getraind.
- **Omgezette versie die Mondig downloadt:** <https://huggingface.co/handy-computer/silero-vad-v6.2-gguf>

## Sortformer v2.1 (sprekers herkennen, tot 4 per spoor)

- **Titel:** diar_streaming_sortformer_4spk-v2.1
- **Maker:** NVIDIA
- **Copyright:** de bron geeft geen copyrightregel
- **Oorspronkelijk model:** <https://huggingface.co/nvidia/diar_streaming_sortformer_4spk-v2.1>
- **Licentie:** NVIDIA Open Model License, <https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/>
- **Garantiedisclaimer van de licentie:** <https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/>
- **Gewijzigd:** Licensed by NVIDIA Corporation under the NVIDIA Open Model License. Omgezet naar GGUF en gekwantiseerd (Q8_0) door handy-computer (commit fafaab5 van het oorspronkelijke model). De gewichten zijn niet opnieuw getraind. De licentie (LICENSE) en het Notice-bestand (NOTICE) komen mee in de modelmap.
- **Omgezette versie die Mondig downloadt:** <https://huggingface.co/handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf>

Licensed by NVIDIA Corporation under the NVIDIA Open Model License. De licentie (`LICENSE`)
en het Notice-bestand (`NOTICE`) komen van dezelfde commit mee in de modelmap. Mondig
levert dit model niet mee: wie sprekers wil herkennen, downloadt het zelf (ADR 0018).
