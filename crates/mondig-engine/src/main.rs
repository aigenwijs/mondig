//! Het werkproces van Mondig: laadt een spraakmodel en schrijft audio uit (ADR 0009).
//!
//! Dit programma wordt gestart door de supervisor in `mondig-core` en praat via stdin
//! en stdout in JSON-regels (zie `mondig_core::engine::protocol`). Alles wat de
//! native bibliotheken (ggml) schrijven gaat naar stderr. Er wordt
//! nooit transcripttekst gelogd.

mod motor;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mondig_core::audio::{self, Stukken};
use mondig_core::engine::protocol::{Antwoord, Apparaat, FoutSoort, Segment, Woord};
use mondig_core::filter::{FilterInstellingen, Meting};
use mondig_core::model;
use mondig_core::spoor::{Spoorlezer, uitingen};
use mondig_engine::Annulering;
use motor::Knip;

/// De echte werker: houdt hooguit één spraakmodel geladen. Het sprekermodel komt er alleen
/// tijdens het herkennen van sprekers bij.
struct Echt {
    geladen: Option<motor::Motor>,
    /// De map van het geladen spraakmodel; het sprekermodel staat ernaast.
    modelmap: Option<PathBuf>,
}

fn fout(soort: FoutSoort, melding: impl Into<String>) -> Antwoord {
    Antwoord::Fout {
        soort,
        melding: melding.into(),
    }
}

impl Echt {
    fn laden(
        &mut self,
        model_id: &str,
        modelmap: &Path,
        apparaat: Apparaat,
        taal: Option<&str>,
    ) -> Result<(), (FoutSoort, String)> {
        let ontbreekt = |melding: String| (FoutSoort::ModelOntbreekt, melding);
        let model = model::zoek(model_id).map_err(|f| ontbreekt(f.to_string()))?;
        // Een hulpmodel als Silero schrijft niets uit; laden als spraakmodel is een fout in het verzoek.
        if !model.is_spraakmodel() {
            return Err((
                FoutSoort::Verzoek,
                format!("{} is geen spraakmodel.", model.naam),
            ));
        }
        // Ontbrekende bestanden zijn geen probleem van de videokaart: aparte soort, zodat de
        // supervisor niet nutteloos op de processor opnieuw probeert.
        if let Some(bestand) = model
            .bestanden
            .iter()
            .find(|b| !modelmap.join(&b.pad).is_file())
        {
            return Err(ontbreekt(format!(
                "Het model {} is niet (helemaal) aanwezig: het bestand {} ontbreekt. Download het model opnieuw.",
                model.naam, bestand.pad
            )));
        }
        // Eerst het oude model loslaten, zodat twee modellen nooit tegelijk in het geheugen staan.
        self.geladen = None;
        self.modelmap = None;

        let bestand = model
            .bestanden
            .first()
            .ok_or_else(|| ontbreekt(format!("Het model {} heeft geen bestanden.", model.naam)))?;
        // Silero staat naast de map van het spraakmodel (zie `model::hulpmodelmap_naast`). Dat
        // is het eenvoudigste: het protocol hoeft geen extra pad te dragen en een test kan
        // met een tijdelijke map werken. Ontbreekt het, dan knippen we niet.
        let vad_pad = vad_bestand(modelmap);
        let motor = motor::laad(
            model.engine,
            &modelmap.join(&bestand.pad),
            apparaat,
            taal,
            vad_pad.as_deref(),
        )?;
        eprintln!("[mondig-engine] model geladen: {model_id} ({apparaat:?})");
        self.geladen = Some(motor);
        self.modelmap = Some(modelmap.to_path_buf());
        Ok(())
    }
}

impl Echt {
    /// Staan het spraakmodel (geladen) en het sprekermodel klaar? Anders het foutantwoord.
    fn sprekers_gereed(&self) -> Result<(), Antwoord> {
        let (Some(_), Some(modelmap)) = (self.geladen.as_ref(), self.modelmap.as_deref()) else {
            return Err(geen_model());
        };
        if model::sprekermodel_naast(modelmap).is_none() {
            return Err(fout(FoutSoort::ModelOntbreekt, GEEN_SPREKERMODEL));
        }
        Ok(())
    }

    /// Draait het sprekermodel op geluid (16 kHz mono) dat al in het geheugen staat. Het
    /// spraakmodel moet geladen zijn en het sprekermodel naast het spraakmodel staan.
    fn sprekers_in(&self, samples: &[f32], annulering: &Annulering) -> Antwoord {
        let (Some(geladen), Some(modelmap)) = (self.geladen.as_ref(), self.modelmap.as_deref())
        else {
            return geen_model();
        };
        let Some(pad) = model::sprekermodel_naast(modelmap) else {
            return fout(FoutSoort::ModelOntbreekt, GEEN_SPREKERMODEL);
        };
        match geladen.herken_sprekers(&pad, samples, annulering.token()) {
            Ok(segmenten) => Antwoord::Sprekers { segmenten },
            Err(_) if annulering.is_geannuleerd() => Antwoord::Geannuleerd,
            Err((soort, melding)) => fout(soort, melding),
        }
    }
}

/// Het Silero-bestand naast de modelmap, als het er helemaal staat (juiste grootte; de
/// hash is bij het downloaden al gecontroleerd). Een half of ontbrekend bestand geeft `None`.
fn vad_bestand(modelmap: &Path) -> Option<PathBuf> {
    let silero = model::zoek(model::SILERO_MODEL).ok()?;
    let bestand = silero.bestanden.first()?;
    let pad = model::hulpmodelmap_naast(modelmap, &silero.id).join(&bestand.pad);
    let compleet = std::fs::metadata(&pad).is_ok_and(|m| m.len() == bestand.grootte);
    compleet.then_some(pad)
}

impl mondig_engine::Werker for Echt {
    fn laad(
        &mut self,
        model_id: &str,
        modelmap: &Path,
        apparaat: Apparaat,
        taal: Option<&str>,
    ) -> Antwoord {
        match self.laden(model_id, modelmap, apparaat, taal) {
            Ok(()) => Antwoord::Geladen,
            Err((soort, melding)) => fout(soort, melding),
        }
    }

    fn transcribeer(&mut self, pad: &Path, annulering: &Annulering) -> Antwoord {
        let Some(geladen) = self.geladen.as_mut() else {
            return geen_model();
        };
        let stukken = match Stukken::open(pad) {
            Ok(stukken) => stukken,
            Err(f) => return fout(FoutSoort::Audio, f.to_string()),
        };
        geladen.zet_annulering(annulering.token());
        schrijf_bestand_uit(geladen, pad, stukken, annulering)
    }

    fn transcribeer_spoor(&mut self, map: &Path, annulering: &Annulering) -> Antwoord {
        let Some(geladen) = self.geladen.as_mut() else {
            return geen_model();
        };
        // Geen stille terugval: zonder Silero zijn er geen uitingen, en dus geen tijden om
        // de twee sporen mee in elkaar te vlechten.
        if !geladen.heeft_vad() {
            return fout(FoutSoort::ModelOntbreekt, GEEN_SILERO);
        }
        let lezer = match Spoorlezer::open(map) {
            Ok(lezer) => lezer,
            Err(f) => return fout(FoutSoort::Audio, f.to_string()),
        };
        geladen.zet_annulering(annulering.token());
        schrijf_spoor_uit(geladen, lezer, annulering)
    }

    fn herken_sprekers(&mut self, map: &Path, annulering: &Annulering) -> Antwoord {
        // Eerst de modellen: zonder spraakmodel of sprekermodel is lezen zinloos.
        if let Err(antwoord) = self.sprekers_gereed() {
            return antwoord;
        }
        let lezer = match Spoorlezer::open(map) {
            Ok(lezer) => lezer,
            Err(f) => return fout(FoutSoort::Audio, f.to_string()),
        };
        let samples = match lees_hele_spoor(lezer, annulering) {
            Ok(Some(samples)) => samples,
            Ok(None) => return Antwoord::Geannuleerd,
            Err(antwoord) => return antwoord,
        };
        self.sprekers_in(&samples, annulering)
    }

    fn herken_sprekers_bestand(&mut self, pad: &Path, annulering: &Annulering) -> Antwoord {
        if let Err(antwoord) = self.sprekers_gereed() {
            return antwoord;
        }
        let stukken = match Stukken::open(pad) {
            Ok(stukken) => stukken,
            Err(f) => return fout(FoutSoort::Audio, f.to_string()),
        };
        let samples = match lees_heel_bestand(stukken, annulering) {
            Ok(Some(samples)) => samples,
            Ok(None) => return Antwoord::Geannuleerd,
            Err(antwoord) => return antwoord,
        };
        self.sprekers_in(&samples, annulering)
    }

    fn apparaten(&mut self) -> Antwoord {
        match motor::videokaarten() {
            Ok(videokaarten) => Antwoord::Apparaten { videokaarten },
            Err((soort, melding)) => fout(soort, melding),
        }
    }

    fn transcribeer_geluid(&mut self, samples: &[f32], knip_stilte: bool) -> Antwoord {
        let Some(geladen) = self.geladen.as_mut() else {
            return geen_model();
        };
        let duur_ms =
            u64::try_from(audio::duur_van_samples(samples.len()).as_millis()).unwrap_or(u64::MAX);
        schrijf_uit(geladen, samples, duur_ms, knip_stilte)
    }
}

fn geen_model() -> Antwoord {
    fout(FoutSoort::NietGeladen, "Er is nog geen model geladen.")
}

/// Schrijft een bestand stuk voor stuk uit (ADR 0015, fase 4): zo staat er nooit meer dan
/// ongeveer één stuk in het geheugen, komt er na elk stuk een voortgang, en kan er
/// tussen twee stukken (en via het token ook midden in een stuk) geannuleerd worden.
///
/// Een stuk met digitale stilte gaat niet naar het model: daar valt niets te horen en
/// het model zou alleen iets kunnen verzinnen. De meting in het resultaat is die van
/// het hele bestand, zodat het filter in de app werkt zoals bij een kort bestand.
fn schrijf_bestand_uit(
    geladen: &mut motor::Motor,
    pad: &Path,
    mut stukken: Stukken,
    annulering: &Annulering,
) -> Antwoord {
    let ms = |d: std::time::Duration| u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
    // Zegt de container de duur niet (een webm uit een browser, een mp3 zonder kop), dan
    // telt het werkproces haar eerst, zodat er een percentage is. Dat kost een extra keer
    // decoderen, maar hier en niet in de app (ADR 0009), en het valt te annuleren.
    let totaal_ms = match stukken.duur_volgens_container() {
        Some(duur) => Some(ms(duur)),
        None => match tel_duur(pad, annulering) {
            Ok(Some(duur)) => Some(ms(duur)),
            Ok(None) => return Antwoord::Geannuleerd,
            Err(f) => return fout(FoutSoort::Audio, f.to_string()),
        },
    };
    let stilte = FilterInstellingen::standaard();
    let mut meting = Meting::opbouw();
    let mut segmenten = Vec::new();
    let mut klaar_ms = 0;
    loop {
        if annulering.is_geannuleerd() {
            return Antwoord::Geannuleerd;
        }
        let stuk = match stukken.volgende() {
            Ok(Some(stuk)) => stuk,
            Ok(None) => break,
            Err(f) => return fout(FoutSoort::Audio, f.to_string()),
        };
        meting.voeg_toe(&stuk.samples);
        klaar_ms = stuk.eind_ms();
        if !Meting::van_samples(&stuk.samples).is_stil(&stilte) {
            let duur_ms = klaar_ms - stuk.begin_ms();
            match geladen.transcribeer(&stuk.samples, duur_ms) {
                Ok(nieuw) => segmenten.extend(verschuif(nieuw, stuk.begin_ms())),
                // transcribe.cpp brak de berekening af omdat het token afging.
                Err(_) if annulering.is_geannuleerd() => return Antwoord::Geannuleerd,
                Err(f) => {
                    return fout(
                        FoutSoort::Transcriberen,
                        format!("Het uitschrijven is mislukt: {f}"),
                    );
                }
            }
        }
        // Is stdout dicht, dan is de supervisor weg: stoppen.
        if mondig_engine::stuur(&Antwoord::Voortgang {
            klaar_ms,
            totaal_ms,
        })
        .is_err()
        {
            return Antwoord::Geannuleerd;
        }
    }
    Antwoord::Resultaat {
        waarschuwing: stukken.waarschuwing(),
        segmenten,
        duur_ms: klaar_ms,
        meting: meting.klaar(),
    }
}

/// De melding als het sprekermodel niet naast het spraakmodel staat.
const GEEN_SPREKERMODEL: &str = "Voor het herkennen van sprekers is een apart model nodig, maar dat staat niet (helemaal) op deze computer. Download het model voor sprekerherkenning.";

/// Zo lang mag een opname hooguit zijn om er sprekers in te herkennen (vier uur). Sortformer
/// krijgt de hele opname in het geheugen: een uur is 230 MB, en een te lange opname zou het
/// werkproces laten crashen, waarna de supervisor de rest van de wachtrij op de processor zet.
const MAX_SPREKERDUUR_MS: u64 = 4 * 3_600_000;

/// Past een opname van deze lengte binnen de grens voor sprekerherkenning?
fn binnen_sprekergrens(duur_ms: u64) -> bool {
    duur_ms <= MAX_SPREKERDUUR_MS
}

/// Het antwoord als een opname te lang is. Van de soort Sprekermodel: geen nieuwe poging op de
/// processor, de tekst is al uitgeschreven en het verslag komt zonder sprekers.
fn te_lang_voor_sprekers() -> Antwoord {
    fout(
        FoutSoort::Sprekermodel,
        "Deze opname is te lang om sprekers te herkennen (meer dan 4 uur); de tekst is wel uitgeschreven.",
    )
}

/// Leest een heel spoor als één stroom (zoals [`schrijf_spoor_uit`], zonder de gaten tussen
/// de ankers), voor de sprekerherkenning: Sortformer kijkt naar de hele opname tegelijk. Een
/// uur is 230 MB aan samples. Na elke minuut komt een voortgang, zodat de supervisor weet dat
/// het werkproces leeft. `Ok(None)` na een annulering; `Err` is het foutantwoord.
fn lees_hele_spoor(
    mut lezer: Spoorlezer,
    annulering: &Annulering,
) -> Result<Option<Vec<f32>>, Antwoord> {
    let lengte_ms = lezer.tijdlijn().stroom_ms();
    if !binnen_sprekergrens(lengte_ms) {
        return Err(te_lang_voor_sprekers());
    }
    let blok = uitingen::samples_in(uitingen::VENSTER);
    let mut samples =
        Vec::with_capacity(usize::try_from(lezer.tijdlijn().totaal_samples()).unwrap_or(0));
    loop {
        if annulering.is_geannuleerd() {
            return Ok(None);
        }
        let nieuw = lezer
            .lees(blok)
            .map_err(|f| fout(FoutSoort::Audio, f.to_string()))?;
        if nieuw.is_empty() {
            return Ok(Some(samples));
        }
        samples.extend_from_slice(&nieuw);
        let klaar_ms = samples_naar_ms(samples.len());
        if mondig_engine::stuur(&Antwoord::Voortgang {
            klaar_ms,
            totaal_ms: Some(lengte_ms),
        })
        .is_err()
        {
            return Ok(None);
        }
    }
}

/// Leest een heel bestand achter elkaar tot samples (16 kHz mono), voor de sprekerherkenning:
/// Sortformer kijkt naar het hele bestand tegelijk. Na elk stuk komt een voortgang, zodat de
/// supervisor weet dat het werkproces leeft. `Ok(None)` na een annulering; `Err` is het
/// foutantwoord.
fn lees_heel_bestand(
    mut stukken: Stukken,
    annulering: &Annulering,
) -> Result<Option<Vec<f32>>, Antwoord> {
    let totaal_ms = stukken
        .duur_volgens_container()
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    if totaal_ms.is_some_and(|ms| !binnen_sprekergrens(ms)) {
        return Err(te_lang_voor_sprekers());
    }
    // Met de bekende duur ineens genoeg ruimte, zodat de Vec niet verdubbelt (en piekt).
    let max_samples = usize::try_from(MAX_SPREKERDUUR_MS * 16).unwrap_or(usize::MAX);
    let mut samples: Vec<f32> = Vec::with_capacity(
        totaal_ms
            .and_then(|ms| usize::try_from(ms * 16).ok())
            .unwrap_or(0)
            .min(max_samples),
    );
    loop {
        if annulering.is_geannuleerd() {
            return Ok(None);
        }
        match stukken.volgende() {
            Ok(Some(stuk)) => {
                samples.extend_from_slice(&stuk.samples);
                // De container zei de duur niet, of loog: ook dan de grens bewaken.
                if samples.len() > max_samples {
                    return Err(te_lang_voor_sprekers());
                }
            }
            Ok(None) => return Ok(Some(samples)),
            Err(f) => return Err(fout(FoutSoort::Audio, f.to_string())),
        }
        if mondig_engine::stuur(&Antwoord::Voortgang {
            klaar_ms: samples_naar_ms(samples.len()),
            totaal_ms,
        })
        .is_err()
        {
            return Ok(None);
        }
    }
}

/// De melding als Silero niet klaarstaat bij een vergadering.
const GEEN_SILERO: &str = "Voor het uitschrijven van een vergadering is de stiltedetectie nodig, maar die staat niet klaar. Download het model voor de stiltedetectie opnieuw.";

/// Schrijft een spoor van een vergadering uit per uiting (sprint 4b-B): Silero zoekt in
/// vensters van een minuut de spraakgebieden, elk gebied gaat apart naar het model en wordt
/// één segment met de tijden van Silero, in milliseconden van de stroom (de app rekent ze
/// om naar de tijdlijn van de opname, na het filter). Een gebied dat over de
/// rand van een venster loopt, gaat mee naar het volgende venster (zie
/// [`uitingen::verdeel`]); er staat dus nooit veel meer dan een venster in het geheugen.
///
/// Na elk venster komt een voortgang in milliseconden van het spoor zelf. De meting in het
/// resultaat is die van het hele spoor, op dezelfde stroomtijden; de duur is de lengte van
/// de stroom.
fn schrijf_spoor_uit(
    geladen: &mut motor::Motor,
    mut lezer: Spoorlezer,
    annulering: &Annulering,
) -> Antwoord {
    let lengte_ms = lezer.tijdlijn().stroom_ms();
    let venster = uitingen::samples_in(uitingen::VENSTER);
    let stroom_ms = |positie: u64| {
        u64::try_from(mondig_core::spoor::stukken::samples_naar_ms(positie)).unwrap_or(0)
    };
    let mut meting = Meting::opbouw();
    let mut segmenten = Vec::new();
    let mut buffer: Vec<f32> = Vec::with_capacity(venster);
    // Waar het venster begint in de stroom.
    let mut begin = 0u64;
    loop {
        if annulering.is_geannuleerd() {
            return Antwoord::Geannuleerd;
        }
        let nieuw = match lezer.lees(venster - buffer.len()) {
            Ok(nieuw) => nieuw,
            Err(f) => return fout(FoutSoort::Audio, f.to_string()),
        };
        meting.voeg_toe(&nieuw);
        buffer.extend_from_slice(&nieuw);
        let einde = buffer.len() < venster;
        if buffer.is_empty() {
            break;
        }
        let spraak = match geladen.spraakgebieden(&buffer) {
            Ok(spraak) => spraak,
            Err(f) => {
                return fout(
                    FoutSoort::Transcriberen,
                    format!("Het zoeken naar spraak is mislukt: {f}"),
                );
            }
        };
        let verdeling = uitingen::verdeel(&buffer, &spraak, einde);
        for bereik in verdeling.klaar {
            if annulering.is_geannuleerd() {
                return Antwoord::Geannuleerd;
            }
            let deel = &buffer[bereik.clone()];
            let gebied_ms = stroom_ms(begin + bereik.start as u64);
            let (tekst, woorden) = match geladen.transcribeer(deel, samples_naar_ms(deel.len())) {
                Ok(uit) => (
                    uit.iter()
                        .map(|s| s.tekst.trim())
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                    // De woordtijden van het model zijn tijden in het gebied; in de stroom
                    // zijn ze de tijden plus het begin van het gebied.
                    uit.into_iter()
                        .flat_map(|s| s.woorden)
                        .map(|w| verschuif_woord(w, gebied_ms))
                        .collect::<Vec<_>>(),
                ),
                Err(_) if annulering.is_geannuleerd() => return Antwoord::Geannuleerd,
                Err(f) => {
                    return fout(
                        FoutSoort::Transcriberen,
                        format!("Het uitschrijven is mislukt: {f}"),
                    );
                }
            };
            if tekst.is_empty() {
                continue;
            }
            segmenten.push(Segment {
                woorden,
                ..Segment::nieuw(gebied_ms, stroom_ms(begin + bereik.end as u64), tekst)
            });
        }
        buffer.drain(..verdeling.verder_vanaf);
        begin += verdeling.verder_vanaf as u64;
        let klaar_ms =
            u64::try_from(mondig_core::spoor::stukken::samples_naar_ms(begin)).unwrap_or(u64::MAX);
        if mondig_engine::stuur(&Antwoord::Voortgang {
            klaar_ms,
            totaal_ms: Some(lengte_ms),
        })
        .is_err()
        {
            return Antwoord::Geannuleerd;
        }
        if einde {
            break;
        }
    }
    Antwoord::Resultaat {
        waarschuwing: None,
        segmenten,
        duur_ms: lengte_ms,
        meting: meting.klaar(),
    }
}

/// Telt de duur van een bestand door het te decoderen. Stuurt elke twee seconden een
/// voortgang zonder totaal, zodat de supervisor weet dat het werkproces nog leeft, en stopt
/// bij een annulering (dan `None`).
fn tel_duur(
    pad: &Path,
    annulering: &Annulering,
) -> Result<Option<std::time::Duration>, mondig_core::audio::AudioFout> {
    let laatste = std::cell::Cell::new(std::time::Instant::now());
    audio::duur_door_te_tellen(pad, &|| {
        if laatste.get().elapsed() >= std::time::Duration::from_secs(2) {
            laatste.set(std::time::Instant::now());
            let teken = Antwoord::Voortgang {
                klaar_ms: 0,
                totaal_ms: None,
            };
            if mondig_engine::stuur(&teken).is_err() {
                return true;
            }
        }
        annulering.is_geannuleerd()
    })
}

/// De gedeelde route voor bestand en geluid: meten, stilte afhandelen, het model draaien.
/// Met `knip_stilte` krijgt het model alleen het stuk met spraak (plus een kleine marge);
/// de tijdcodes in het resultaat blijven die van de hele opname.
fn schrijf_uit(
    geladen: &mut motor::Motor,
    samples: &[f32],
    duur_ms: u64,
    knip_stilte: bool,
) -> Antwoord {
    // Meten doen we hier, waar de samples toch al in het geheugen staan; de app hoeft
    // de audio dan niet zelf te decoderen. De meting is die van de hele opname, ook als
    // we knippen: het filter in de app rekent met tijdcodes van de hele opname.
    let meting = Meting::van_samples(samples);
    if let Some(stil) = mondig_engine::antwoord_bij_stilte(duur_ms, &meting) {
        return stil;
    }

    let (deel, begin_ms) = if knip_stilte {
        match geladen.knip(samples) {
            Knip::Bereik(bereik) => {
                let begin_ms = samples_naar_ms(bereik.start);
                (&samples[bereik], begin_ms)
            }
            // Geen spraak: een resultaat zonder segmenten. Het filter in de app maakt
            // daar "geen tekst herkend" van, in plaats van dat het model iets verzint.
            Knip::GeenSpraak => {
                return Antwoord::Resultaat {
                    waarschuwing: None,
                    segmenten: Vec::new(),
                    duur_ms,
                    meting,
                };
            }
            Knip::NietBeschikbaar => (samples, 0),
        }
    } else {
        (samples, 0)
    };
    let deel_duur_ms =
        u64::try_from(audio::duur_van_samples(deel.len()).as_millis()).unwrap_or(u64::MAX);

    match geladen.transcribeer(deel, deel_duur_ms) {
        Ok(segmenten) => Antwoord::Resultaat {
            waarschuwing: None,
            segmenten: verschuif(segmenten, begin_ms),
            duur_ms,
            meting,
        },
        Err(f) => fout(
            FoutSoort::Transcriberen,
            format!("Het uitschrijven is mislukt: {f}"),
        ),
    }
}

/// Aantal milliseconden dat bij een aantal samples (16 kHz) hoort.
fn samples_naar_ms(samples: usize) -> u64 {
    u64::try_from(audio::duur_van_samples(samples).as_millis()).unwrap_or(u64::MAX)
}

/// Zet de tijdcodes van een uitgeknipt stuk terug naar die van de hele opname, ook die van
/// de woorden.
fn verschuif(segmenten: Vec<Segment>, begin_ms: u64) -> Vec<Segment> {
    segmenten
        .into_iter()
        .map(|s| Segment {
            start_ms: s.start_ms.saturating_add(begin_ms),
            eind_ms: s.eind_ms.saturating_add(begin_ms),
            tekst: s.tekst,
            woorden: s
                .woorden
                .into_iter()
                .map(|w| verschuif_woord(w, begin_ms))
                .collect(),
        })
        .collect()
}

/// Een woord met `begin_ms` later.
fn verschuif_woord(woord: Woord, begin_ms: u64) -> Woord {
    Woord {
        start_ms: woord.start_ms.saturating_add(begin_ms),
        eind_ms: woord.eind_ms.saturating_add(begin_ms),
        tekst: woord.tekst,
    }
}

fn main() -> ExitCode {
    let mut werker = Echt {
        geladen: None,
        modelmap: None,
    };
    match mondig_engine::draai(&mut werker) {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => {
            eprintln!("[mondig-engine] gestopt door een I/O-fout: {f}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laad_fout(model_id: &str, map: &Path) -> (FoutSoort, String) {
        let mut werker = Echt {
            geladen: None,
            modelmap: None,
        };
        werker
            .laden(model_id, map, Apparaat::Gpu, None)
            .unwrap_err()
    }

    #[test]
    fn tijdcodes_van_een_uitgeknipt_stuk_gaan_terug_naar_de_hele_opname() {
        let stuk = vec![Segment {
            woorden: vec![Woord {
                start_ms: 100,
                eind_ms: 600,
                tekst: "hallo".into(),
            }],
            ..Segment::nieuw(0, 1500, "hallo")
        }];
        let terug = verschuif(stuk, 2800);
        assert_eq!((terug[0].start_ms, terug[0].eind_ms), (2800, 4300));
        assert_eq!(
            (terug[0].woorden[0].start_ms, terug[0].woorden[0].eind_ms),
            (2900, 3400)
        );
        assert_eq!(samples_naar_ms(16_000), 1000);
        assert_eq!(samples_naar_ms(3200), 200);
    }

    #[test]
    fn silero_naast_de_modelmap_wordt_alleen_gevonden_als_het_helemaal_klaar_staat() {
        let basis = tempfile::tempdir().unwrap();
        let modelmap = basis.path().join("parakeet-ultra-q8");
        std::fs::create_dir_all(&modelmap).unwrap();
        assert_eq!(vad_bestand(&modelmap), None);

        let silero = model::zoek(model::SILERO_MODEL).unwrap();
        let map = basis.path().join(model::SILERO_MODEL);
        std::fs::create_dir_all(&map).unwrap();
        let pad = map.join(&silero.bestanden[0].pad);
        // Een afgebroken bestand telt niet.
        std::fs::write(&pad, b"half").unwrap();
        assert_eq!(vad_bestand(&modelmap), None);
        // Met de goede grootte wel.
        let volledig = std::fs::File::create(&pad).unwrap();
        volledig.set_len(silero.bestanden[0].grootte).unwrap();
        assert_eq!(vad_bestand(&modelmap), Some(pad));
    }

    #[test]
    fn sprekers_herkennen_zonder_spraakmodel_of_sprekermodel_geeft_een_fout() {
        let mut werker = Echt {
            geladen: None,
            modelmap: None,
        };
        let map = tempfile::tempdir().unwrap();
        let antwoord =
            mondig_engine::Werker::herken_sprekers(&mut werker, map.path(), &Annulering::nooit());
        assert!(
            matches!(
                antwoord,
                Antwoord::Fout {
                    soort: FoutSoort::NietGeladen,
                    ..
                }
            ),
            "{antwoord:?}"
        );
        // Het sprekermodel laden als spraakmodel is een fout in het verzoek.
        let (soort, melding) = laad_fout(model::SPREKERMODEL, map.path());
        assert_eq!(soort, FoutSoort::Verzoek);
        assert!(melding.contains("geen spraakmodel"), "{melding}");
    }

    #[test]
    fn opnames_boven_de_vier_uur_krijgen_geen_sprekers() {
        assert!(binnen_sprekergrens(4 * 3_600_000));
        assert!(!binnen_sprekergrens(4 * 3_600_000 + 1));
        assert!(matches!(
            te_lang_voor_sprekers(),
            Antwoord::Fout {
                soort: FoutSoort::Sprekermodel,
                ..
            }
        ));
    }

    #[test]
    fn een_hulpmodel_is_geen_spraakmodel() {
        let map = tempfile::tempdir().unwrap();
        let (soort, melding) = laad_fout(model::SILERO_MODEL, map.path());
        assert_eq!(soort, FoutSoort::Verzoek);
        assert!(melding.contains("geen spraakmodel"), "{melding}");
    }

    #[test]
    fn onbekend_model_is_geen_apparaatprobleem() {
        let map = tempfile::tempdir().unwrap();
        let (soort, _) = laad_fout("bestaat-niet", map.path());
        assert_eq!(soort, FoutSoort::ModelOntbreekt);
        assert!(!soort.kan_aan_apparaat_liggen());
    }

    #[test]
    fn ontbrekende_modelbestanden_zijn_geen_apparaatprobleem() {
        let map = tempfile::tempdir().unwrap();
        for model_id in [
            "parakeet-ultra-q8",
            "canary-1b-v2-q8",
            "whisper-large-v3-turbo-q8",
        ] {
            let (soort, melding) = laad_fout(model_id, map.path());
            assert_eq!(soort, FoutSoort::ModelOntbreekt, "{model_id}");
            assert!(melding.contains("ontbreekt"), "{melding}");
        }
    }
}
