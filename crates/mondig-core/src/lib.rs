//! Kern van Mondig: alle logica die los van de app te testen moet zijn.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod aanbieder;
pub mod afgeschaft;
pub mod afspelen;
pub mod audio;
pub mod bestanden;
pub mod bewaak;
#[cfg(feature = "bewaren")]
pub mod bewaren;
pub mod bewerken;
pub mod dictaten;
#[cfg(feature = "download")]
pub mod download;
#[cfg(feature = "download")]
pub mod downloadrij;
pub mod engine;
pub mod filter;
pub mod gesprek;
#[cfg(feature = "download")]
pub mod hardware;
pub mod logboek;
pub mod meten;
pub mod model;
pub mod modelkeuze;
pub mod notulen;
#[cfg(feature = "opname")]
pub mod opname;
#[cfg(feature = "taalmodel")]
pub mod opschonen;
pub mod persoonsgegevens;
pub mod probleem;
pub mod proef;
#[cfg(feature = "taalmodel")]
pub mod sleutel;
pub mod snelheid;
pub mod sneltoets;
pub mod soevereiniteit;
pub mod spoor;
pub mod stilte;
pub mod taak;
#[cfg(feature = "taalmodel")]
pub mod taalmodel;
pub mod verslag;
pub mod wakker;
