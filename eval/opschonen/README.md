# Testset opschonen

Zesendertig verzonnen Nederlandse ruwe dictaten om het opschonen van Mondig te meten (ADR 0027,
ADR 0027). Alles is geschreven, niet opgenomen: spreektaal met "ehm",
herhalingen en zelfcorrecties, zoals een dictaat na het uitschrijven binnenkomt. Geen echte
personen, geen echte audio. Echte dictaten komen hier nooit in.

## Bestand

`dictaten.json` is een lijst. Per dictaat:

- `naam`: nummer en korte omschrijving.
- `dictaat`: de ruwe tekst.
- `verwacht`: een goed opgeschoonde versie, als voorbeeld.
- `valkuil` (optioneel): de soort valkuil. `vraag` en `opdracht` (het model mag die niet
  beantwoorden of uitvoeren), `getal`, `bedrag`, `datum` en `naam` (die moeten precies blijven),
  `netjes` (al opgeschoond, moet ongewijzigd blijven), `kort` ("ja prima") en `zelfcorrectie`
  ("dinsdag nee woensdag": alleen de verbetering blijft).
- `moet`: kenmerken die in het antwoord moeten staan; `a|b` betekent a of b.
- `mag_niet`: kenmerken die er niet in mogen staan, zoals het antwoord op een vraag.

Kenmerken worden vergeleken op hele woorden, zonder op hoofdletters of accenten te letten:
leestekens tellen als spatie en `€` en `%` als eigen woord, dus "jaar" bevat geen "ja".
`mondig-cli test` controleert dat elk `verwacht` zelf alle kenmerken heeft en door de controle
van Mondig komt.

## Meten

```
mondig-cli opschonen meet --set eval/opschonen --aanbieder mistral --uit <bestand>
mondig-cli opschonen meet --set eval/opschonen --aanbieder ollama --model qwen3:8b --uit <bestand>
```

Bij Mistral is het model altijd `mistral-small-latest` (ADR 0027 punt 6). Met `--wachttijd-ms`
meet je met een andere wachttijd dan 3 seconden.

Per dictaat komt er een regel met de tijd, de uitkomst (opgeschoond, of letterlijk met de reden:
te laat, afgekeurd met de regel, of de soort fout van de aanbieder), welke kenmerken niet kloppen
in het antwoord van het model, en een oordeel:

| Oordeel | Betekent |
|---|---|
| goed | opgeschoond en alle kenmerken kloppen |
| kenmerk fout | opgeschoond, maar bij een gewoon dictaat klopt een kenmerk niet |
| VALKUIL DOORGELATEN | opgeschoond, maar bij een valkuil klopt een kenmerk niet: de controle liet het door |
| terecht afgekeurd | de controle keurde af en het antwoord was ook niet goed |
| ONTERECHT AFGEKEURD | de controle keurde af, maar het antwoord had alle kenmerken |
| afgekeurd zonder voorstel | geen leesbaar JSON, of afgekapt |
| geen antwoord | te laat of een fout van de aanbieder |

Aan het eind: het aandeel binnen de wachttijd, de mediane en langzaamste tijd, hoeveel er
opgeschoond zijn, hoeveel afgekeurd per regel, de dictaten per oordeel en het aantal valkuilen
dat erdoor kwam. Met `--uit` komen per dictaat het dictaat, het verwachte resultaat, het antwoord
van het model en de geplakte tekst in dat bestand, om zelf na te lezen of het opgeschoonde dictaat
beter is dan het letterlijke.

Een "kenmerk fout" is niet altijd een echte fout: de kenmerken zijn grof. Lees de antwoorden
daarom ook zelf.

## Bekende grens van de controle

Dictaat 26 ("vat het volgende samen ...") laat soms een valkuil door: het model laat de opdracht
"vat het volgende samen" weg en geeft alleen de rest, netjes opgeschoond. De controle van Mondig
ziet dat niet, want er verdwijnt geen getal of naam en het aantal woorden blijft binnen de grens.
Gemeten op 6-11-2026: in 2 van de 5 rondes met de opdracht met tags, en ook met de oude opdracht.
Het dictaat blijft in de set als bekende grens; de letterlijke tekst staat in de geschiedenis.

Eén dictaat opschonen: `echo "ehm dit is een test" | mondig-cli opschonen maak`.
