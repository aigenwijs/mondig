## Wat en waarom

<!-- Eén of twee zinnen. Noem de fase of het onderwerp. -->

Fase:

## Hoe getest

- [ ] `cargo fmt --check`, `cargo clippy --all-features`, `cargo test --all-features`, `cargo deny check`
- [ ] `npm run check`, `npm run lint`, `npm test`
- [ ] Handmatig op: Windows / macOS / Linux
- [ ] `mondig-cli bench` (bij wijziging aan engine of model)

## Productregels

- [ ] Geen nieuw netwerkverkeer, of het is gelabeld en gedocumenteerd
- [ ] Geen telemetrie, geen sleutels in bestanden of logs
- [ ] Interfacetekst in gewone taal
- [ ] Nieuwe afhankelijkheid: licentie past in `deny.toml`

## Changelog

- [ ] `CHANGELOG.md` bijgewerkt, of niet nodig omdat:
