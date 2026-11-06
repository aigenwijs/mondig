<script lang="ts">
  import RijKop from '#lib/components/RijKop.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { instellingenTerugzetten, logboekMapOpen, logboekTekst } from '#lib/computer.js';
  import { kopieerNaarKlembord } from '#lib/klembord.js';
  import { rij, rijBediening, rijLabel } from '#lib/stijl.js';
  import { foutUitCommando } from '#lib/taak.js';

  // "Problemen oplossen": het logboek voor support en Mondig terugzetten (fase 5, S-2). Het
  // logboek bevat geen tekst die je dicteert of uitschrijft; je kopieert het zelf, er gaat niets
  // vanzelf weg (ADR 0003).

  // Eigen meldingen per rij: wat bij het logboek hoort, staat onder het logboek.
  let logMelding = $state<string | null>(null);
  let logFout = $state<string | null>(null);
  let melding = $state<string | null>(null);
  let fout = $state<string | null>(null);
  let vraagt = $state(false);

  async function kopieer(): Promise<void> {
    logMelding = null;
    logFout = null;
    try {
      const gelukt = await kopieerNaarKlembord(await logboekTekst());
      if (gelukt) logMelding = t().logboekGekopieerd;
      else logFout = t().logboekKopieerMislukt;
    } catch (e) {
      logFout = foutUitCommando(e, t().logboekKopieerMislukt).melding;
    }
  }

  async function openMap(): Promise<void> {
    logMelding = null;
    logFout = null;
    try {
      await logboekMapOpen();
    } catch (e) {
      logFout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }

  async function zetTerug(): Promise<void> {
    fout = null;
    try {
      const uitkomst = await instellingenTerugzetten();
      vraagt = false;
      melding = uitkomst.sneltoetsBezet ? t().terugzetSneltoetsBezet : t().terugzetKlaar;
      // De rijen hierboven laden hun stand bij het openen; opnieuw laden laat ze de standaard zien.
      window.setTimeout(() => {
        window.location.reload();
      }, 1500);
    } catch (e) {
      fout = foutUitCommando(e, t().somsGingErIetsMis).melding;
    }
  }
</script>

<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="logboek-kop"
      label={t().logboekKop}
      hulpId="logboek-hulp"
      hulp={t().logboekHulp}
    />
    {#if logMelding}
      <p class="m-0 text-klein text-gelukt" role="status">{logMelding}</p>
    {/if}
    {#if logFout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{logFout}</p>
    {/if}
  </div>
  <div class={rijBediening}>
    <Button variant="tweede" onclick={() => void openMap()}>{t().logmapOpenen}</Button>
    <Button variant="tweede" onclick={() => void kopieer()}>{t().logboekKopieren}</Button>
  </div>
</div>
<div class={rij}>
  <div class={rijLabel}>
    <RijKop
      kopId="terugzet-kop"
      label={t().terugzetKop}
      hulpId="terugzet-hulp"
      hulp={t().terugzetHulp}
    />
    {#if melding}
      <p class="m-0 text-klein text-gelukt" role="status">{melding}</p>
    {/if}
    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}
  </div>
  <div class={rijBediening}>
    {#if vraagt}
      <span class="text-basis" role="alert">{t().terugzetVraag}</span>
      <Button variant="spook" onclick={() => (vraagt = false)}>{t().annuleren}</Button>
      <Button variant="tweede" onclick={() => void zetTerug()}>{t().terugzetBevestig}</Button>
    {:else}
      <Button variant="tweede" onclick={() => (vraagt = true)}>{t().terugzetKnop}</Button>
    {/if}
  </div>
</div>
