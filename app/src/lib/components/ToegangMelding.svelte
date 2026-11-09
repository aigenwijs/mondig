<script lang="ts">
  import { onMount } from 'svelte';
  import CircleAlert from '@lucide/svelte/icons/circle-alert';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import {
    toegankelijkheidOpen,
    toegankelijkheidStand,
    type ToegangStand
  } from '#lib/commandos.js';
  import { foutVak } from '#lib/stijl.js';

  // Alleen op de Mac: zonder het recht op toegankelijkheid kan Mondig niet plakken (mijlpaal
  // M1). Bij de start gekeken, en opnieuw als het venster terugkomt na Systeeminstellingen.
  // Lukt het opvragen niet (Windows, de tests), dan tonen we niets.
  let stand = $state<ToegangStand>('niet_nodig');

  async function controleer(): Promise<void> {
    try {
      stand = await toegankelijkheidStand();
    } catch {
      stand = 'niet_nodig';
    }
  }

  onMount(() => {
    void controleer();
    const bijFocus = (): void => {
      void controleer();
    };
    window.addEventListener('focus', bijFocus);
    return () => {
      window.removeEventListener('focus', bijFocus);
    };
  });
</script>

{#if stand === 'geweigerd'}
  <div class={foutVak} role="alert">
    <p class="m-0 flex items-start gap-2.5">
      <CircleAlert size={16} class="mt-0.5 shrink-0" aria-hidden="true" />
      <span>
        <b class="mb-0.5 block font-semibold">{t().toegangKop}</b>
        {t().toegangTekst}
      </span>
    </p>
    <p class="m-0 text-klein">{t().toegangOpnieuw}</p>
    <div class="flex flex-wrap gap-2">
      <Button onclick={() => void toegankelijkheidOpen()} data-hoofdactie=""
        >{t().toegankelijkheidOpenen}</Button
      >
      <Button variant="tweede" onclick={() => void controleer()}>{t().opnieuwControleren}</Button>
    </div>
  </div>
{/if}
