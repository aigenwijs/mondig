<script lang="ts">
  import { tick } from 'svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import { t } from '#lib';

  // "Lijst met maat" (ontwerpsysteem §5): onder een lijst die eerst een stuk toont, de knop
  // "Meer tonen" links en de telling "10 van 37" rechts. Geen knop en geen telling als alles
  // zichtbaar is. Na het laden van meer blijft de focus op de eerste nieuwe rij.
  // Dit onderdeel is alleen de voet: de ouder tekent de rijen en weet hoe er meer komt (uit Rust
  // laden, of een groter deel van een lijst die al in het geheugen zit). De ouder geeft de
  // aantallen en `meer`; deze voet doet het wachten, de focus en de foutafhandeling.

  interface Props {
    /** Hoeveel rijen er nu staan. */
    getoond: number;
    /** Hoeveel rijen er in totaal zijn. */
    totaal: number;
    /** Laadt of onthult het volgende stuk. Moet klaar zijn als de belofte vervuld is. */
    meer: () => Promise<void>;
    /** De rij met dit volgnummer (vanaf 0), voor de focus. Mag `null` geven. */
    rij: (index: number) => HTMLElement | null | undefined;
    /** Het id van de knop, voor tests en voor de ouder. */
    id?: string;
    /** Meldt voor de schermlezer hoeveel rijen erbij kwamen. */
    bijErbij?: (aantal: number) => void;
    /** Meldt een mislukte `meer`; de focus blijft dan op de knop. */
    bijFout?: (fout: unknown) => void;
    /**
     * Een eigen knoptekst in plaats van "Meer tonen", bijvoorbeeld "Alle 6 tonen" voor een lijst
     * die in één keer alles laat zien. Met een label verdwijnt ook de telling: "Alle 6 tonen"
     * zegt het totaal al.
     */
    label?: string;
  }
  let { getoond, totaal, meer, rij, id = 'lijst-meer', bijErbij, bijFout, label }: Props = $props();

  async function toon(): Promise<void> {
    const vorig = getoond;
    try {
      await meer();
    } catch (e) {
      bijFout?.(e);
      return;
    }
    await tick();
    const erbij = getoond - vorig;
    if (erbij > 0) bijErbij?.(erbij);
    rij(vorig)?.focus();
  }
</script>

{#if totaal > getoond}
  <div class="flex items-center gap-3 px-2 pt-1 pb-1.5">
    <Button {id} variant="spook" onclick={toon}>{label ?? t().meerTonen}</Button>
    {#if label === undefined}
      <span class="ml-auto pr-2 text-klein text-dim tabular-nums"
        >{t().lijstTelling(getoond, totaal)}</span
      >
    {/if}
  </div>
{/if}
