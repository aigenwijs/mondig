# Changelog

Alle merkbare wijzigingen staan hier. Opmaak volgens
[Keep a Changelog](https://keepachangelog.com/nl/1.1.0/), versies volgens
[Semantic Versioning](https://semver.org/lang/nl/).

## [Nog niet uitgebracht]

### Toegevoegd

- Stopwoorden weghalen: klanken als "ehm", "uh" en "hmm" gaan uit je dictaten, uitschriften
  van bestanden en opnames, zonder taalmodel en zonder internet. Het staat standaard aan; je zet
  het uit bij Instellingen onder Dicteren, achter Meer opties. Alleen klanken die geen woord
  zijn gaan weg, nooit woorden als "nou" of "dus". Bij Nederlands gaan ook "eh" en "euh"
  weg, bij Engels ook "um", "ah" en "eh". Opende zo'n klank de zin, dan krijgt het volgende
  woord de hoofdletter. Opschonen met een taalmodel ziet de tekst al zonder stopwoorden.
- Dictaten opschonen: een taalmodel maakt van je dictaat lopende zinnen met leestekens, voordat
  Mondig de tekst plakt. Je kiest het bij Instellingen onder Dicteren: Uit (standaard), Met een
  eigen sneltoets (Ctrl+Shift+Spatie geeft dan een opgeschoond dictaat, je gewone sneltoets de
  letterlijke tekst) of Bij elk dictaat. Het werkt met het taalmodel dat je onder Notulen koos.
  De eerste keer vraagt Mondig of je dictaten naar die aanbieder mogen; namen en andere
  persoonsgegevens worden niet vervangen. Tijdens het opschonen toont de dicteerbalk het label
  van de aanbieder, en het kruisje plakt meteen de letterlijke tekst. Duurt het langer dan drie
  seconden of klopt het antwoord niet, dan plakt Mondig de letterlijke tekst. Bij je dictaten
  staat wie het opschoonde, met een knop om de letterlijke tekst te kopiëren.
- Van een verslag maakt Mondig nu notulen: een samenvatting, de besluiten en de actiepunten (wat,
  wie en wanneer). Je kiest zelf waar de tekst heen gaat, bij Instellingen onder Notulen: naar
  Mistral (Europees, met een eigen sleutel die Mondig in de sleutelbos van je computer bewaart) of
  naar Ollama op je eigen computer. Bij Mistral zet je eerst het vinkje dat training uit staat voor
  je sleutel; Mondig kan dat zelf niet controleren en legt uit waarom het ernaar vraagt. In het
  verslag staan de tabbladen Verslag en Notulen. Bij elk punt staat een knop met de tijd waar het
  gezegd is: een klik brengt je naar die plek in het verslag. Mondig controleert elke bron zelf.
  Een punt dat niet in het verslag staat, blijft staan met een oranje markering, zodat jij kunt
  beslissen. Je kunt elk punt aanpassen of weghalen, de notulen kopiëren en opnieuw laten maken.
  Opslaan en Kopiëren zetten de notulen voor het verslag. Schrijf je het verslag opnieuw uit, dan
  zegt Mondig dat de notulen bij de vorige versie horen.
- Onder Instellingen kun je Mondig opnieuw laten afstellen: na een minuut kijken (met de stappen
  zichtbaar en een knop Stoppen) toont Mondig wat beter past bij je computer als lijst met vinkjes,
  bijvoorbeeld de videokaart gebruiken of het nauwkeurige model voor opnames en bestanden. Er
  begint niets tot je op "Advies overnemen" klikt; een nieuw model wordt pas gebruikt als de
  download klaar is. Is er niets te verbeteren, dan zegt Mondig dat het alles al goed staat.
- Het menu van het systeemvak heeft nu "Opname starten", met dezelfde soort als de vorige keer.
  Bij Online en In de ruimte opent Mondig eerst de vraag "Vertel het de anderen", bij Alleen ik
  begint de opname meteen. Tijdens een opname staat bovenaan "Opname loopt" met de tijd en
  daaronder "Stoppen en uitschrijven".
- Een verslag kun je opnieuw laten uitschrijven met een nauwkeuriger model, via het menu (⋯) van het
  verslag of van een verslag onder Eerder opgenomen. Sprekers, de namen die je gaf en wat je zelf
  markeerde blijven; Mondig zegt vooraf hoe lang het ongeveer duurt en hoeveel regels je zelf
  verbeterde. Tijdens het uitschrijven kun je het verslag lezen, kopiëren en opslaan, en met
  Stoppen blijft alles zoals het was. Daarna zie je welke regels veranderden, en met "Vorige versie
  terugzetten" ga je terug tot je het verslag sluit. Is de opname al gewist, dan staat de keuze
  grijs met de reden.
- Een verslag blijft nu bewaard als je het sluit. Onder Opnemen staat "Eerder opgenomen" met je laatste
  verslagen: klik om er een te openen, geef het een eigen naam, of wis het. Mondig bewaart verslag en
  opname samen, standaard 7 dagen. Onder Instellingen heet die keuze nu "Opnames en verslagen wissen na".
- Dicteren toont nu je dictaten van de afgelopen 24 uur, per dag, met Kopiëren en Verwijderen,
  zodat je tekst terugvindt als het plakken niet lukte. Onder Instellingen kies je bij Dictaten
  bewaren: niet bewaren, 24 uur of 7 dagen. Ze staan alleen op deze computer en verdwijnen vanzelf
  na de gekozen tijd.
- Eerste start: wie nog geen spraakmodel heeft, krijgt een korte uitleg waarom dat nodig is
  (het uitschrijven gebeurt op je eigen computer), hoe groot de download is, hoeveel schijfruimte
  er vrij is en hoe lang het ongeveer duurt. Mondig kijkt naar je computer en raadt een model aan,
  apart voor dicteren en voor bestanden; het tweede model mag je overslaan. De videokaart raadt
  Mondig alleen aan als een korte proef laat zien dat hij sneller is. Onder Instellingen kun je de
  computer opnieuw laten checken.
- Onder Instellingen, bij Problemen oplossen: een logboek met alleen gebeurtenissen en fouten (nooit
  wat je dicteert, bestandsnamen of namen van sprekers) dat je zelf kopieert of opent, en Mondig
  terugzetten: alle instellingen naar de standaard, zonder je modellen te wissen. Er gaat niets
  vanzelf weg.
- Foutmeldingen in gewone taal per soort, elk met één vervolgactie: geluid dat niet te gebruiken is,
  te weinig videogeheugen, te weinig werkgeheugen, een ontbrekend model en een volle schijf.
- Eerste versie van de app: een startscherm in het Nederlands en een pictogram in de
  systeembalk. Dicteren en bestanden uitschrijven volgen later.
- Nieuwe spraakmotor: nauwkeuriger en sneller, en hij kan de videokaart gebruiken. Drie
  modellen om uit te kiezen: Parakeet (standaard, snel), Canary (het nauwkeurigst) en
  Whisper (99 talen). Eerder gedownloade modellen moeten opnieuw worden gedownload.
- De Windows-installer neemt alles mee wat Mondig nodig heeft, zodat het ook werkt op een pc
  waar dat nog niet op staat. Bij een update stopt de installer eerst wat van Mondig nog
  draait.
- Nieuw startscherm met drie hoofdacties: Dicteren, Bestand uitschrijven en Vergadering
  opnemen. Alleen Dicteren doet al iets; de andere twee volgen later.
- Het spraakmodel downloaden in de app zelf: je ziet vooraf hoe groot het is, hoeveel
  ruimte je vrij hebt en waar het vandaan komt, en tijdens het downloaden het
  percentage en de resterende tijd. Je kunt stoppen en later verder downloaden.
- Een proef: kies een opname of een video (mp4 of mov) en Mondig schrijft hem lokaal
  uit, zonder internet. De tekst kun je kopiëren; hij blijft alleen in het geheugen
  en verdwijnt bij afsluiten.
- De spraakengine start pas als hij nodig is en stopt na vijf minuten zonder gebruik,
  zodat Mondig in rust weinig geheugen gebruikt.
- Dicteren met een sneltoets: houd hem ingedrukt of tik kort, en de tekst verschijnt in
  Mondig. Stilte aan het begin en eind wordt weggeknipt. Hoort Mondig niets, dan zie je
  een melding.
- Instellingen: de microfoon kiezen en testen, de sneltoets wijzigen (combinaties die je
  in andere programma's nodig hebt, zoals Ctrl+C, worden geweigerd) en instellen hoe lang
  het spraakmodel in het geheugen blijft.
- Als sneltoets kun je ook een losse toets kiezen, zoals Insert, Scroll Lock of
  F13 tot en met F24. F1 tot en met F12 kunnen na een waarschuwing, want andere
  programma's gebruiken ze. Letters, cijfers en de pijltjes blijven geweigerd.
- Een nieuwe vormgeving met een zijbalk en het woordmerk "mondig.", met een nieuw
  app-pictogram.
- Na een dictaat plakt Mondig de tekst waar je typt, met een spatie erachter. Het
  klembord dat je had, komt daarna terug. De tekst komt niet in de klembordgeschiedenis
  (Win+V) en niet in de cloud. Lukt plakken niet, bijvoorbeeld in een venster met
  beheerrechten, dan zie je een melding en kopieer je de tekst met één klik. Het laatste
  dictaat kun je ook vanuit de systeembalk kopiëren.
- Instelling "Alleen op het klembord zetten", voor programma's waarin plakken niet werkt.
- De dicteerbalk: een zwevend balkje tijdens het dicteren, onderaan (of bovenaan) het scherm
  waar je werkt. Je ziet dat Mondig luistert, met een meter en de duur, en kunt stoppen of
  annuleren. Daarna "Geplakt", of een melding met één knop als er iets misging. De
  dicteerbalk neemt nooit de plek over waar je typt en is te volgen met een schermlezer.
  Na negen minuten waarschuwt hij dat het dicteren bijna stopt.
- Een kort, zacht geluidje bij starten en stoppen, met aan/uit en volume.
- Instelling "Starten bij aanmelden" (standaard uit): Mondig wacht dan in de systeembalk.
- Blokkeert Windows de microfoon, dan legt Mondig uit wat je moet doen, met een knop naar
  de juiste Windows-instelling.
- Het scherm "Over Mondig", te openen onderaan de zijbalk, onderaan Instellingen en via het
  menu in de systeembalk. Het toont de versie met bouw en datum (met een knop om de
  versiegegevens te kopiëren voor een supportvraag), wat er nieuw is, van wie de
  spraakmodellen zijn en onder welke licentie, en een doorzoekbare lijst van onderdelen van
  anderen met hun licenties. Na een update laat Mondig één keer zien wat er nieuw is. Dit
  werkt allemaal zonder internet.
- Meer bestanden lezen: Opus (zoals WhatsApp-spraakberichten en browseropnames) en het
  geluid uit video's in mkv en webm, naast mp4 en mov. Alles gebeurt lokaal, zonder
  extra programma. Kan Mondig een bestand niet lezen (bijvoorbeeld avi of wmv), dan zegt
  het welke formaten wel werken.
- Het scherm Bestanden: sleep opnames of video's op het venster of kies ze, en Mondig
  schrijft ze na elkaar uit, op deze pc. Per bestand zie je de duur, het percentage en de
  resterende tijd, en je kunt annuleren of een bestand uit de rij halen. Een bestand dat
  Mondig niet kan lezen, staat in de rij met wat je kunt doen. Een klaar bestand open je
  met tijdcodes, de duur, de datum en het model, en kopieer je met één knop. Dicteren
  gaat voor.
- Transcripten van bestanden bewaren: één knop "Opslaan als Word" slaat op in je eigen
  map (standaard Documenten\Mondig); onder het pijltje kies je PDF voor archief (PDF/A),
  LibreOffice, platte tekst of ondertitels (SRT en VTT), of een andere plek. Datum, bron,
  model, label en "Lokaal verwerkt" staan in het document zelf en in de
  documenteigenschappen. Een bestaand bestand wordt nooit stil overschreven: je kiest
  "Opslaan als (2)", Vervangen of Annuleren. Na het opslaan open je het bestand of de map
  met één klik, en Verwijderen haalt het transcript weg met de bestanden die Mondig zelf
  opsloeg (alleen als ze sindsdien niet gewijzigd zijn).
- Instellingen, groep Bestanden: de map, de bestandsnaam uit blokjes (datum, naam van de
  opname, label, tijd, model) met een voorbeeld, het standaardformaat, tijden in het
  document aan of uit, en automatisch opslaan na het uitschrijven (standaard uit).
- Vergaderingen opnemen en uitschrijven, in het nieuwe scherm Vergadering. Kies online (je
  microfoon en het geluid van de computer, apart) of een fysieke vergadering (alleen de
  microfoon).
  Vóór elke start herinnert Mondig je eraan dat je de anderen vertelt dat je opneemt. Na
  afloop schrijft Mondig alles uit op je eigen computer, en maak je er een verslag van met
  Jij en Anderen, waaraan je een naam kunt geven. Het verslag kun je kopiëren of opslaan
  als Word of een ander formaat; de opname zelf wordt gewist, tenzij je hem eerst bewaart.
  Een opname die niet goed is afgesloten kun je herstellen, en een opname kun je ook later
  uitschrijven. Een vergadering wordt uitgeschreven met het model dat je voor bestanden
  gebruikt.
- Opnames van vergaderingen die nog op je computer staan (later uitschrijven, een verslag
  dat openstond bij het afsluiten, of een crash) wist Mondig vanzelf na 7 dagen. In
  Instellingen kies je 1, 7 of 30 dagen. Een dag van tevoren staat er bij de opname dat hij
  binnenkort wordt gewist. Een verslag of audio die je zelf hebt opgeslagen blijft staan.
- Een pagina Modellen in de zijbalk. Bovenaan zie je welk model je gebruikt om te dicteren en
  welk voor bestanden. Daaronder staan de modellen in groepen, met per model voor welke taak
  het past (aanbevolen, sneller of nauwkeuriger), de grootte, de talen en hoe snel het is.
  Je downloadt een model met voortgang en kunt stoppen en later verdergaan, zet het in
  gebruik voor dicteren of voor bestanden, en haalt het weg met een bevestiging. Wat je
  gebruikt, verwijder je niet. Er loopt één download tegelijk, en voor een download vraagt
  Mondig 20 procent ruimte extra op de schijf. Sluit je Mondig terwijl een model binnenkomt,
  dan vraagt Mondig eerst of je dat zeker weet. De voortgang staat ook in de zijbalk, op elke
  pagina. Een nieuw model krijgt de badge "Nieuw" tot je de pagina opent. Instellingen heeft
  één rij die naar de pagina verwijst.
- De pagina Modellen deelt de modellen in op wat ze doen (nu: spraak naar tekst) en toont per
  model twee balkjes, Nauwkeurig en Snel, met uitleg bij het (i)-icoon. Een download laat een
  duidelijke voortgangsbalk zien, in de kaart en in de zijbalk.
- Een eigen model per taak: dicteren gebruikt Parakeet Ultra, bestanden uitschrijven gebruikt
  Canary als je dat downloadt (anders hetzelfde model als dicteren). Whisper turbo kun je
  alleen voor bestanden kiezen, want hij is te traag om mee te dicteren.
- Instelling "Geminimaliseerd starten", onder "Starten bij aanmelden": Mondig start dan in
  het systeemvak, zonder venster. Zet je "Starten bij aanmelden" aan en had je dit nog nooit
  zelf ingesteld, dan gaat het mee aan. Is het spraakmodel nog niet gedownload, dan opent
  het venster altijd.
- Bij Instellingen zie je naast de microfoon meteen een niveaumeter. De microfoon staat
  alleen aan zolang je dat scherm ziet; de aparte test "Microfoon testen" is vervallen.
- De onderdelen van de bestandsnaam kun je slepen, bijvoorbeeld om de datum achter de naam
  te zetten. Zonder muis kan het ook: met Alt en de pijltjes links of rechts schuif je het
  onderdeel op, en een schermlezer meldt de nieuwe plek. Bestaande instellingen blijven
  hetzelfde.
- Een derde soort opname: "Alleen ik", om je gedachten in te spreken. Mondig neemt alleen de
  microfoon op en schrijft het na afloop uit als alinea's. Er komt geen herinnering om het
  aan anderen te vertellen, en het verslag, het opgeslagen bestand en de bewaarde opname
  heten "Notitie" in plaats van "Vergadering".
- Op het scherm Opnemen zie je naast de microfoon en "Geluid van" meteen een niveaumeter,
  en met "Testgeluid afspelen" hoor je of het juiste apparaat is gekozen. De meters stoppen
  zodra je de opname start of een ander scherm opent.
- Bestanden heeft rechtsboven een knop "Map openen": die opent de map waarin Mondig je
  uitschriften bewaart in de Verkenner. Bestaat de map nog niet, dan maakt Mondig hem aan.
  Dezelfde knop staat nu ook boven de opnames die nog wachten, in het menu van het
  pictogram in het systeemvak, en als "Openen" naast de map in Instellingen.
- Nieuwe instelling onder Opslaan, "Meer opties": "Opname ook bewaren als geluidsbestand".
  Staat die aan, dan zet Mondig bij het opslaan van een verslag de opname als WAV ernaast,
  met dezelfde naam. Standaard staat hij uit en wordt de opname gewist.
- Videokaart gebruiken: een nieuwe keuze in Instellingen (Spraakmodellen, Meer opties),
  standaard uit. Met een losse videokaart schrijft Mondig veel sneller uit; gaat er iets mis,
  dan gebruikt Mondig vanzelf de processor. Je ziet wat er werkelijk gebruikt wordt en welke
  videokaart Mondig vond. Zonder geschikte videokaart is de keuze uitgegrijsd. Met "Snelheid
  testen" zie je of aanzetten op jouw computer loont.
- Sprekers herkennen in een vergaderverslag: met het
  aparte model Sortformer (139 MB, alleen voor wie het downloadt) splitst Mondig online
  "Anderen" in Spreker 1, Spreker 2 en zo verder, en bij een fysieke vergadering de
  microfoon. Ook als iemand midden in een zin het woord overneemt. Een spreker geef je één
  keer een naam, een beurt geef je aan een andere spreker en twee sprekers voeg je samen.
  Zonder het model, met de nieuwe instelling "Sprekers herkennen" uit of met maar één
  spreker blijft het verslag zoals het was. De licentie van NVIDIA en het Notice-bestand
  staan bij het model en op Over Mondig.
- Sprekers herkennen, nu ook in de app. In het verslag staat bij elke beurt "Spreker 1",
  "Spreker 2" of de naam die je gaf; klik op een naam om hem te veranderen voor die hele
  spreker, om de beurt aan een andere of nieuwe spreker te geven, of om twee sprekers samen
  te voegen. Tijdens het uitschrijven zie je de stap "Sprekers herkennen". De schakelaar
  "Sprekers herkennen" staat bij Instellingen (Opslaan) en op Bestanden. Aanzetten zonder
  het model vraagt eerst of Mondig het mag downloaden (139 MB) en gaat pas aan als het er is;
  weigeren laat hem uit. Op de pagina Modellen kun je het model ook los downloaden en
  verwijderen. Ook bij een bestand worden de sprekers herkend: in het transcript, bij Kopiëren
  en in elke export (tekst, Word, LibreOffice, pdf en ondertitels) staat dan de spreker voor
  de tekst. Zonder sprekers blijft alles zoals het was.
- Een map in de gaten houden (Instellingen, "Map in de gaten houden"): zet er opnames in, zoals
  een Teams-opname, een dictafoon of een telefoonopname, en Mondig schrijft ze vanzelf uit zodra
  het kopiëren klaar is. Dat werkt zolang Mondig draait, ook als alleen het pictogram in de
  systeembalk zichtbaar is. De opname zelf blijft zoals hij was. Het transcript komt in je
  map voor transcripten, in je standaardformaat en met je naamregel, of op verzoek naast de
  opname. Wat al uitgeschreven is, onthoudt Mondig ook na een herstart. Standaard kijkt Mondig
  niet in submappen, slaat het opnames over waar al een transcript van is, en laat het wat er bij
  het aanzetten al in de map staat liggen. Een opname waar al een transcript van is, staat in de lijst als overgeslagen.
- De wachtrij bij Bestanden toont hoe lang alles samen nog duurt ("Nog ongeveer 12 minuten
  voor 4 bestanden"), zodra Mondig een paar seconden heeft kunnen meten.
- Mondig houdt de computer wakker zolang je een vergadering opneemt, dicteert of er bestanden
  worden uitgeschreven, zodat de slaapstand het werk niet afbreekt. Het scherm mag gewoon uit.
  Alleen op Windows.
- Downloads van modellen zijn stabieler: valt de verbinding weg of blijft ze een halve minuut
  stil, dan probeert Mondig het zelf tot vijf keer opnieuw en gaat verder waar het bleef. Pas
  daarna zie je een melding met "Opnieuw proberen". Een download blijft nooit eindeloos hangen.
- Je kunt meerdere modellen tegelijk laten downloaden: ze komen na elkaar binnen, in een rij.
  Elk model kun je apart stoppen. Alle downloads staan in de zijbalk boven Modellen, met
  voortgang of hun stand (wacht, opnieuw, klaar, mislukt). Werd Mondig afgesloten tijdens een
  download, dan staat hij de volgende keer als "Onderbroken" klaar om te hervatten.
- Na een klik op Downloaden zie je meteen dat er iets gebeurt ("Wordt gestart…"), daarna de
  voortgang op dezelfde plek en in de zijbalk. Bij "Sprekers herkennen" springt de schakelaar
  na de download zichtbaar aan, met een korte bevestiging eronder.
- Het verslag verbeteren, beluisteren en doorzoeken, bij Opnemen en bij een transcript op
  Bestanden. Klik op een regel (of druk op Enter) om de tekst te verbeteren: Enter bewaart en
  gaat naar de volgende regel, Shift+Enter begint een nieuwe regel, Esc maakt ongedaan, en
  een leeg veld houdt de oude tekst. Kopiëren en alle exports (tekst, Word, LibreOffice, pdf
  en ondertitels) gebruiken de verbeterde tekst. Klik op de tijd van een regel om die regel te
  beluisteren, zolang de opname of het bronbestand er nog is; spatie speelt of pauzeert, en de
  snelheid (1, 1,5 of 2 keer) blijft bewaard. Met Ctrl+F zoek je in het verslag: de treffers
  zijn gemarkeerd, Enter en Shift+Enter gaan naar de volgende en vorige, en Esc sluit.
  Het geluid blijft in Mondig zelf en gaat nooit naar buiten.
- Persoonsgegevens markeren en vervangen, bij Opnemen en bij een transcript op Bestanden. Mondig
  markeert gegevens met een vaste vorm, lokaal en zonder taalmodel: BSN (met elfproef), IBAN (met
  controle), telefoonnummers, e-mailadressen, postcode of straat met huisnummer, datums en
  kentekens, ook als de spraakherkenning cijfers in losse groepen schrijft. Gewone getallen,
  bedragen, jaartallen en tijden worden niet gemarkeerd. Je kunt een markering weghalen of zelf
  een stuk tekst markeren, bijvoorbeeld een naam. Met de schakelaar "Persoonsgegevens vervangen
  bij Kopiëren en Opslaan" komt overal [BSN], [IBAN], [TELEFOON], [E-MAIL], [ADRES], [DATUM] of
  [KENTEKEN] in de plaats; er wordt geen sleutellijst bewaard. Mondig vindt niet alles: lees het
  verslag altijd zelf na.
- Exporteren naar Markdown (.md) en een webpagina (.html), bij Opslaan en onder "Ander formaat".
  De webpagina is zelfstandig: de opmaak zit in het bestand, er worden geen lettertypen, scripts of
  plaatjes van buiten geladen en alle tekst is veilig opgemaakt. Beide formaten werken met en zonder
  tijden, met en zonder sprekers en met "Persoonsgegevens vervangen".

### Gewijzigd

- Mondig controleert de beveiligde verbinding voor het downloaden van modellen nu met de
  certificaten van je computer. Daardoor werkt downloaden ook bij organisaties die het verkeer
  controleren met een eigen certificaat, zoals veel gemeenten.
- Modellen begint met "Wat Mondig gebruikt": een kaart voor Dicteren, voor Opnames en bestanden en voor Sprekers herkennen, met het snelle of nauwkeurige model in gebruik en Wijzigen. Wijzigen toont alleen de modellen die bij die taak passen. Heeft Mondig je computer bekeken (bij de eerste start of met "Mondig opnieuw afstellen"), dan staat "Past bij deze computer" bij het model dat het beste past, met de datum en "Opnieuw controleren". Daaronder staan de modellen als compacte kaarten, op je computer en te downloaden, met een rand om wat in gebruik is.
- Het venster opent hoger (780 in plaats van 640). Past dat niet op je scherm, dan wordt het zo hoog als past en staat het in het midden.
- Opnemen: "Opname starten" staat direct onder de microfoon en het geluid, vóór de lijsten. Een standaardapparaat heet nu bij zijn naam met "(standaard)" erachter, en onder "Nog uit te schrijven" staat na hoeveel dagen Mondig de opnames wist.
- Dicteren: "Standaard herstellen" staat klein naast Wijzigen, alleen als je een eigen sneltoets koos. De knop "Probeer met een opname" is weg; een opname uitschrijven doe je bij Bestanden.
- Bestanden: "Sprekers herkennen" is een korte regel met schakelaar onder "Bestanden kiezen", de grootte van het sprekermodel staat er alleen bij zolang het nog niet gedownload is. "Map openen" heet nu "Uitschriften openen", en Mondig zegt in welke map het uitschrift komt.
- Dicteren begint met je sneltoets in grote toetsen, met de naam van je microfoon eronder. De microfoon staat pas aan als je op "Microfoon testen" klikt en gaat weer uit bij "Klaar", na 15 seconden stilte of als je naar een ander scherm gaat. Het lampje van Windows brandt dus niet meer zodra je Dicteren opent. De microfoon kiezen kan onder Instellingen. "Wat is er nieuw" is nu één regel, en het proefveld zit achter "Probeer het hier".
- Bestanden zonder opnames begint met een vlak waar je opnames of video's naartoe sleept: streepjes die eindigen in de punt van het woordmerk, en een groene knop "Bestanden kiezen". Hang je een bestand boven het venster, dan kleurt het vlak, bewegen de streepjes (niet als Windows op minder beweging staat) en staat er "Laat los, dan begint Mondig". De regel met de bestandsformaten is weg. In een klein venster is "Map openen" een icoon.
- Instellingen begint met "Deze computer": "Mondig opnieuw afstellen" (de computercheck, eerst onder Spraakmodellen bij Meer opties) en een rij "Modellen" met de knop "Naar Modellen". De microfoon kiezen kan hier met een knop "Testen": de meter loopt pas na een klik en stopt als je erop klikt, na 15 seconden stilte of als je naar een ander scherm gaat. In een klein venster staat de bediening onder de uitleg, op volle breedte.
- Voor alles wat weggaat staat nu hetzelfde prullenbakje, ook bij wachtende en klaar staande bestanden. Het kruisje betekent alleen nog sluiten.
- Een uitgeschakelde knop blijft leesbaar in plaats van half doorzichtig, bijvoorbeeld "Verwijderen" bij een model dat in gebruik is.
- De bouwcode staat niet meer in de kop van Over Mondig maar onderaan de pagina.
- "Wat is er nieuw" op Dicteren staat op één regel; de hele zin staat in de tooltip.
- Opnemen: de soorten heten korter (Online, In de ruimte, Alleen ik) en passen ook in een klein venster. "Opname starten" staat in een groene balk die altijd in beeld blijft, ook als er veel opnames wachten. Alleen opnemen is rood: de stip en de knop "Stoppen en uitschrijven". Het geluid staat overal als streepjes, tijdens de opname twintig per bron. Bij het uitschrijven staat naast het percentage hoelang het nog ongeveer duurt.
- Het verslag na een opname laat de tekst eerst zien: bovenaan Sluiten en Nieuwe opname, de titel op één regel met Kopiëren, Opslaan en een menu, één regel met de gegevens, en Zoeken en Snelheid in één balk. Persoonsgegevens staan nu op één regel: de lijst opent bij de teller, de schakelaar zegt wat er gebeurt, en in de stand Vervangen ziet ook het verslag zelf [BSN] en zo. Direct na het uitschrijven zegt een korte melding dat je verslag klaar staat.
- Grotere tekst en grotere knoppen, beter zichtbare randen van velden en schakelaars, en overal dezelfde notatie voor percentages (38%) en tijden (12:36, 1:02:40). Lange apparaatnamen in een keuzelijst eindigen op "…".
- Duidelijkere taal: de namen van de spraakmodellen staan alleen nog bij Modellen, "Wat is er nieuw" is korter, en een melding over een bestand dat Mondig niet kan lezen zegt nu wat je kunt doen. Bij een bezette microfoon kun je het meteen opnieuw proberen.
- Uitschrijven van bestanden en opnames gaat op de achtergrond, zodat dicteren altijd voorgaat. Lange opnames worden in stukken verwerkt, zodat een uur geluid niet al je werkgeheugen gebruikt.
- Het tabblad Vergadering heet nu "Opnemen". De soort kies je met een schakelaar met drie
  standen, met eronder één regel uitleg. "Opname starten" staat boven de opnames die nog
  wachten, zodat de knop altijd in beeld is. Die opnames staan elk op één regel, hooguit tien
  tegelijk ("Alle tonen" voor de rest), en alleen die van de gekozen soort; wachten er ook
  opnames van een andere soort, dan staat dat eronder, met een waarschuwing als er een
  binnenkort gewist wordt. Verwijderen doe je met de prullenbak achter de regel; tijdens de
  opname zit Weggooien in een menu (⋯). Beide vragen altijd om bevestiging.
- De soort "In de ruimte" heet nu "Fysieke vergadering".
- "Opname bewaren" begint in dezelfde map als je verslagen en stelt dezelfde naam voor als
  het verslag krijgt.
- Het pictogram in het systeemvak zegt tijdens een opname wat er wordt opgenomen: een
  vergadering, een fysieke vergadering of een notitie. Het tabblad Opnemen heeft een eigen
  pictogram.
- Instellingen heeft vijf groepen: Dicteren, Opslaan, Opstarten, Spraakmodellen en onderaan
  Over Mondig. De sneltoets staat bovenaan bij Dicteren, Bestanden en Vergadering zijn
  samengevoegd onder Opslaan, en wat je zelden verandert zit per groep achter "Meer opties".
  De sectie Geavanceerd is er niet meer; alle instellingen zijn er nog.
- Mondig ziet er rustiger uit: kopjes in gewone letters, de uitleg bij een instelling als
  korte grijze regel onder de naam (in plaats van achter een (i)), geen gekleurde rand meer
  langs meldingen, en duur, model en "lokaal" in een verslag als gewone tekst. Sprekers in
  een verslag zijn een vette naam. Bij Modellen staat niet meer bij elk model dat het lokaal
  is. De zijbalk toont "door Aigenwijs" onder de naam en heeft een voet van één regel met
  Lokaal en de versie. Bestanden heeft een rustiger lege staat.
- Het scherm Dicteren is opnieuw opgebouwd: bovenaan de sneltoets met een knop "Wijzigen",
  dan de microfoon met een meter (dezelfde keuze als in Instellingen), en een proefveld
  waarin je met de sneltoets kunt dicteren. Daaronder staat het laatste dictaat met de tijd
  en "Kopiëren". De link "Probeer met een opname" staat nu bij het proefveld.
- "Spraakmodel in het geheugen houden" heet nu "Model in het geheugen houden" en heeft vier
  keuzes: 5 minuten (standaard), 15 minuten, 1 uur en Altijd (voorheen "Altijd klaar"). De
  keuzes meteen, 2 en 10 minuten zijn vervallen. Het geldt voor dicteren; het model voor
  bestanden is alleen geladen terwijl het uitschrijft.
- Nieuw standaardmodel: Parakeet Ultra, met minder fouten in het Nederlands. Het oude
  Parakeet v3 is vervangen en staat niet meer in Mondig. Had je het al gedownload, dan ruimt
  Mondig die download bij het starten vanzelf op (ongeveer 740 MB) en dicteer je daarna met
  Parakeet Ultra, dat Mondig dan één keer vraagt te downloaden.
- Spraakmodellen staan nu in `AppData\Local` in plaats van `AppData\Roaming`, zodat ze
  bij een meereizend Windows-profiel niet bij elke aanmelding mee gekopieerd worden.

### Opgelost

- De eerste start toont bij het downloaden de voortgang van alles samen ("0,6 van 1,8 GB") in plaats van van het ene model dat binnenkomt ("10 van 742 MB"), zodat het klopt met de knop "Downloaden (1,8 GB)".
- Na een update kon een oude spraakengine naast de nieuwe app blijven staan, met als gevolg
  een melding als "Onbekend model". De installer stopt de spraakengine nu ook bij het
  verwijderen, laat je een bestand dat in gebruik is niet meer overslaan, en Mondig
  controleert bij het starten of app en spraakengine bij elkaar horen. Zo niet, dan zie je
  "Mondig is niet volledig bijgewerkt. Sluit Mondig af en installeer de nieuwste versie
  opnieuw."
- Kies je bij installeren of verwijderen voor Annuleren als de installer vraagt Mondig te
  sluiten, dan blijft Mondig nu gewoon werken. Eerder was de spraakengine dan al gestopt.
- "Versiegegevens kopiëren" noemt nu ook de versie van de spraakengine, zodat support kan
  zien of app en spraakengine bij elkaar horen.
