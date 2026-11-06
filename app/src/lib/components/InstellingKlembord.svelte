<script lang="ts">
  import InstellingSchakelaar from '#lib/components/InstellingSchakelaar.svelte';
  import { t } from '#lib';
  import { alleenKlembordInstelling, alleenKlembordZet } from '#lib/instellingen.js';

  // De terugval "Alleen op het klembord zetten" (sprint 3c): voor programma's waarin Ctrl+V
  // niet werkt. Direct bewaard; geldt vanaf het volgende dictaat. Standaard uit.
</script>

<InstellingSchakelaar
  kopId="klembord-kop"
  hulpId="klembord-hulp"
  label={t().klembordKop}
  hulp={t().klembordHulp}
  lees={alleenKlembordInstelling}
  zet={alleenKlembordZet}
  standaard={false}
/>
