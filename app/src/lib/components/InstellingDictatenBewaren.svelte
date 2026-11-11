<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Keuze from '#lib/components/Keuze.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { DICTATEN_TERMIJNEN, herbevestig, type DictatenTermijn } from '#lib/dictaten.js';
  import { termijnLabel } from '#lib/dictatentekst.js';
  import { dictatenStand } from '#lib/dictatenstand.svelte.js';
  import { rij, rijBediening, rijLabel, rijNaam, rijUitleg, vlak } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // "Dictaten bewaren" (ADR 0021): Niet bewaren, Laatste 10, 24 uur (standaard), 7 dagen of
  // 30 dagen. Direct bewaard, zonder "Opslaan". Een kortere of strengere stand wist meteen wat
  // niet meer mag blijven, dus daar komt eerst een vraag, maar alleen als er dictaten wegvallen.

  let bewaard = $state(false);
  // Hoeveel dictaten wegvallen bij de termijn waarop we wachten.
  let wachtAantal = $state(0);
  let fout = $state<string | null>(null);
  // De termijn waarvoor we om bevestiging wachten; de keuzelijst toont ondertussen wat geldt.
  let wacht = $state<DictatenTermijn | null>(null);
  let bezig = $state(false);

  const waarde = $derived(dictatenStand.termijn);

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
    let aantal: number;
    try {
      // Rust telt op dit moment, dus ook een dictaat dat tijdens het kiezen is bijgekomen.
      aantal = await dictatenStand.wegval(gekozen);
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      return;
    }
    if (aantal > 0) {
      wachtAantal = aantal;
      wacht = gekozen;
      await tick();
      document.getElementById('dictaten-wacht-annuleer')?.focus();
      return;
    }
    await zet(gekozen);
  }

  async function bevestig(): Promise<void> {
    const doel = wacht;
    if (doel === null) return;
    // Het getal in de vraag kan verouderd zijn: eerst opnieuw tellen.
    try {
      const { doorgaan, aantal } = await herbevestig(wachtAantal, () => dictatenStand.wegval(doel));
      if (!doorgaan) {
        wachtAantal = aantal;
        return;
      }
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
      return;
    }
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
    // De huidige termijn komt uit Rust.
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
          <option value={termijn}>{termijnLabel(termijn)}</option>
        {/each}
      </Keuze>
    </div>
  </div>
  {#if wacht !== null}
    <div
      class="{vlak} mx-4 mb-3.5 flex flex-col gap-2.5 bg-hover px-4 py-3.5"
      role="alertdialog"
      aria-labelledby="dictaten-wacht-kop"
      aria-describedby="dictaten-wacht-uitleg"
    >
      <b id="dictaten-wacht-kop" class="font-semibold"
        >{wacht === 'niet' ? t().dictatenNietBewarenKop : t().dictatenKorterKop}</b
      >
      <span id="dictaten-wacht-uitleg" class="text-gedempt"
        >{t().dictatenWisVraag(wachtAantal)}</span
      >
      <div class="flex flex-wrap gap-2">
        <Button
          class="bg-fout-tekst text-achtergrond not-aria-disabled:hover:bg-fout-tekst/90"
          onclick={bevestig}
          >{wacht === 'niet' ? t().dictatenNietBewarenBevestig : t().dictatenKorterBevestig}</Button
        >
        <Button id="dictaten-wacht-annuleer" variant="spook" onclick={annuleer}
          >{t().annuleren}</Button
        >
      </div>
    </div>
  {/if}
</div>
