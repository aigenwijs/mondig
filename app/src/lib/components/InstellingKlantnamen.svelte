<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import RijKop from '#lib/components/RijKop.svelte';
  import VerwijderVraag from '#lib/components/VerwijderVraag.svelte';
  import { t } from '#lib';
  import { klantenLijst, klantenVerwijder } from '#lib/klanten.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // Klantnamen (UI-E): de namen die Mondig onthoudt als suggestie bij het veld Klant, alleen op
  // deze computer. "Verwijderen…" opent de vraag "12 klantnamen verwijderen?" (ontwerpsysteem §5);
  // opgeslagen bestanden en verslagen blijven zoals ze zijn. Bij nul namen is er geen knop maar
  // een uitleg. Na het verwijderen gaat de focus naar de regel eronder (`role="status"`), want de
  // knop verdwijnt (ontwerpsysteem §6).

  interface Props {
    /** Alleen voor de tests: het aantal klantnamen vooraf, zonder het op te halen. */
    startAantal?: number | null;
  }
  let { startAantal = null }: Props = $props();

  // `null` zolang de lijst nog niet is opgehaald.
  let aantal = $state<number | null>(untrack(() => startAantal));
  let open = $state(false);
  let bezig = $state(false);
  let fout = $state<string | null>(null);
  let verwijderd = $state(false);
  // Het aantal waarmee de vraag opende: de vraag verandert niet mee als de lijst leeg raakt.
  let vraagAantal = $state(0);

  onMount(() => {
    klantenLijst()
      .then((lijst) => {
        aantal = lijst.length;
      })
      .catch(() => {
        aantal = 0;
      });
  });

  async function verwijder(): Promise<void> {
    if (bezig) return;
    bezig = true;
    fout = null;
    try {
      await klantenVerwijder();
      aantal = 0;
      verwijderd = true;
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      bezig = false;
    }
  }

  const status = $derived(
    aantal === null
      ? ''
      : verwijderd
        ? t().klantnamenVerwijderd
        : aantal === 0
          ? t().klantnamenGeen
          : t().klantnamenAantal(aantal)
  );
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="klantnamen-kop"
      label={t().klantnamenKop}
      hulpId="klantnamen-hulp"
      hulp={t().klantnamenHulp}
    />
    <!-- De vaste plek voor de focus na het verwijderen, en de melding voor de schermlezer. -->
    <p
      id="klantnamen-status"
      tabindex="-1"
      role="status"
      class="m-0 rounded-klein text-klein text-gedempt outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus empty:hidden"
    >
      {status}
    </p>
    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}
  </div>
  <div class={rijBediening}>
    <VerwijderVraag
      id="klantnamen"
      bind:open
      knop={aantal !== null && aantal > 0}
      knopAria="{t().verwijderenKnop} {t().klantnamenKop}"
      vraag={t().klantnamenVraag(vraagAantal)}
      voorOpen={() => {
        vraagAantal = aantal ?? 0;
        return Promise.resolve(true);
      }}
      uitleg={t().klantnamenUitleg}
      {bezig}
      {verwijder}
      focusTerug={() => document.getElementById('klantnamen-status')}
    />
  </div>
</div>
