# Testset notulen

Drie verzonnen Nederlandse vergaderingen om de notulen van Mondig te meten (ADR 0026).
Alles is geschreven, niet opgenomen: geen echte personen, geen
echte audio. Echte vergaderingen komen hier nooit in.

| Map | Soort | Ongeveer |
|---|---|---|
| `projectoverleg` | gemeentelijk projectoverleg | 10 minuten |
| `teamoverleg` | teamoverleg met veel actiepunten | 30 minuten |
| `verkenning` | verkennend gesprek met weinig besluiten en "we zouden misschien"-zinnen die geen besluit zijn | 60 minuten |

## Bestanden

- `vergadering.txt`: per beurt een regel `mm:ss Naam: tekst` (boven het uur `h:mm:ss`). Regels
  die met `#` beginnen en lege regels tellen niet. Beurt 1 is de eerste beurtregel.
- `verwacht.json`: de echte besluiten en actiepunten met het nummer van de beurt waarin ze gezegd
  worden, en onder `geen_besluit` de twijfelzinnen die geen besluit mogen worden.

## Meten

```
mondig-cli notulen meet --set eval/notulen --aanbieder mistral --model mistral-small-latest --uit <map>
mondig-cli notulen meet --set eval/notulen --aanbieder ollama --model qwen3:8b --uit <map>
```

Per vergadering komt er een regel met: beurten, geschatte tokens, delen, besluiten en actiepunten
gevonden van verwacht (met daarachter de extra punten en hoeveel daarvan de markering "niet
teruggevonden" kregen), twijfelzinnen die toch een besluit werden, `wie` en `wanneer` ingevuld
terwijl dat niet gezegd is, actiepunten waarbij Mondig `wie` of `wanneer` niet in de bronbeurt
terugvond (gemarkeerd), bronnen die niet teruggevonden zijn, herhaalde aanvragen, tijd en
tokens. Met `--uit` komen de notulen per vergadering als tekst in die map, met bij elk punt de bron
en de uitkomst van de controle.

Een verwacht punt telt als gevonden als er een gemaakt punt van dezelfde soort is in dezelfde beurt
of één beurt ernaast (waar de bron is teruggevonden, anders de beurt die het model noemde). Elk
punt telt één keer. Een extra punt is niet per se verzonnen: het kan een echt punt zijn dat niet in
`verwacht.json` staat. Lees de notulen daarom ook zelf na.

Notulen van een echt verslag: `mondig-cli notulen maak --verslag <opnamemap> --aanbieder mistral
--model mistral-small-latest --uit <bestand>`. Op het scherm komen standaard alleen getallen; de
notulen staan in het bestand. Alleen met `--toon-inhoud` komen ze ook op het scherm.
