import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import type { AdviesRegel } from '../afstellen';
import AfstellenRij from './AfstellenRij.svelte';

const regels: AdviesRegel[] = [
  {
    sleutel: 'kaart',
    titel: 'Videokaart gebruiken',
    uitleg: 'Uitschrijven gaat ongeveer 3 keer zo snel.',
    invoer: { soort: 'videokaart' }
  },
  {
    sleutel: 'bestanden',
    titel: 'Nauwkeurig model voor opnames en bestanden',
    uitleg: 'Minder fouten in verslagen. Downloaden: 1,1 GB.',
    invoer: { soort: 'model', gebruik: 'bestanden', model: 'canary-1b-v2-q8' }
  },
  {
    sleutel: 'dicteren',
    titel: 'Dicteren blijft zoals het is',
    uitleg: 'Parakeet Ultra past al het beste.',
    invoer: null
  }
];

describe('Mondig opnieuw afstellen: de vijf toestanden', () => {
  it('rust: een rij met de knop Afstellen', () => {
    const { body } = render(AfstellenRij, { props: { toestand: 'rust' } });
    expect(body).toContain('Mondig opnieuw afstellen');
    expect(body).toContain('>Afstellen<');
    expect(body).not.toContain('Stoppen');
  });

  it('rust na Stoppen: zegt dat de test op de achtergrond afrondt', () => {
    const { body } = render(AfstellenRij, {
      props: { toestand: 'rust', melding: 'Gestopt. De snelheidstest rondt af.' }
    });
    expect(body).toContain('Gestopt. De snelheidstest rondt af.');
  });

  it('bezig: de stappen en Stoppen', () => {
    const { body } = render(AfstellenRij, { props: { toestand: 'bezig', stap: 'kaart' } });
    expect(body).toContain('Mondig bekijkt je computer');
    expect(body).toContain('Dat duurt ongeveer een minuut. Je kunt gewoon verder werken.');
    expect(body).toContain('Processor en geheugen');
    expect(body).toContain('Videokaart testen');
    expect(body).toContain('Modellen vergelijken');
    expect(body).toContain('>Stoppen<');
    expect(body).not.toContain('>Afstellen<');
  });

  it('bezig zonder videokaartproef: de stap Videokaart testen staat er niet', () => {
    const { body } = render(AfstellenRij, {
      props: { toestand: 'bezig', stap: 'modellen', toonKaartStap: false }
    });
    expect(body).not.toContain('Videokaart testen');
    expect(body).toContain('Modellen vergelijken');
  });

  it('advies: verbeteringen met een vinkje, wat al goed staat zonder, en twee knoppen', () => {
    const { body } = render(AfstellenRij, {
      props: {
        toestand: 'advies',
        regels,
        computerZin: '16 GB werkgeheugen, 8 processorkernen'
      }
    });
    expect(body).toContain('Dit past beter bij deze computer');
    expect(body).toContain('We keken naar je computer: 16 GB werkgeheugen, 8 processorkernen.');
    expect(body).toContain('Videokaart gebruiken');
    expect(body).toContain('Minder fouten in verslagen. Downloaden: 1,1 GB.');
    // Twee vinkvakken, allebei aan; de derde regel staat er zonder vinkvak.
    expect((body.match(/role="checkbox"/g) ?? []).length).toBe(2);
    expect((body.match(/aria-checked="true"/g) ?? []).length).toBe(2);
    expect(body).toContain('Dicteren blijft zoals het is');
    expect(body).toContain('>Advies overnemen<');
    expect(body).toContain('>Niet nu<');
    expect(body).not.toContain('Vink minstens één punt aan');
  });

  it('advies: staat elk vinkje uit, dan is Advies overnemen uit en zegt de rij waarom', () => {
    const { body } = render(AfstellenRij, {
      props: { toestand: 'advies', regels, uit: ['kaart', 'bestanden'] }
    });
    expect((body.match(/aria-checked="false"/g) ?? []).length).toBe(2);
    expect(body).toContain('Vink minstens één punt aan om over te nemen.');
    expect(body).toMatch(/aria-disabled="true"[^>]*>(<!---->)?Advies overnemen/);
  });

  it('advies: een mislukte overname laat het advies staan met de reden', () => {
    const { body } = render(AfstellenRij, {
      props: { toestand: 'advies', regels, fout: 'Er is te weinig ruimte op de schijf.' }
    });
    expect(body).toContain('Er is te weinig ruimte op de schijf.');
    expect(body).toContain('>Advies overnemen<');
  });

  it('overgenomen: zegt wat er gebeurd is en verwijst naar Modellen', () => {
    const { body } = render(AfstellenRij, {
      props: {
        toestand: 'overgenomen',
        overgenomen: 'De videokaart staat aan. Canary wordt gedownload.'
      }
    });
    expect(body).toContain('Overgenomen');
    expect(body).toContain('De videokaart staat aan. Canary wordt gedownload.');
    expect(body).toContain('Naar Modellen');
    expect(body).toContain('href="/modellen"');
  });

  it('alles goed: noemt de datum van de controle en heeft Sluiten', () => {
    const { body } = render(AfstellenRij, {
      props: {
        toestand: 'goed',
        goedHulp:
          'Mondig keek naar je computer en vond niets dat beter kan. Gecontroleerd op 4 november.'
      }
    });
    expect(body).toContain('Alles staat al goed');
    expect(body).toContain('Gecontroleerd op 4 november.');
    expect(body).toContain('>Sluiten<');
  });

  it('advies met alleen een open punt (videokaart niet getest): geen Advies overnemen, wel Sluiten', () => {
    const { body } = render(AfstellenRij, {
      props: {
        toestand: 'advies',
        regels: [
          {
            sleutel: 'kaart',
            titel: 'De videokaart is niet getest',
            uitleg: 'Mondig kon de videokaart niet testen: fout. Probeer het later opnieuw.',
            invoer: null,
            probleem: true
          }
        ]
      }
    });
    expect(body).toContain('De videokaart is niet getest');
    expect(body).not.toContain('Advies overnemen');
    expect(body).not.toContain('Vink minstens');
    expect(body).toContain('>Sluiten<');
  });

  it('advies: te weinig ruimte zet Advies overnemen uit en zegt waarom', () => {
    const { body } = render(AfstellenRij, {
      props: {
        toestand: 'advies',
        regels,
        ruimteMelding:
          'Er is niet genoeg ruimte voor beide modellen; maak ruimte vrij of kies er één.'
      }
    });
    expect(body).toContain('Er is niet genoeg ruimte voor beide modellen');
    expect(body).toMatch(/aria-disabled="true"[^>]*>(<!---->)?Advies overnemen/);
  });

  it('toont de fout in elke toestand', () => {
    for (const toestand of ['rust', 'overgenomen', 'goed'] as const) {
      const { body } = render(AfstellenRij, { props: { toestand, fout: 'Iets ging mis.' } });
      expect(body, toestand).toContain('Iets ging mis.');
    }
  });

  it('rust: Afstellen is uit zolang een gestopte snelheidstest nog afrondt, met de reden', () => {
    const { body } = render(AfstellenRij, {
      props: { toestand: 'rust', afstellenUitReden: 'De vorige snelheidstest rondt Mondig nog af.' }
    });
    expect(body).toContain('De vorige snelheidstest rondt Mondig nog af.');
    expect(body).toMatch(/aria-disabled="true"[^>]*>(<!---->)?Afstellen/);
  });
});
