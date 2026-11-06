<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Keuze from '#lib/components/Keuze.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { DICTATEN_TERMIJNEN, vraagVoorTermijn, type DictatenTermijn } from '#lib/dictaten.js';
  import { dictatenStand } from '#lib/dictatenstand.svelte.js';
  import { rij, rijBediening, rijLabel, rijNaam, rijUitleg, vlak } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // "Dictaten bewaren" (ADR 0021): Niet bewaren, 24 uur (standaard) of 7 dagen. Direct bewaard,
  // zonder "Opslaan". Korter zetten wist meteen wat ouder is. Naar Niet bewaren wist alles,
  // dus daar komt eerst een vraag, maar alleen als er bewaarde dictaten zijn.

  let bewaard = $state(false);
  let fout = $state<string | null>(null);
  // De termijn waarvoor we om bevestiging wachten; de keuzelijst toont ondertussen wat geldt.
  let wacht = $state<DictatenTermijn | null>(null);
  let bezig = $state(false);

  const waarde = $derived(dictatenStand.termijn);

  function label(termijn: DictatenTermijn): string {
    switch (termijn) {
      case 'niet':
        return t().dictatenTermijnNiet;
      case '24u':
        return t().dictatenTermijn24u;
      case '7d':
        return t().dictatenTermijn7d;
    }
  }

  async function zet(termijn: DictatenTermijn): Promise<void> {
    bewaard = false;
    fout = null;
    bezig = true;
    try {
      await dictatenStand.zetTermijn(termijn);
      bewaard = true;
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    } finally {
      bezig = false;
    }
  }

  async function wijzig(event: Event & { currentTarget: HTMLSelectElement }): Promise<void> {
    const gekozen = DICTATEN_TERMIJNEN.find((d) => d === event.currentTarget.value);
    if (gekozen === undefined || gekozen === waarde) return;
    // Het menu laat tot na de bevestiging zien wat nu geldt.
    event.currentTarget.value = waarde;
    // De lijst kan sinds het laden veranderd zijn (een dictaat tijdens het kiezen).
    await dictatenStand.ververs();
    if (vraagVoorTermijn(gekozen, dictatenStand.bewaard)) {
      wacht = gekozen;
      await tick();
      document.getElementById('dictaten-niet-annuleer')?.focus();
      return;
    }
    await zet(gekozen);
  }

  async function bevestig(): Promise<void> {
    const doel = wacht;
    if (doel === null) return;
    wacht = null;
    await zet(doel);
    await tick();
    document.getElementById('dictaten-keuze')?.focus();
  }

  async function annuleer(): Promise<void> {
    wacht = null;
    await tick();
    document.getElementById('dictaten-keuze')?.focus();
  }

  onMount(() => {
    // Het aantal bewaarde dictaten is nodig voor de bevestigvraag.
    void dictatenStand.ververs();
  });
</script>

<div id="dictaten-bewaren">
  <div class={rij}>
    <div class={rijLabel}>
      <h3 id="dictaten-bewaren-kop" class={rijNaam}>{t().dictatenBewarenKop}</h3>
      <span id="dictaten-bewaren-hulp" class={rijUitleg}>{t().dictatenBewarenHulp}</span>
      {#if fout}
        <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
      {/if}
      <p class="m-0 text-klein text-gelukt" role="status">{bewaard ? t().opgeslagen : ''}</p>
    </div>
    <div class={rijBediening}>
      <Keuze
        id="dictaten-keuze"
        aria-labelledby="dictaten-bewaren-kop"
        aria-describedby="dictaten-bewaren-hulp"
        breedte="sm:w-[10rem]"
        value={waarde}
        disabled={bezig}
        onchange={wijzig}
      >
        {#each DICTATEN_TERMIJNEN as termijn (termijn)}
          <option value={termijn}>{label(termijn)}</option>
        {/each}
      </Keuze>
    </div>
  </div>
  {#if wacht !== null}
    <div
      class="{vlak} mx-4 mb-3.5 flex flex-col gap-2.5 bg-hover px-4 py-3.5"
      role="alertdialog"
      aria-labelledby="dictaten-niet-kop"
      aria-describedby="dictaten-niet-uitleg"
    >
      <b id="dictaten-niet-kop" class="font-semibold">{t().dictatenNietBewarenKop}</b>
      <span id="dictaten-niet-uitleg" class="text-gedempt"
        >{t().dictatenWisVraag(dictatenStand.bewaard)}</span
      >
      <div class="flex flex-wrap gap-2">
        <Button
          class="bg-fout-tekst text-achtergrond not-aria-disabled:hover:bg-fout-tekst/90"
          onclick={bevestig}>{t().dictatenNietBewarenBevestig}</Button
        >
        <Button id="dictaten-niet-annuleer" variant="spook" onclick={annuleer}
          >{t().annuleren}</Button
        >
      </div>
    </div>
  {/if}
</div>
