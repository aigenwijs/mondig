<script lang="ts">
  import InstellingSchakelaar from '#lib/components/InstellingSchakelaar.svelte';
  import { t } from '#lib';
  import { stopwoordenInstelling, stopwoordenZet } from '#lib/instellingen.js';

  // "Stopwoorden weghalen" (ADR 0028): klanken als 'ehm' en 'uh' gaan weg, met vaste regels in
  // Rust. Standaard aan. Direct bewaard; geldt vanaf het volgende dictaat of bestand.
</script>

<InstellingSchakelaar
  kopId="stopwoorden-kop"
  hulpId="stopwoorden-hulp"
  label={t().stopwoordenKop}
  hulp={t().stopwoordenHulp}
  lees={stopwoordenInstelling}
  zet={stopwoordenZet}
  standaard={true}
/>
