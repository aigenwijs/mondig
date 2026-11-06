# Merk Mondig

Woordmerk "mondig." (besluit, 1-11-2026): Sora 600, kleine letters, letter-spacing
-0.045em, de punt in de accentkleur. Groen `#00FF95` op donker, paars `#7100F6` op licht
(groen-regel van de huisstijl), wit op paars.

- `mondig-woordmerk-donker.svg`: lichte letters, groene punt (op donker).
- `mondig-woordmerk-licht.svg`: inktletters, paarse punt (op wit).
- `mondig-woordmerk-wit.svg`: alles wit (op paars of een foto met donkere overlay).
- `mondig-woordmerk-inkt.svg`: alles inkt (eenkleurig drukwerk).
- `mondig-icoon-groen.svg` en `mondig-icoon-zwart.svg`: app-icoon "m." op 1024 px.

Opnieuw maken (letters naar paden, met kerning via HarfBuzz):

    uvx --with fonttools --with uharfbuzz --with brotli python -I scripts/merk/woordmerk.py <sora-variable.woff2> docs/merk

