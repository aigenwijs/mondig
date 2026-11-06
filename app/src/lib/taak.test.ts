import { describe, expect, it } from 'vitest';
import { foutUitCommando, meldingTekst } from './taak';

describe('foutUitCommando', () => {
  it('neemt een foutmelding uit Rust over', () => {
    const uitRust = { melding: 'Het spraakmodel is er nog niet.', vervolg: 'opnieuw_downloaden' };
    expect(foutUitCommando(uitRust, 'terugval')).toEqual(uitRust);
  });

  it('valt terug op een vaste tekst zonder vervolg bij alles wat geen foutmelding is', () => {
    const verwacht = { melding: 'terugval', vervolg: 'geen' };
    expect(foutUitCommando('platte tekst', 'terugval')).toEqual(verwacht);
    expect(foutUitCommando(new Error('technisch'), 'terugval')).toEqual(verwacht);
    expect(foutUitCommando(null, 'terugval')).toEqual(verwacht);
    expect(foutUitCommando({ melding: 'x', vervolg: 'onbekend' }, 'terugval')).toEqual(verwacht);
  });
});

describe('meldingTekst', () => {
  it('neemt de melding van Rust over, behalve bij een niet bijgewerkte installatie', () => {
    expect(meldingTekst({ melding: 'Rust zegt iets', vervolg: 'geen' }, 'vertaald')).toBe(
      'Rust zegt iets'
    );
    expect(
      meldingTekst(
        { melding: 'Nederlandse tekst uit Rust', vervolg: 'opnieuw_installeren' },
        'vertaald'
      )
    ).toBe('vertaald');
  });

  it('herkent de nieuwe vervolgactie uit een commando', () => {
    const fout = { melding: 'x', vervolg: 'opnieuw_installeren' };
    expect(foutUitCommando(fout, 'terugval')).toEqual(fout);
  });
});
