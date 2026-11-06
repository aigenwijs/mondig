<script lang="ts">
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import ArrowRight from '@lucide/svelte/icons/arrow-right';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import X from '@lucide/svelte/icons/x';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { dictaatAnnuleer } from '#lib/commandos.js';
  import { formatDuur, stripToestand } from '#lib/dictaat.js';
  import { dictaatStand } from '#lib/dictaatstand.svelte.js';
  import { meldingTekst } from '#lib/taak.js';

  // Een smalle strook bovenaan elk scherm behalve het startscherm, zolang er een dictaat
  // loopt. Het grote paneel op het startscherm toont dezelfde stand (ADR 0012: één
  // toestandsmodel, meerdere weergaven).
  const weergave = $derived(dictaatStand.weergave);
  const opStart = $derived(page.url.pathname === '/');
  const toestand = $derived(opStart ? null : stripToestand(weergave, dictaatStand.gezien));
  const luistert = $derived(weergave.type === 'luistert' ? weergave : null);
  const klaarMetTekst = $derived(weergave.type === 'klaar' && weergave.uitslag.tekst !== null);

  let nu = $state(Date.now());
  $effect(() => {
    if (toestand !== 'luistert') return;
    nu = Date.now();
    const timer = setInterval(() => {
      nu = Date.now();
    }, 1000);
    return () => {
      clearInterval(timer);
    };
  });

  async function annuleer(): Promise<void> {
    if (!('taak' in weergave)) return;
    try {
      await dictaatAnnuleer(weergave.taak);
    } catch {
      // Was het al afgelopen, dan meldt het toestandsmodel dat zelf.
    }
  }

  const strook =
    'flex flex-wrap items-center gap-x-3 gap-y-1 rounded-groot bg-vlak py-1.5 pr-1.5 pl-3.5 text-basis';
</script>

<!-- Buiten de pagina's, boven de inhoud van het hoofdvlak. De statusregio staat er altijd, zodat
     een schermlezer elke wijziging meldt zonder dat de focus verspringt. Een fout is een alert. -->
<div
  class="w-full max-w-[760px] px-10 max-[639px]:px-5"
  class:pt-5={toestand !== null && toestand !== 'fout'}
  role="status"
>
  {#if toestand === 'luistert' && luistert}
    <div class={strook}>
      <span
        class="size-2 shrink-0 rounded-full bg-actie motion-safe:animate-pulse"
        aria-hidden="true"
      ></span>
      <b class="font-semibold">{t().dictaatLuistert}</b>
      <span class="font-kop font-semibold text-gedempt tabular-nums" aria-hidden="true"
        >{formatDuur(nu - luistert.sinds)}</span
      >
      <Button variant="spook" class="ml-auto" onclick={annuleer}>
        <X size={16} aria-hidden="true" />{t().annulerenMetEsc}</Button
      >
    </div>
  {:else if toestand === 'verwerkt'}
    <div class={strook}>
      <LoaderCircle
        size={16}
        class="shrink-0 text-gedempt motion-safe:animate-spin"
        aria-hidden="true"
      />
      <b class="font-semibold">{t().stripVerwerkt}</b>
      <Button variant="spook" class="ml-auto" onclick={annuleer}>
        <X size={16} aria-hidden="true" />{t().annuleren}</Button
      >
    </div>
  {:else if toestand === 'klaar'}
    <div class={strook}>
      <span class="size-2 shrink-0 rounded-full bg-gelukt" aria-hidden="true"></span>
      <span class="font-medium">
        {klaarMetTekst ? t().stripKlaar : t().geenTekst}
      </span>
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
