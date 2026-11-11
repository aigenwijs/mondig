<script lang="ts">
  import { tick } from 'svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';
  import { eenmalig } from '#lib/eenmalig.js';
  import { vlak } from '#lib/stijl.js';

  // "Verwijderen met bevestiging" (ontwerpsysteem §5): de knop "Verwijderen…", dan een vraag
  // "… verwijderen?", een zin over wat er verdwijnt en wat blijft, en de knoppen "Verwijderen" en
  // "Annuleren". Verwijderen is een gewone knop, geen rode (ontwerpsysteem §2).
  // Focus: bij het openen op Annuleren (de veilige keuze); na Annuleren, en na het verwijderen
  // als het scherm blijft staan, terug op de knop waarmee je begon.
  // De vraag kan ook van buiten open gaan (bijvoorbeeld vanuit een menu): zet dan `knop` uit,
  // bind `open` en geef met `focusTerug` het element dat de focus krijgt als de vraag sluit.

  interface Props {
    /** De vraag, bijvoorbeeld "Uitschrift verwijderen?". */
    vraag: string;
    /** Wat er verdwijnt en wat blijft ("Wat je als bestand hebt opgeslagen, blijft in je map."). */
    uitleg: string;
    /** Wordt aangeroepen bij Verwijderen. Fouten handelt de ouder zelf af. */
    verwijder: () => Promise<void>;
    /** Staat de vraag open? Bindbaar, voor als de ouder hem opent of sluit. */
    open?: boolean;
    /** Toon de knop "Verwijderen…". Uit als de vraag van buiten opent. */
    knop?: boolean;
    /** De tekst van de knop; standaard "Verwijderen…". */
    knopTekst?: string;
    /** Een aria-label voor de knop, als de tekst alleen niet genoeg zegt. */
    knopAria?: string;
    /** De tekst van de bevestigknop; standaard "Verwijderen". */
    bevestigTekst?: string;
    /** Een ouder die eerst iets moet ophalen: geeft `false` om niet te openen. */
    voorOpen?: () => Promise<boolean>;
    /** Loopt er al iets? Dan doet Verwijderen niets (aria-disabled, blijft bereikbaar). */
    bezig?: boolean;
    /** Het element dat de focus krijgt als de vraag sluit; standaard de knop. */
    focusTerug?: () => HTMLElement | null | undefined;
    /** Voorvoegsel voor de ids, zodat de ouder en de tests het kunnen vinden. */
    id?: string;
    /** Extra klassen voor de knop "Verwijderen…". */
    knopKlasse?: string;
  }
  let {
    vraag,
    uitleg,
    verwijder,
    open = $bindable(false),
    knop = true,
    knopTekst,
    knopAria,
    bevestigTekst,
    voorOpen,
    bezig = false,
    focusTerug,
    id = 'verwijder',
    knopKlasse
  }: Props = $props();

  let knopRef = $state<HTMLElement | null>(null);
  // Aan zolang de bevestig-actie loopt: een tweede klik doet dan niets.
  let bezigIntern = $state(false);
  const geblokkeerd = $derived(bezig || bezigIntern);

  // Opent de vraag van buiten (open gaat naar waar): de focus naar Annuleren.
  let vorigOpen = false;
  $effect(() => {
    if (open && !vorigOpen) {
      void tick().then(() => document.getElementById(`${id}-annuleer`)?.focus());
    }
    vorigOpen = open;
  });

  function terug(): void {
    (focusTerug?.() ?? knopRef)?.focus();
  }

  async function vraagOpen(): Promise<void> {
    if (voorOpen && !(await voorOpen())) return;
    open = true;
  }

  async function annuleer(): Promise<void> {
    open = false;
    await tick();
    terug();
  }

  // Eén keer tegelijk: een dubbelklik roept `verwijder` niet twee keer aan.
  const bevestig = eenmalig(
    async () => {
      if (bezig) return;
      await verwijder();
      open = false;
      await tick();
      terug();
    },
    (b) => {
      bezigIntern = b;
    }
  );
</script>

{#if open}
  <div
    class="{vlak} flex w-full flex-col gap-2.5 px-4 py-3.5"
    role="alertdialog"
    aria-labelledby="{id}-kop"
    aria-describedby="{id}-uitleg"
  >
    <b id="{id}-kop" class="font-semibold">{vraag}</b>
    <span id="{id}-uitleg" class="break-words text-gedempt empty:hidden">{uitleg}</span>
    <div class="flex flex-wrap gap-2">
      <Button variant="tweede" aria-disabled={geblokkeerd} onclick={bevestig}
        >{bevestigTekst ?? t().verwijderBevestig}</Button
      >
      <Button id="{id}-annuleer" variant="spook" onclick={annuleer}>{t().annuleren}</Button>
    </div>
  </div>
{:else if knop}
  <Button
    variant="tweede"
    class={knopKlasse}
    aria-label={knopAria}
    bind:ref={knopRef}
    onclick={vraagOpen}>{knopTekst ?? t().verwijderenKnop}</Button
  >
{/if}
