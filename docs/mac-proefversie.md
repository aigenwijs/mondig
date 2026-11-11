# Mondig op de Mac proberen

Dit is een proefversie voor Macs met een Apple-chip (M1 of nieuwer), met macOS 14.2 of nieuwer.
De app is nog niet door Apple gecontroleerd. Daarom waarschuwt macOS bij het eerste openen, en moet je
bij elke nieuwe versie een paar dingen opnieuw toestaan.

## Downloaden

1. Log in op GitHub en open de pagina met de uitgevoerde jobs van de bouw (Actions). Kies de
   laatste geslaagde bouw voor macOS.
2. Download onderaan bij "Artifacts" het bestand `mondig-macos-dmg`. Dat is een zip; pak hem uit.
   Hij blijft zeven dagen beschikbaar.
3. Open het `.dmg`-bestand en sleep Mondig naar Programma's.

## Openen ondanks de waarschuwing

1. Open Mondig uit Programma's. macOS meldt dat de app niet geopend kan worden. Kies "Gereed".
2. Open Systeeminstellingen, dan Privacy en beveiliging. Scrol naar beneden tot de melding over
   Mondig en kies "Toch openen". Voer je wachtwoord in.
3. Dit hoeft per versie maar één keer.

## Rechten opnieuw geven na elke nieuwe versie

Rechten hangen aan de handtekening van de app, en die is bij elke bouw anders. Na het installeren van
een nieuwe versie vraagt macOS de microfoon dus opnieuw. Staat Mondig nog in de lijst bij
Systeeminstellingen, Privacy en beveiliging, Microfoon, maar werkt het niet? Zet het uit en weer aan, of
haal Mondig uit de lijst en start de app opnieuw.

## Wat testen we nu

1. De app start en toont het hoofdvenster. Het icoon staat in de menubalk.
2. Het model downloadt (of staat al klaar) en de app meldt dat het compleet is.
3. Bij het eerste opnemen verschijnt de microfoonvraag, met een Nederlandse tekst (Engelse
   systeemtaal: Engelse tekst). Geef toestemming.
4. Neem een stukje spraak op, laat het uitschrijven en controleer dat de tekst klopt.
5. Schrijf een bestand uit (kies een geluidsbestand van de computer) en bekijk het resultaat.
6. Meld hoe snel het uitschrijven ging, en of de computer veel moeite leek te hebben.

Dicteren in andere programma's, het geluidje, de pil en vergaderingen opnemen werken nog niet; die
horen bij latere rondes. Meld bij een fout wat je deed, wat je zag en voeg een schermafbeelding toe.
