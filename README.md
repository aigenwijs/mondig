# Mondig

Soevereine dicteer- en transcriptie-app voor Windows, gemaakt door Aigenwijs. Spreek in en de
tekst verschijnt in elke app, of zet een opname, audio- of videobestand om in tekst. Standaard
lokaal op je eigen computer, Nederlands eerst.

*English: Mondig is a sovereign dictation and transcription app for Windows. Speech is
processed locally by default, there is no telemetry, and every cloud option shows where your
audio would go. Status: alpha, under development. Licensed under the EUPL-1.2. Build
instructions are in the section "Bouwen op Windows" below.*

> **Status: alfa, in ontwikkeling.** De app werkt, maar is nog niet klaar voor algemeen
> gebruik. Voorlopig bouwen we voor Windows; macOS en Linux volgen later.

## Principes

- Lokaal is de standaard. Audio verlaat je computer alleen als jij dat kiest.
- Geen telemetrie, geen analytics, geen externe crashrapportage.
- Bij elke cloudoptie zie je of die op je computer, in Europa of buiten Europa draait.
  Audio naar een aanbieder buiten Europa gaat pas na een duidelijke waarschuwing.
- API-sleutels staan in de sleutelbos van het besturingssysteem, nooit in bestanden of logs.
- Gemaakt voor mensen zonder technische kennis: de interface is in gewone taal.

## Wat zit erin

| Map | Inhoud |
|---|---|
| `crates/mondig-core/` | De kern in Rust: alle logica zonder interface |
| `crates/mondig-cli/` | Opdrachtregel voor tests en benchmarks |
| `app/` | De Tauri-app: Svelte-frontend in `src/`, dunne Rust-schil in `src-tauri/` |
| `eval/` | Scripts en afspraken om de kwaliteit te meten (geen audio in de repo) |
| `scripts/` | Hulpscripts voor bouwen, versies en licenties |

## Bouwen op Windows

Nodig:

- Rust. De juiste versie haalt `rust-toolchain.toml` zelf op via [rustup](https://rustup.rs).
- Node 24 (zie `.nvmrc`).
- Visual Studio Build Tools met het onderdeel C++ en de Windows SDK.
- CMake, en de [Vulkan SDK](https://vulkan.lunarg.com) (de spraakmotor wordt met Vulkan gebouwd;
  `VULKAN_SDK` moet gezet zijn en `%VULKAN_SDK%\Bin` op het zoekpad staan voor `glslc`).
- Voor de vermeldingen van derden in de app: `cargo install cargo-about --version 0.9.3 --locked`.

Ontwikkelen:

```
npm --prefix app ci
npm --prefix app run dev:app
```

De installer bouwen (inclusief spraakmotor en C++-runtime):

```
npm --prefix app ci
cd app
npm run bundel
```

Het resultaat staat in `target/release/bundle/nsis/`.

Controles die we voor elke wijziging draaien:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny --all-features check
npm --prefix app run check
npm --prefix app run lint
npm --prefix app test
```

De spraakmodellen zitten niet in de repo; de app laat je ze downloaden. Zie `MODEL_LICENSES.md`.

## Licentie en vermeldingen

Mondig valt onder de [European Union Public Licence 1.2](LICENSE) (EUPL-1.2). Je mag de code
gebruiken, aanpassen en verspreiden, ook in een eigen versie, zolang je die versie onder
dezelfde licentie openbaar houdt.

- [NOTICE](NOTICE): copyright en de werken waarvan Mondig werkwijzen of code heeft overgenomen
  (Handy en Vibe, beide MIT), en de lettertypen.
- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md): de bibliotheken die Mondig gebruikt, met
  hun licenties.
- [MODEL_LICENSES.md](MODEL_LICENSES.md): van wie de spraakmodellen zijn en onder welke
  licentie ze staan.

## Bijdragen en beveiliging

Fouten en wensen zijn welkom als issue. Pull requests van buiten nemen we voorlopig niet aan;
lees [CONTRIBUTING.md](CONTRIBUTING.md) voor de reden en de mogelijkheden. Een
beveiligingsprobleem meld je zoals beschreven in [SECURITY.md](SECURITY.md).

Verwijzingen als "ADR 0024" in commentaar noemen interne ontwerpbesluiten; die staan niet in deze repo.

Een momentopname van de broncode per versie staat hier; de dagelijkse ontwikkeling gebeurt
elders.

---

Copyright 2026 Aigenwijs. Licensed under the EUPL-1.2.
