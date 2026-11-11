<script lang="ts">
  import { resolve } from '$app/paths';
  import { t } from '#lib';
  import { Button } from '#lib/components/ui/button/index.js';

  // De zin over hoe lang Mondig opnames bewaart, met "Wijzigen" naar de instelling. Eén
  // formulering voor boven "Nog uit te schrijven" en onder "Eerder opgenomen" (UI-check punt 13).
  // De termijn komt altijd uit de instelling, nooit vast uit de tekst.

  interface Props {
    /** De bewaartermijn in dagen, uit de instelling. */
    dagen: number;
  }
  let { dagen }: Props = $props();
</script>

<span
  >{t().vergaderBewaartRegel(dagen)}
  <Button
    href={`${resolve('/instellingen')}#bewaartermijn`}
    variant="spook"
    class="min-h-6 px-1 text-klein underline underline-offset-[3px]"
    >{t().vergaderBewaartWijzigen}</Button
  ></span
>
