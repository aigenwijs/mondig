<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { stripToestand } from '#lib/dictaat.js';
  import { dictaatStand } from '#lib/dictaatstand.svelte.js';
  import { meldingTekst } from '#lib/taak.js';

  // Een smalle strook bovenaan elk scherm behalve het startscherm, voor een dictaat dat
  // afliep met een probleem dat de gebruiker nog niet zag. Tijdens luisteren en verwerken
  // blijft hij weg: de dicteerbalk toont dat al. Het grote paneel op het startscherm toont
  // dezelfde stand (ADR 0012: één toestandsmodel, meerdere weergaven).
  const weergave = $derived(dictaatStand.weergave);
  const opStart = $derived(page.url.pathname === '/');
  const toestand = $derived(
    opStart ? null : stripToestand(weergave, dictaatStand.gezien, dictaatStand.plak)
  );
</script>

<!-- Buiten de pagina's, boven de inhoud van het hoofdvlak. De statusregio staat er altijd, zodat
     een schermlezer een probleem na afloop meldt zonder dat de focus verspringt. Een fout is
     een alert. -->
<div
  class="w-full max-w-[760px] px-10 max-[639px]:px-5"
  class:pt-5={toestand === 'plakprobleem'}
  role="status"
>
  {#if toestand === 'plakprobleem'}
    <div
      class="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-groot bg-vlak py-1.5 pr-1.5 pl-3.5 text-basis"
    >
      <span class="size-2 shrink-0 rounded-full bg-waarsch" aria-hidden="true"></span>
      <span class="font-medium">{t().stripPlakprobleem}</span>
      <Button variant="spook" class="ml-auto" href={resolve('/')}>
        {t().stripNaarStart}<ArrowRight size={16} aria-hidden="true" /></Button
      >
    </div>
  {/if}
</div>

{#if toestand === 'fout' && weergave.type === 'fout'}
  <div class="w-full max-w-[760px] px-10 pt-5 max-[639px]:px-5" role="alert">
    <div
      class="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-groot bg-fout-vlak py-1.5 pr-1.5 pl-3.5 text-basis text-fout-tekst"
    >
      <CircleAlert size={16} class="shrink-0" aria-hidden="true" />
      <span>{meldingTekst(weergave, t().appNietBijgewerkt)}</span>
      <Button variant="spook" class="ml-auto text-fout-tekst" href={resolve('/')}>
        {t().stripNaarStart}<ArrowRight size={16} aria-hidden="true" /></Button
      >
    </div>
  </div>
{/if}
