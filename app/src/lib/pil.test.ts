import { describe, expect, it } from 'vitest';
import type { Weergave } from './dictaat';
import {
  GEEN_TIJDEN,
  KLAAR_ZICHTBAAR_MS,
  MELDING_ZICHTBAAR_MS,
  VERWERKT_NA_MS,
  pilActie,
  pilToestand,
  restSeconden,
  schuifNiveau,
  staafHoogte,
  volgTijden,
  volgendeTik,
  type PilInvoer,
  type PilTijden
} from './pil';
import type { ProefResultaat, Vervolg } from './taak';

const uitslag = (tekst: string | null, uitkomst: ProefResultaat['uitkomst'] = 'tekst') =>
  ({
    taak: 7,
    uitkomst,
    tekst,
    duurMs: 1200,
    soevereiniteit: 'lokaal'
  }) satisfies ProefResultaat;

function invoer(weergave: Weergave, deel: Partial<PilInvoer> = {}): PilInvoer {
  return {
    weergave,
    plak: null,
    pilTaak: 7,
    gesloten: null,
    tijden: { taak: 7, verwerktSinds: 0, afgerondSinds: 0 },
    nu: 0,
    ...deel
  };
}

describe('welke toestand de pil toont', () => {
  it('in rust en na annuleren niets', () => {
    expect(pilToestand(invoer({ type: 'rust' })).type).toBe('verborgen');
    expect(pilToestand(invoer({ type: 'geannuleerd', taak: 7 })).type).toBe('verborgen');
  });

  it('alleen voor het dictaat van de sneltoets, niet voor de proef', () => {
    const luistert: Weergave = { type: 'luistert', taak: 7, sinds: 0 };
    expect(pilToestand(invoer(luistert)).type).toBe('luistert');
    expect(pilToestand(invoer(luistert, { pilTaak: 3 })).type).toBe('verborgen');
    expect(pilToestand(invoer(luistert, { pilTaak: null })).type).toBe('verborgen');
  });

  it('luistert met de duur, en na negen minuten met wat er nog over is', () => {
    const luistert: Weergave = { type: 'luistert', taak: 7, sinds: 1000 };
    expect(pilToestand(invoer(luistert, { nu: 5000 }))).toEqual({
      type: 'luistert',
      duurMs: 4000,
      restMs: null
    });
    expect(pilToestand(invoer(luistert, { nu: 1000 + 9 * 60_000 + 12_000 }))).toEqual({
      type: 'luistert',
      duurMs: 9 * 60_000 + 12_000,
      restMs: 48_000
    });
    expect(restSeconden(48_000)).toBe(48);
    expect(restSeconden(47_100)).toBe(48);
  });

  it('een fout blijft staan tot de gebruiker hem sluit', () => {
    const fout: Weergave = {
      type: 'fout',
      taak: 7,
      melding: 'Mondig hoort niets.',
      vervolg: 'microfooninstelling_openen'
    };
    expect(pilToestand(invoer(fout, { nu: 600_000 })).type).toBe('fout');
    expect(pilToestand(invoer(fout, { gesloten: 7 })).type).toBe('verborgen');
  });
});

describe('wanneer verwerkt verschijnt', () => {
  const verwerkt: Weergave = { type: 'verwerkt', taak: 7 };
  const tijden: PilTijden = { taak: 7, verwerktSinds: 10_000, afgerondSinds: null };

  it('pas na meer dan een seconde; tot dan de rustige tussenstand, nooit weg', () => {
    expect(pilToestand(invoer(verwerkt, { tijden, nu: 10_000 })).type).toBe('afronden');
    expect(pilToestand(invoer(verwerkt, { tijden, nu: 10_000 + VERWERKT_NA_MS })).type).toBe(
      'afronden'
    );
    expect(pilToestand(invoer(verwerkt, { tijden, nu: 10_000 + VERWERKT_NA_MS + 1 })).type).toBe(
      'verwerkt'
    );
  });

  it('de tekst is er maar het plakken loopt nog: nog steeds verwerkt', () => {
    const klaar: Weergave = { type: 'klaar', taak: 7, uitslag: uitslag('Hallo') };
    expect(pilToestand(invoer(klaar, { tijden, nu: 10_500 })).type).toBe('afronden');
    expect(pilToestand(invoer(klaar, { tijden, nu: 12_000 })).type).toBe('verwerkt');
  });

  it('wekt zichzelf op het moment dat verwerkt moet verschijnen', () => {
    expect(volgendeTik(invoer(verwerkt, { tijden, nu: 10_400 }))).toBe(601);
  });
});

describe('de timer van klaar', () => {
  const klaar: Weergave = { type: 'klaar', taak: 7, uitslag: uitslag('Hallo') };
  const tijden: PilTijden = { taak: 7, verwerktSinds: 0, afgerondSinds: 20_000 };

  it('"Geplakt" staat anderhalve seconde', () => {
    const op = (nu: number) => pilToestand(invoer(klaar, { plak: 'geplakt', tijden, nu }));
    expect(op(20_000)).toEqual({ type: 'klaar', soort: 'geplakt', duurMs: 1200 });
    expect(op(20_000 + KLAAR_ZICHTBAAR_MS - 1).type).toBe('klaar');
    expect(op(20_000 + KLAAR_ZICHTBAAR_MS).type).toBe('verborgen');
    expect(volgendeTik(invoer(klaar, { plak: 'geplakt', tijden, nu: 20_500 }))).toBe(1000);
  });

  it('op het klembord of misschien geplakt: een melding die langer staat', () => {
    const op = (plak: 'op_klembord' | 'waarschijnlijk_geplakt', nu: number) =>
      pilToestand(invoer(klaar, { plak, tijden, nu }));
    expect(op('op_klembord', 22_000)).toMatchObject({ soort: 'klembord' });
    expect(op('waarschijnlijk_geplakt', 22_000)).toMatchObject({ soort: 'waarschijnlijk' });
    expect(pilToestand(invoer(klaar, { plak: 'in_mondig', tijden, nu: 22_000 }))).toMatchObject({
      type: 'klaar',
      soort: 'in_mondig'
    });
    expect(op('op_klembord', 20_000 + MELDING_ZICHTBAAR_MS).type).toBe('verborgen');
  });

  it('geen tekst herkend: een melding, geen fout', () => {
    const leeg: Weergave = { type: 'klaar', taak: 7, uitslag: uitslag(null, 'geen_tekst') };
    expect(pilToestand(invoer(leeg, { tijden, nu: 21_000 }))).toMatchObject({
      type: 'klaar',
      soort: 'geen_tekst'
    });
  });

  it('plakken mislukt: blijft staan met Kopiëren', () => {
    const toestand = pilToestand(invoer(klaar, { plak: 'beheerrechten', tijden, nu: 99_000 }));
    expect(toestand).toEqual({ type: 'plakfout', uitkomst: 'beheerrechten' });
    expect(pilActie(toestand)).toBe('kopieren');
    expect(volgendeTik(invoer(klaar, { plak: 'beheerrechten', tijden, nu: 99_000 }))).toBeNull();
  });
});

describe('de momenten bijhouden', () => {
  it('zet elk moment één keer en begint opnieuw bij een nieuwe taak', () => {
    let tijden = volgTijden(GEEN_TIJDEN, { type: 'luistert', taak: 7, sinds: 0 }, null, 100);
    expect(tijden).toEqual({ taak: 7, verwerktSinds: null, afgerondSinds: null });
    tijden = volgTijden(tijden, { type: 'verwerkt', taak: 7 }, null, 4000);
    tijden = volgTijden(tijden, { type: 'verwerkt', taak: 7 }, null, 4500);
    expect(tijden.verwerktSinds).toBe(4000);
    const klaar: Weergave = { type: 'klaar', taak: 7, uitslag: uitslag('Hallo') };
    tijden = volgTijden(tijden, klaar, null, 4700);
    expect(tijden.afgerondSinds).toBeNull();
    tijden = volgTijden(tijden, klaar, 'geplakt', 4900);
    expect(tijden).toEqual({ taak: 7, verwerktSinds: 4000, afgerondSinds: 4900 });
    expect(volgTijden(tijden, { type: 'luistert', taak: 8, sinds: 0 }, null, 9000)).toEqual({
      taak: 8,
      verwerktSinds: null,
      afgerondSinds: null
    });
  });

  it('geeft hetzelfde object terug als er niets verandert', () => {
    const tijden = volgTijden(GEEN_TIJDEN, { type: 'verwerkt', taak: 7 }, null, 10);
    expect(volgTijden(tijden, { type: 'verwerkt', taak: 7 }, null, 20)).toBe(tijden);
  });
});

describe('de knop bij een fout', () => {
  const fout = (vervolg: Vervolg) => pilActie({ type: 'fout', melding: 'x', vervolg });
  it('één vervolgactie per soort fout', () => {
    expect(fout('microfooninstelling_openen')).toBe('microfoon');
    expect(fout('opnieuw_downloaden')).toBe('mondig');
    expect(fout('ander_model_kiezen')).toBe('mondig');
    expect(fout('opnieuw_proberen')).toBeNull();
    expect(fout('opnieuw_installeren')).toBeNull();
    expect(pilActie({ type: 'verwerkt' })).toBeNull();
  });
});

describe('de meter', () => {
  it('schuift de niveaus door en houdt ze binnen 0 tot 1', () => {
    let niveaus: number[] = [];
    for (const n of [0.1, 0.2, 0.3, 0.4, 0.5, 2, Number.NaN]) niveaus = schuifNiveau(niveaus, n);
    expect(niveaus).toEqual([0.3, 0.4, 0.5, 1, 0]);
  });

  it('staafjes van 4 tot 18 px', () => {
    expect(staafHoogte(0)).toBe(4);
    expect(staafHoogte(0.5)).toBe(11);
    expect(staafHoogte(1)).toBe(18);
    expect(staafHoogte(-1)).toBe(4);
  });
});

describe('opschonen in de pil (ADR 0027)', () => {
  const verwerkt: Weergave = { type: 'verwerkt', taak: 7 };
  const klaar: Weergave = { type: 'klaar', taak: 7, uitslag: uitslag('Hallo.') };

  it('toont tijdens het opschonen "Opschonen" met het label van de aanbieder', () => {
    const opschonen = { taak: 7, fase: { type: 'bezig', label: 'europees' } } as const;
    expect(pilToestand(invoer(verwerkt, { opschonen }))).toEqual({
      type: 'opschonen',
      label: 'europees'
    });
    // Ook al na een seconde: geen "Bezig met uitschrijven".
    expect(pilToestand(invoer(verwerkt, { opschonen, nu: 5000 })).type).toBe('opschonen');
    // Een bericht over een ander dictaat telt niet.
    const ander = { taak: 3, fase: { type: 'bezig', label: 'europees' } } as const;
    expect(pilToestand(invoer(verwerkt, { opschonen: ander })).type).toBe('afronden');
  });

  it('meldt bij terugval anderhalve seconde "niet opgeschoond" en gaat dan dicht', () => {
    const opschonen = { taak: 7, fase: { type: 'letterlijk' } } as const;
    const deel = { opschonen, plak: 'geplakt' as const };
    expect(pilToestand(invoer(klaar, { ...deel, nu: 100 }))).toEqual({
      type: 'terugval',
      soort: 'geplakt'
    });
    expect(volgendeTik(invoer(klaar, { ...deel, nu: 100 }))).toBe(KLAAR_ZICHTBAAR_MS - 100);
    expect(pilToestand(invoer(klaar, { ...deel, nu: KLAAR_ZICHTBAAR_MS })).type).toBe('verborgen');
  });

  it('toont na gelukt opschonen gewoon "Geplakt"', () => {
    const opschonen = { taak: 7, fase: { type: 'opgeschoond' } } as const;
    expect(pilToestand(invoer(klaar, { opschonen, plak: 'geplakt' })).type).toBe('klaar');
  });

  it('meldt de terugval ook als de tekst op het klembord staat of misschien geplakt is', () => {
    const opschonen = { taak: 7, fase: { type: 'letterlijk' } } as const;
    expect(pilToestand(invoer(klaar, { opschonen, plak: 'op_klembord' }))).toEqual({
      type: 'terugval',
      soort: 'klembord'
    });
    expect(pilToestand(invoer(klaar, { opschonen, plak: 'waarschijnlijk_geplakt' }))).toEqual({
      type: 'terugval',
      soort: 'waarschijnlijk'
    });
    // Die meldingen blijven langer staan, zoals zonder opschonen.
    expect(
      pilToestand(invoer(klaar, { opschonen, plak: 'op_klembord', nu: KLAAR_ZICHTBAAR_MS })).type
    ).toBe('terugval');
    expect(
      pilToestand(invoer(klaar, { opschonen, plak: 'op_klembord', nu: MELDING_ZICHTBAAR_MS })).type
    ).toBe('verborgen');
  });

  it('houdt een plakfout ook na terugval als fout', () => {
    const opschonen = { taak: 7, fase: { type: 'letterlijk' } } as const;
    expect(pilToestand(invoer(klaar, { opschonen, plak: 'mislukt' })).type).toBe('plakfout');
  });
});
