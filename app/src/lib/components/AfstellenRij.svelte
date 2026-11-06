<script lang="ts">
  import { resolve } from '$app/paths';
  import Check from '@lucide/svelte/icons/check';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import LoaderCircle from '@lucide/svelte/icons/loader-circle';
  import RijKop from '#lib/components/RijKop.svelte';
  import Vinkvak from '#lib/components/Vinkvak.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import type { AdviesRegel, AfstelStap, AfstelToestand } from '#lib/afstellen.js';
  import { rij, rijBediening, rijLabel, rijNaam } from '#lib/stijl.js';

  // De rij "Mondig opnieuw afstellen" in vijf toestanden (ontwerp ux-naar-9, sectie 5): rust, bezig
  // (met de stappen en Stoppen), het advies als lijst met vinkjes, overgenomen en "Alles staat al
  // goed". Deze component toont alleen; de logica staat in `InstellingComputer.svelte`.

  interface Props {
    toestand: AfstelToestand;
    /** Waar de stappen zijn (alleen bij `bezig`). */
    stap?: AfstelStap;
    /** Staat de stap "Videokaart testen" in de lijst? Alleen als er een proef komt. */
    toonKaartStap?: boolean;
    /** Eén zin over de computer ("16 GB werkgeheugen, 8 processorkernen, ..."). */
    computerZin?: string;
    regels?: AdviesRegel[];
    /** Sleutels van de regels waarvan het vinkje weg is. */
    uit?: string[];
    /** Wat de afgelopen actie meldt (Stoppen, een fout). */
    melding?: string | null;
    fout?: string | null;
    /** Passen de aangevinkte downloads samen niet op de schijf? De melding, anders `null`. */
    ruimteMelding?: string | null;
    /** Mag Afstellen nu niet (een gestopte snelheidstest rondt nog af)? Met de reden. */
    afstellenUitReden?: string | null;
    /** Klopt de knop "Advies overnemen" op dit moment? Uit zolang het loopt of niets aangevinkt is. */
    overnemenBezig?: boolean;
    overgenomen?: string;
    goedHulp?: string;
    onAfstellen?: () => void;
    onStoppen?: () => void;
    onWissel?: (sleutel: string, aan: boolean) => void;
    onOvernemen?: () => void;
    onNietNu?: () => void;
    onSluiten?: () => void;
  }

  let {
    toestand,
    stap = 'computer',
    toonKaartStap = true,
    computerZin = '',
    regels = [],
    uit = [],
    melding = null,
    fout = null,
    ruimteMelding = null,
    afstellenUitReden = null,
    overnemenBezig = false,
    overgenomen = '',
    goedHulp = '',
    onAfstellen,
    onStoppen,
    onWissel,
    onOvernemen,
    onNietNu,
    onSluiten
  }: Props = $props();

  const STAPPEN: { stap: AfstelStap; tekst: () => string }[] = [
    { stap: 'computer', tekst: () => t().afstelStapComputer },
    { stap: 'kaart', tekst: () => t().afstelStapKaart },
    { stap: 'modellen', tekst: () => t().afstelStapModellen }
  ];
  const zichtbareStappen = $derived(STAPPEN.filter((s) => s.stap !== 'kaart' || toonKaartStap));
  const stapNummer = $derived(zichtbareStappen.findIndex((s) => s.stap === stap));
  const aangevinkt = $derived(
    regels.filter((r) => r.invoer !== null && !uit.includes(r.sleutel)).length
  );
  const verbeteringen = $derived(regels.filter((r) => r.invoer !== null));
  const reedsGoed = $derived(regels.filter((r) => r.invoer === null));
  const geenVinkje = $derived(aangevinkt === 0);
  /** Staat er iets om over te nemen? Zo niet (alleen een opmerking), dan is er geen knop daarvoor. */
  const kanOvernemen = $derived(verbeteringen.length > 0);
  const overnemenUit = $derived(overnemenBezig || geenVinkje || ruimteMelding !== null);
</script>

{#if toestand === 'rust'}
  <div class={rij}>
    <div class={rijLabel}>
      <RijKop
        kopId="check-kop"
        label={t().computerCheckKop}
        hulpId="check-hulp"
        hulp={t().computerCheckHulp}
      />
      {#if melding}
        <p class="m-0 text-klein text-gedempt" role="status">{melding}</p>
      {/if}
      {#if afstellenUitReden}
        <p class="m-0 text-klein text-dim">{afstellenUitReden}</p>
      {/if}
      {#if fout}
        <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
      {/if}
    </div>
    <div class={rijBediening}>
      <Button
        variant="tweede"
        aria-describedby="check-kop check-hulp"
        aria-disabled={afstellenUitReden !== null}
        onclick={() => {
          if (afstellenUitReden === null) onAfstellen?.();
        }}>{t().computerCheckKnop}</Button
      >
    </div>
  </div>
{:else if toestand === 'bezig'}
  <div class="{rij} items-start">
    <div class={rijLabel}>
      <RijKop
        kopId="check-kop"
        label={t().afstelBezigKop}
        hulpId="check-hulp"
        hulp={t().afstelBezigHulp}
      />
      <ol class="m-0 mt-2.5 flex list-none flex-col gap-1.5 p-0 text-klein" role="status">
        {#each zichtbareStappen as s, i (s.stap)}
          <li
            class="flex items-center gap-2 {i < stapNummer
              ? 'text-gedempt'
              : i === stapNummer
                ? 'text-tekst'
                : 'text-dim'}"
          >
            {#if i < stapNummer}
              <Check size={14} class="text-actie" aria-label={t().afstelStapKlaarAria} />
            {:else if i === stapNummer}
              <LoaderCircle
                size={14}
                class="motion-safe:animate-spin"
                aria-label={t().afstelStapBezigAria}
              />
            {:else}
              <span class="inline-block w-3.5" aria-hidden="true"></span>
            {/if}
            {s.tekst()}
          </li>
        {/each}
      </ol>
    </div>
    <div class={rijBediening}>
      <Button variant="spook" onclick={() => onStoppen?.()}>{t().afstelStoppen}</Button>
    </div>
  </div>
{:else if toestand === 'advies'}
  <div class="flex flex-col gap-3 px-4 py-3.5" role="group" aria-labelledby="advies-kop">
    <div class="flex flex-col gap-0.5">
      <h3 id="advies-kop" class={rijNaam}>{t().afstelAdviesKop}</h3>
      {#if computerZin}
        <p class="m-0 text-klein text-dim">{t().wizAdviesGelezen(computerZin)}</p>
      {/if}
    </div>
    <ul class="m-0 flex list-none flex-col gap-1 p-0" aria-label={t().afstelAdviesLijstAria}>
      {#each verbeteringen as r (r.sleutel)}
        <li>
          <Vinkvak
            bind:checked={
              () => !uit.includes(r.sleutel), (aan: boolean) => onWissel?.(r.sleutel, aan)
            }
            disabled={overnemenBezig}
          >
            <b class="block font-medium">{r.titel}</b>
            <small class="block text-klein text-gedempt">{r.uitleg}</small>
          </Vinkvak>
        </li>
      {/each}
      {#each reedsGoed as r (r.sleutel)}
        <li class="flex items-start gap-2.5 py-1 text-basis">
          <span class="mt-0.5 flex size-5 shrink-0 items-center justify-center text-dim">
            <Check size={14} aria-hidden="true" />
          </span>
          <span class="min-w-0">
            <b class="block font-medium text-gedempt">{r.titel}</b>
            <small class="block text-klein text-dim">{r.uitleg}</small>
          </span>
        </li>
      {/each}
    </ul>
    {#if fout}
      <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
    {/if}
    <div class="flex flex-wrap items-center gap-2">
      {#if kanOvernemen}
        <Button
          aria-disabled={overnemenUit}
          aria-describedby={ruimteMelding
            ? 'advies-niets'
            : geenVinkje
              ? 'advies-niets'
              : undefined}
          onclick={() => {
            if (!overnemenUit) onOvernemen?.();
          }}>{t().afstelOvernemen}</Button
        >
        <Button variant="spook" onclick={() => onNietNu?.()}>{t().afstelNietNu}</Button>
      {:else}
        <Button variant="tweede" onclick={() => onNietNu?.()}>{t().afstelSluiten}</Button>
      {/if}
      {#if ruimteMelding}
        <span id="advies-niets" class="text-klein text-fout-tekst">{ruimteMelding}</span>
      {:else if kanOvernemen && geenVinkje}
        <span id="advies-niets" class="text-klein text-dim">{t().afstelNietsAangevinkt}</span>
      {/if}
    </div>
  </div>
{:else if toestand === 'overgenomen'}
  <div class={rij}>
    <CircleCheck size={18} class="text-actie" aria-hidden="true" />
    <div class={rijLabel}>
      <h3 class={rijNaam}>{t().afstelOvergenomenKop}</h3>
      <p class="m-0 text-klein text-gedempt" role="status">{overgenomen}</p>
      {#if fout}
        <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
      {/if}
    </div>
    <div class={rijBediening}>
      <Button variant="tweede" href={resolve('/modellen')}>{t().instellingModellenKnop}</Button>
    </div>
  </div>
{:else}
  <div class={rij}>
    <CircleCheck size={18} class="text-actie" aria-hidden="true" />
    <div class={rijLabel}>
      <h3 class={rijNaam}>{t().afstelGoedKop}</h3>
      <p class="m-0 text-klein text-gedempt" role="status">{goedHulp}</p>
      {#if fout}
        <p class="m-0 text-klein text-fout-tekst" role="alert">{fout}</p>
      {/if}
    </div>
    <div class={rijBediening}>
      <Button variant="spook" onclick={() => onSluiten?.()}>{t().afstelSluiten}</Button>
    </div>
  </div>
{/if}
