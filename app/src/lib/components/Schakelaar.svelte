<script lang="ts">
  import { Switch } from 'bits-ui';

  // De aan-uitschakelaar van de instellingen in de stijl van ADR 0014, gelijk aan die van
  // "Alleen op het klembord zetten".
  let {
    aan,
    wijzig,
    labelledby,
    describedby,
    disabled = false,
    bestuurd = false
  }: {
    aan: boolean;
    wijzig: (nieuw: boolean) => void;
    labelledby: string;
    describedby: string;
    /** Uitgegrijsd en niet te bedienen; de reden staat in de beschrijving. */
    disabled?: boolean;
    /**
     * De schakelaar toont alleen wat `aan` zegt, en klapt niet zelf om bij een klik. Voor een
     * schakelaar die pas aangaat als er eerst iets gebeurt (de download van een model): zegt
     * `wijzig` dat het nog niet kan, dan blijft hij uit.
     */
    bestuurd?: boolean;
  } = $props();

  const wijzigAan = (nieuw: boolean): void => {
    wijzig(nieuw);
  };

  // 44 x 24 px met een rand in het veldtoken: de uit-baan houdt zo minstens 3:1 tegen elk vlak.
  // De ::before maakt het klikgebied 32 px hoog (22 px binnen de rand plus 2 x 5) zonder dat de
  // schakelaar groter oogt.
  const baan =
    "relative inline-flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full border border-rand-veld bg-hover p-[2px] transition-colors duration-150 before:absolute before:inset-x-0 before:-inset-y-[5px] before:content-[''] data-[state=checked]:border-actie data-[state=checked]:bg-actie data-[disabled]:cursor-not-allowed data-[disabled]:opacity-50";
</script>

{#snippet duim()}
  <Switch.Thumb
    class="pointer-events-none block size-[18px] rounded-full bg-tekst transition-transform duration-150 data-[state=checked]:translate-x-5 data-[state=checked]:bg-op-actie"
  />
{/snippet}

{#if bestuurd}
  <Switch.Root
    bind:checked={() => aan, wijzigAan}
    aria-labelledby={labelledby}
    aria-describedby={describedby}
    {disabled}
    class={baan}
  >
    {@render duim()}
  </Switch.Root>
{:else}
  <Switch.Root
    checked={aan}
    onCheckedChange={wijzig}
    aria-labelledby={labelledby}
    aria-describedby={describedby}
    {disabled}
    class={baan}
  >
    {@render duim()}
  </Switch.Root>
{/if}
