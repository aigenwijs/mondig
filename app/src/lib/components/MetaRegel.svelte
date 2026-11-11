<script lang="ts">
  import ShieldCheck from '@lucide/svelte/icons/shield-check';
  import { huidigeTaal } from '#lib';
  import { datumKort, duurKort } from '#lib/metaregel.js';

  // De metaregel onder een titel (ontwerpsysteem §4 en §5): soort · datum · duur · plek, als
  // gedempte tekst met middenpunten. Wat ontbreekt, blijft weg. Een regel die afbreekt, begint
  // niet met een middenpunt: het punt staat achter het stuk ervoor (zonder breekpunt ertussen),
  // dus het blijft op de regel erboven staan. Het punt is voor de schermlezer verborgen; de
  // stukken staan in de leesvolgorde als losse zinsdelen.

  interface Props {
    /** Bijvoorbeeld "Online vergadering"; de ouder geeft de tekst al in de goede taal. */
    soort?: string | null;
    /** Het moment waarop het ding klaar was (ms sinds 1970). */
    datumMs?: number | null;
    /** De tijd achter de datum zetten ("2 nov, 14:00"), bij een opname waar het moment telt. */
    metTijd?: boolean;
    /** De duur in ms. */
    duurMs?: number | null;
    /** Extra stukken na de duur, bijvoorbeeld "3 sprekers". */
    extra?: string[];
    /** Waar het gebeurde, bijvoorbeeld "op deze computer". */
    plek?: string | null;
    /** Tooltip bij de plek, bijvoorbeeld met welk model. */
    plekTitel?: string;
    /** Extra tekst voor de schermlezer achter de plek. */
    plekSr?: string;
    /** Een schildje voor de plek (de gegevens blijven op deze computer). */
    plekIcoon?: boolean;
    /** Extra stukken na de plek, bijvoorbeeld "Mondig bewaart dit verslag tot dinsdag". */
    staart?: string[];
    /** Het moment van nu voor "dit jaar", standaard de klok (voor de tests). */
    nu?: number;
  }
  let {
    soort = null,
    datumMs = null,
    metTijd = false,
    duurMs = null,
    extra = [],
    plek = null,
    plekTitel,
    plekSr,
    plekIcoon = false,
    staart = [],
    nu
  }: Props = $props();

  const taal = $derived(huidigeTaal());
  const stukken = $derived(
    [
      soort ? { sleutel: 'soort', tekst: soort } : null,
      datumMs !== null ? { sleutel: 'datum', tekst: datumKort(datumMs, taal, nu, metTijd) } : null,
      duurMs !== null ? { sleutel: 'duur', tekst: duurKort(duurMs, taal) } : null,
      ...extra.map((tekst, i) => ({ sleutel: `extra-${String(i)}`, tekst })),
      plek ? { sleutel: 'plek', tekst: plek } : null,
      ...staart.map((tekst, i) => ({ sleutel: `staart-${String(i)}`, tekst }))
    ].filter((s) => s !== null)
  );
</script>

<!-- Geen witruimte tussen de tekst en het middenpunt: het punt hoort aan het stuk ervoor. -->
<!-- prettier-ignore -->
<p class="m-0 flex flex-wrap gap-x-1.5 text-klein text-dim">
  {#each stukken as stuk, i (stuk.sleutel)}
    <span class="whitespace-nowrap" data-meta={stuk.sleutel}>{#if stuk.sleutel === 'plek'}<span class="inline-flex items-center gap-1" title={plekTitel}>{#if plekIcoon}<ShieldCheck size={14} aria-hidden="true" />{/if}{stuk.tekst}{#if plekSr}<span class="sr-only">, {plekSr}</span>{/if}</span>{:else}{stuk.tekst}{/if}{#if i < stukken.length - 1}<span aria-hidden="true">&nbsp;·</span>{/if}</span>
  {/each}
</p>
