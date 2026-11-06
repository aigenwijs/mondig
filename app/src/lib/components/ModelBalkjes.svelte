<script lang="ts">
  import { Tooltip } from 'bits-ui';
  import Info from '@lucide/svelte/icons/info';
  import { t } from '#lib';
  import type { Stap } from '#lib/modellen.js';

  // De twee balkjes van een model: "nauwkeurig" en "snel", elk in vijf vaste stappen, onder
  // elkaar rechts in een beheerkaart op Modellen (zoals Handy; gebruikerstest na N-C). De stappen
  // komen uit de catalogus (afgeleid van onze bench, zie modellen.json). Een schermlezer krijgt
  // per balkje één zin ("Nauwkeurig: 3 van 5"); de blokjes zelf zijn alleen voor het oog. Het
  // (i)-icoon opent met muis, toetsenbord (focus) en schermlezer dezelfde uitleg.

  interface Props {
    nauwkeurigheid: Stap;
    snelheid: Stap;
  }
  let { nauwkeurigheid, snelheid }: Props = $props();

  const stappen = [1, 2, 3, 4, 5] as const;
  const balken = $derived([
    { naam: t().balkNauwkeurig, stap: nauwkeurigheid },
    { naam: t().balkSnel, stap: snelheid }
  ]);
</script>

<div class="flex items-center gap-1">
  <div class="flex flex-col items-end gap-1">
    {#each balken as { naam, stap } (naam)}
      <div
        class="flex items-center gap-2 text-klein text-dim"
        role="img"
        aria-label={t().balkStappen(naam, stap)}
      >
        <span aria-hidden="true">{naam.toLowerCase()}</span>
        <span class="flex gap-[2px]" aria-hidden="true">
          {#each stappen as nr (nr)}
            <span class="h-1.5 w-3 rounded-[2px] {nr <= stap ? 'bg-actie' : 'bg-lijn'}"></span>
          {/each}
        </span>
      </div>
    {/each}
  </div>
  <Tooltip.Provider delayDuration={100}>
    <Tooltip.Root>
      <Tooltip.Trigger
        class="-my-1 inline-flex size-8 cursor-help items-center justify-center rounded-full text-dim transition-colors hover:text-tekst"
        aria-label={t().balkUitlegKnop}
      >
        <Info size={14} aria-hidden="true" />
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content
          side="top"
          sideOffset={6}
          class="z-50 max-w-[260px] rounded-klein bg-verhoogd px-3 py-2 text-klein leading-snug text-tekst shadow-lg ring-1 ring-lijn"
        >
          {t().balkUitleg}
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  </Tooltip.Provider>
</div>
