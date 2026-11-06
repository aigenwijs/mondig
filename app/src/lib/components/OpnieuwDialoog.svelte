<script lang="ts">
  import { resolve } from '$app/paths';
  import { RadioGroup } from 'bits-ui';
  import Check from '@lucide/svelte/icons/check';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import { Button } from '#lib/components/ui/button/index.js';
  import { huidigeTaal, t } from '#lib';
  import { keuzeRegel, rolTekst } from '#lib/opnieuw.js';
  import type { OpnieuwKeuzes } from '#lib/vergadering.js';

  // De inhoud van de dialoog "Opnieuw uitschrijven" (N-C4, ADR 0023, ontwerp ux-naar-9 sectie
  // Verslag): welk model het verslag maakte (bij zijn rol), per rol een keuze met de tijd voor
  // deze opname of de downloadgrootte, wat er blijft, en Annuleren en Opnieuw uitschrijven. Een
  // model dat er nog niet is, start niet: dan gaat de knop naar Modellen. De pagina zet dit in een
  // dialoog; de kop heeft het id `opnieuw-kop` voor `aria-labelledby`.

  interface Props {
    /** De keuzes uit Rust; `null` zolang ze geladen worden. */
    keuzes: OpnieuwKeuzes | null;
    /** Een fout bij het laden of starten. */
    fout?: string | null;
    /** Hoeveel regels de gebruiker zelf verbeterde. */
    verbeterd: number;
    /** De duur van de opname, voor "voor deze 52 minuten". */
    opnameMs: number;
    /** Het gekozen model (id). */
    gekozen?: string | null;
    bezig?: boolean;
    annuleer: () => void;
    start: (modelId: string) => void;
  }

  let {
    keuzes,
    fout = null,
    verbeterd,
    opnameMs,
    gekozen = $bindable(null),
    bezig = false,
    annuleer,
    start
  }: Props = $props();

  const taal = $derived(huidigeTaal());
  const keuze = $derived(keuzes?.keuzes.find((k) => k.modelId === gekozen) ?? null);
</script>

<div class="flex flex-col gap-3.5">
  <h2 id="opnieuw-kop" class="m-0 font-kop text-kop2 font-semibold">{t().opnieuwKop}</h2>
  <p class="m-0 text-gedempt">{t().opnieuwUitleg(rolTekst(keuzes?.rol ?? null, t()))}</p>
  {#if keuzes === null}
    <p class="m-0 text-klein text-dim" role="status">{t().opnieuwLaden}</p>
  {:else if keuzes.keuzes.length === 0}
    <p class="m-0 text-gedempt">{t().opnieuwGeenKeuze}</p>
  {:else}
    <RadioGroup.Root
      value={gekozen ?? ''}
      onValueChange={(waarde: string) => {
        gekozen = waarde;
      }}
      aria-labelledby="opnieuw-kop"
      class="flex flex-col gap-2"
    >
      {#each keuzes.keuzes as k (k.modelId)}
        <RadioGroup.Item
          value={k.modelId}
          class="group flex cursor-pointer items-start gap-3 rounded-[8px] border border-lijn px-3 py-2.5 text-left outline-none hover:bg-hover focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus data-[state=checked]:border-actie data-[state=checked]:bg-nieuw"
        >
          <span
            class="relative mt-[3px] size-4 shrink-0 rounded-full border-[1.5px] border-rand-veld group-data-[state=checked]:border-actie"
            aria-hidden="true"
            ><span
              class="absolute inset-[3px] hidden rounded-full bg-actie group-data-[state=checked]:block"
            ></span></span
          >
          <span class="flex min-w-0 flex-col">
            <b class="font-medium">{t().opnieuwKeuzeNaam(k.rol)}</b>
            <small class="text-klein text-dim"
              >{keuzeRegel(k, opnameMs, keuzes.rol, t(), taal)}</small
            >
          </span>
        </RadioGroup.Item>
      {/each}
    </RadioGroup.Root>
  {/if}
  <ul class="m-0 flex list-none flex-col gap-1.5 p-0 text-klein">
    <li class="flex items-start gap-2">
      <Check size={14} class="mt-0.5 shrink-0 text-actie" aria-hidden="true" />{t()
        .opnieuwBlijftSprekers}
    </li>
    <li class="flex items-start gap-2">
      <Check size={14} class="mt-0.5 shrink-0 text-actie" aria-hidden="true" />{t()
        .opnieuwBlijftMarkeringen}
    </li>
    {#if verbeterd > 0}
      <li class="flex items-start gap-2" data-testid="opnieuw-verbeterd">
        <TriangleAlert
          size={14}
          class="mt-0.5 shrink-0 text-waarsch"
          aria-hidden="true"
        />{t().opnieuwVerbeterd(verbeterd)}
      </li>
    {/if}
  </ul>
  {#if fout}
    <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
  {/if}
  <div class="-mx-6 -mb-6 flex flex-wrap justify-end gap-2 border-t border-lijn px-6 pt-3 pb-4">
    <Button variant="spook" onclick={annuleer}>{t().annuleren}</Button>
    {#if keuzes !== null && keuzes.keuzes.length === 0}
      <!-- Al met het nauwkeurigste model uitgeschreven: niets te starten. -->
    {:else if keuze && !keuze.aanwezig}
      <Button href={resolve('/modellen')} onclick={annuleer}>{t().opnieuwNaarModellen}</Button>
    {:else}
      <Button
        aria-disabled={bezig || keuze === null}
        onclick={() => {
          if (!bezig && keuze) start(keuze.modelId);
        }}>{t().opnieuwStart}</Button
      >
    {/if}
  </div>
</div>
