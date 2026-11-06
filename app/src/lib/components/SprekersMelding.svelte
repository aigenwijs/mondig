<script lang="ts">
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import { huidigeTaal, t } from '#lib';
  import { Button } from '#lib/components/ui/button/index.js';
  import { Progress } from '#lib/components/ui/progress/index.js';
  import { procentVan } from '#lib/downloadrij.js';
  import { leesbareGrootte, leesbareVoortgang } from '#lib/grootte.js';
  import { modellenStand } from '#lib/modellenstand.svelte.js';
  import { SPREKERMODEL } from '#lib/sprekers.js';
  import { sprekers } from '#lib/sprekers.svelte.js';

  // Wat er onder de schakelaar "Sprekers herkennen" staat (poort uit `sprekers.ts`): de vraag of
  // het sprekermodel gedownload mag worden, daarna meteen de stand van de download (wordt
  // gestart, wacht in de rij, voortgang, nieuwe poging), na afloop een bevestiging, en een
  // eventuele fout. Staat in Instellingen en op Bestanden onder dezelfde schakelaar. De download
  // staat ook in de zijbalk.

  const taal = $derived(huidigeTaal());
  const grootte = $derived(leesbareGrootte(sprekers.model?.grootteBytes ?? 139_310_336, taal));
  const item = $derived(modellenStand.itemVoor(SPREKERMODEL));
  const fase = $derived(modellenStand.knopFase(SPREKERMODEL));
  const procent = $derived(fase === 'bezig' ? procentVan(item?.voortgang) : 0);
  const getallen = $derived(
    fase === 'bezig' && item?.voortgang
      ? leesbareVoortgang(item.voortgang.binnen, item.voortgang.totaal, taal)
      : null
  );
  const standTekst = $derived.by(() => {
    if (item?.toestand.type === 'nieuwe_poging') {
      return t().downloadNieuwePoging(item.toestand.poging, item.toestand.van);
    }
    if (fase === 'wacht') return t().sprekersWacht;
    if (getallen) {
      return t().sprekersBezigStand(t().downloadVoortgang(getallen.binnen, getallen.totaal));
    }
    if (fase === 'bezig') return t().sprekersBezig;
    return t().sprekersWordtGestart;
  });
</script>

{#if sprekers.weergave === 'vraag'}
  <div
    class="flex basis-full flex-col gap-2.5 rounded-klein bg-verhoogd p-3"
    role="group"
    aria-labelledby="sprekers-vraag-kop"
  >
    <b id="sprekers-vraag-kop" class="font-semibold">{t().sprekersVraagKop}</b>
    <span class="text-gedempt">{t().sprekersVraagTekst(grootte)}</span>
    <span class="flex flex-wrap gap-2">
      <Button onclick={() => void sprekers.download()} data-hoofdactie=""
        >{t().sprekersDownloaden(grootte)}</Button
      >
      <Button
        variant="spook"
        onclick={() => {
          sprekers.weiger();
        }}>{t().annuleren}</Button
      >
    </span>
  </div>
{:else if sprekers.weergave === 'voortgang'}
  <div class="flex basis-full flex-col gap-1.5" role="status" data-sprekers-stand={fase}>
    <span class="text-klein font-medium text-tekst">{standTekst}</span>
    <Progress value={procent} aria-label={t().downloadVoortgangNaam} class="h-2 bg-lijn" />
    <span>
      <Button
        variant="spook"
        class="text-klein"
        onclick={() => {
          sprekers.weiger();
        }}>{t().annuleren}</Button
      >
    </span>
  </div>
{:else if sprekers.weergave === 'aangezet'}
  <p class="m-0 flex basis-full items-center gap-2 text-klein text-tekst" role="status">
    <CircleCheck size={14} class="shrink-0 text-actie" aria-hidden="true" />{t().sprekersAangezet}
  </p>
{/if}
{#if sprekers.fout}
  <p class="m-0 basis-full text-klein text-fout-tekst" role="alert">{sprekers.fout}</p>
{/if}
