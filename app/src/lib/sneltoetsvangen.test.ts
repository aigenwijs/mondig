import { describe, expect, it } from 'vitest';
import {
  LEGE_VANG,
  STANDAARD_SNELTOETS,
  bouwToets,
  drukIn,
  isAfgerond,
  labelVanToets,
  laatLos,
  liveLabel,
  toetsNaam
} from './sneltoetsvangen';

describe('toetsnamen in gewone taal', () => {
  it('gebruikt Nederlandse namen', () => {
    expect(toetsNaam('Space')).toBe('Spatie');
    expect(toetsNaam('ControlRight')).toBe('Ctrl');
    expect(toetsNaam('AltLeft')).toBe('Alt');
    expect(toetsNaam('ShiftLeft')).toBe('Shift');
    expect(toetsNaam('KeyD')).toBe('D');
    expect(toetsNaam('Digit7')).toBe('7');
    expect(toetsNaam('Numpad2')).toBe('Num 2');
    expect(toetsNaam('ArrowLeft')).toBe('Pijl links');
    expect(toetsNaam('Escape')).toBe('Esc');
  });

  it('vertaalt een sneltoets van de plugin', () => {
    expect(labelVanToets('Ctrl+Space')).toBe('Ctrl+Spatie');
    expect(labelVanToets('Ctrl+Alt+Shift+KeyD')).toBe('Ctrl+Alt+Shift+D');
    expect(labelVanToets(STANDAARD_SNELTOETS)).toBe('Ctrl+Spatie');
  });
});

describe('live label', () => {
  it('zet modifiers voorop en eindigt op een plus zolang er geen gewone toets is', () => {
    expect(liveLabel([])).toBe('');
    expect(liveLabel(['ControlLeft'])).toBe('Ctrl+');
    expect(liveLabel(['AltLeft', 'ControlLeft'])).toBe('Ctrl+Alt+');
    expect(liveLabel(['ShiftLeft', 'ControlLeft', 'KeyD'])).toBe('Ctrl+Shift+D');
    expect(liveLabel(['Space'])).toBe('Spatie');
  });

  it('telt links en rechts van dezelfde modifier één keer', () => {
    expect(liveLabel(['ControlLeft', 'ControlRight'])).toBe('Ctrl+');
  });
});

describe('vangen van toetsen', () => {
  it('legt de combinatie vast zodra alle toetsen los zijn', () => {
    let stand = LEGE_VANG;
    stand = drukIn(stand, 'ControlLeft');
    stand = drukIn(stand, 'Space');
    expect(isAfgerond(stand)).toBe(false);
    stand = laatLos(stand, 'Space');
    expect(isAfgerond(stand)).toBe(false);
    stand = laatLos(stand, 'ControlLeft');
    expect(isAfgerond(stand)).toBe(true);
    expect(stand.piek).toEqual(['ControlLeft', 'Space']);
  });

  it('negeert herhaalde toetsdrukken van het toetsenbord', () => {
    let stand = drukIn(LEGE_VANG, 'ControlLeft');
    const zelfde = drukIn(stand, 'ControlLeft');
    expect(zelfde).toBe(stand);
    stand = laatLos(stand, 'KeyZ');
    expect(stand.vast).toEqual(['ControlLeft']);
  });

  it('onthoudt de grootste combinatie ook als een modifier eerder loslaat', () => {
    let stand = LEGE_VANG;
    for (const code of ['ControlLeft', 'AltLeft', 'Space']) stand = drukIn(stand, code);
    stand = laatLos(stand, 'AltLeft');
    stand = laatLos(stand, 'Space');
    stand = laatLos(stand, 'ControlLeft');
    expect(stand.piek).toEqual(['ControlLeft', 'AltLeft', 'Space']);
  });

  it('is niet afgerond zonder dat er iets ingedrukt is', () => {
    expect(isAfgerond(LEGE_VANG)).toBe(false);
  });
});

describe('bouwToets', () => {
  it('maakt de standaardtoets', () => {
    expect(bouwToets(['ControlLeft', 'Space'])).toBe('Ctrl+Space');
  });

  it('zet de modifiers in vaste volgorde, wat de gebruiker ook eerst indrukte', () => {
    expect(bouwToets(['KeyD', 'ShiftRight', 'AltLeft', 'ControlRight'])).toBe(
      'Ctrl+Alt+Shift+KeyD'
    );
  });

  it('telt links en rechts van dezelfde modifier een keer', () => {
    expect(bouwToets(['ControlLeft', 'ControlRight', 'F9'])).toBe('Ctrl+F9');
  });

  it('geeft een losse toets zonder modifiers door', () => {
    expect(bouwToets(['Insert'])).toBe('Insert');
    expect(bouwToets(['ScrollLock'])).toBe('ScrollLock');
  });

  it('laat Rust oordelen: Windows-toets, toetsen die de plugin niet kent en meerdere toetsen gaan mee', () => {
    expect(bouwToets(['MetaLeft', 'ControlLeft', 'Space'])).toBe('Super+Ctrl+Space');
    expect(bouwToets(['OSRight', 'KeyD'])).toBe('Super+KeyD');
    expect(bouwToets(['AltLeft', 'Tab'])).toBe('Alt+Tab');
    expect(bouwToets(['ControlLeft', 'KeyA', 'KeyB'])).toBe('Ctrl+KeyA+KeyB');
    expect(bouwToets(['ControlLeft', 'AltLeft'])).toBe('Ctrl+Alt');
    expect(bouwToets([])).toBe('');
  });
});

describe('labels', () => {
  it('schrijft Scroll Lock met een spatie, zoals Rust', () => {
    expect(toetsNaam('ScrollLock')).toBe('Scroll Lock');
    expect(labelVanToets('Ctrl+ScrollLock')).toBe('Ctrl+Scroll Lock');
  });

  it('laat functietoetsen en Insert staan', () => {
    expect(labelVanToets('F13')).toBe('F13');
    expect(labelVanToets('Insert')).toBe('Insert');
  });
});
