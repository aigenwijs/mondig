<script lang="ts">
  import { onMount } from 'svelte';
  import Search from '@lucide/svelte/icons/search';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { keuzeVeld, vlak } from '#lib/stijl.js';
  import { laadVermeldingen, zoekOnderdelen, type Vermeldingen } from '#lib/vermeldingen.js';

  // De onderdelen van anderen (ADR 0016): doorzoekbaar, per onderdeel open te klappen met de
  // volledige licentietekst en eventuele NOTICE. De lijst komt uit scripts/vermeldingen.mjs.

  const STAP = 100;

  let gegevens = $state<Vermeldingen | null>(null);
  let toestand = $state<'laden' | 'klaar' | 'ontbreekt'>('laden');
  let zoekterm = $state('');
  let zichtbaar = $state(STAP);
  // Welke onderdelen openstaan; alleen die renderen hun (lange) tekst.
  let open = $state<Record<string, boolean>>({});

  onMount(() => {
    laadVermeldingen()
      .then((v) => {
        gegevens = v;
        toestand = v ? 'klaar' : 'ontbreekt';
      })
      .catch(() => {
        toestand = 'ontbreekt';
      });
  });

  const gevonden = $derived(gegevens ? zoekOnderdelen(gegevens.onderdelen, zoekterm) : []);
  const getoond = $derived(gevonden.slice(0, zichtbaar));

  const sleutel = (naam: string, versie: string, bron: string): string =>
    `${bron}:${naam}@${versie}`;
</script>

{#if toestand === 'laden'}
  <p class="m-0 text-gedempt" role="status">{t().onderdelenLaden}</p>
{:else if toestand === 'ontbreekt' || !gegevens}
  <p class="m-0 text-gedempt" role="status">{t().onderdelenNietAanwezig}</p>
{:else}
  <div class="relative">
    <Search
      size={16}
      class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-gedempt"
      aria-hidden="true"
    />
    <input
      type="search"
      class="{keuzeVeld} pl-9"
      bind:value={zoekterm}
      oninput={() => {
        // Bij een nieuwe zoekterm weer vanaf de eerste honderd.
        zichtbaar = STAP;
      }}
      aria-label={t().onderdelenZoeken}
      placeholder={t().onderdelenZoeken}
      aria-controls="onderdelenlijst"
    />
  </div>
  <p class="m-0 text-klein text-gedempt" role="status">
    {t().onderdelenAantal(gevonden.length, gegevens.onderdelen.length)}
  </p>

  {#if gevonden.length === 0}
    <p class="m-0 text-gedempt">{t().onderdelenGeen}</p>
  {:else}
    <ul id="onderdelenlijst" class="{vlak} m-0 list-none divide-y divide-lijn p-0">
      {#each getoond as o (sleutel(o.naam, o.versie, o.bron))}
        {@const s = sleutel(o.naam, o.versie, o.bron)}
        <li>
          <details
            ontoggle={(e) => {
              open[s] = e.currentTarget.open;
            }}
          >
            <summary
              class="flex cursor-pointer flex-wrap items-baseline gap-x-3 gap-y-0.5 px-4 py-2.5 hover:bg-hover"
            >
              <span class="font-medium">{o.naam}</span>
              {#if o.versie}<span class="text-gedempt tabular-nums">{o.versie}</span>{/if}
              <span class="ml-auto text-klein text-dim">{o.licentie}</span>
            </summary>
            {#if open[s]}
              <div class="flex flex-col gap-3 px-4 pt-1 pb-4">
                {#if o.teksten.length === 0}
                  <p class="m-0 text-klein text-gedempt">
                    {t().licentieTekstOntbreekt(o.licentie)}
                  </p>
                {/if}
                {#each o.teksten as i (i)}
                  <div>
                    <h4 class="m-0 mb-1 text-klein font-semibold text-gedempt">
                      {t().licentieTekstKop}
                    </h4>
                    <pre
                      class="m-0 max-h-72 overflow-auto rounded-klein bg-verhoogd p-3 text-sectie leading-snug whitespace-pre-wrap">{gegevens
                        .teksten[i] ?? ''}</pre>
                  </div>
                {/each}
                {#if o.notice !== null}
                  <div>
                    <h4 class="m-0 mb-1 text-klein font-semibold text-gedempt">
                      {t().noticeKop}
                    </h4>
                    <pre
                      class="m-0 max-h-72 overflow-auto rounded-klein bg-verhoogd p-3 text-sectie leading-snug whitespace-pre-wrap">{gegevens
                        .teksten[o.notice] ?? ''}</pre>
                  </div>
                {/if}
              </div>
            {/if}
          </details>
        </li>
      {/each}
    </ul>
    {#if gevonden.length > getoond.length}
      <div>
        <Button
          variant="tweede"
          onclick={() => {
            zichtbaar += STAP;
          }}>{t().onderdelenMeer(Math.min(STAP, gevonden.length - getoond.length))}</Button
        >
      </div>
    {/if}
  {/if}
{/if}
