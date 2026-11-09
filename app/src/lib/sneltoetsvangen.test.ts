import { describe, expect, it } from 'vitest';
import {
  LEGE_VANG,
  bouwToets,
  drukIn,
  isAfgerond,
  labelVanToets,
  labelVoorlezen,
  laatLos,
  liveLabel,
  liveVoorlezen,
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
    expect(labelVanToets('Ctrl+Space', 'windows')).toBe('Ctrl+Spatie');
    expect(labelVanToets('Ctrl+Space', 'linux')).toBe('Ctrl+Spatie');
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

// De Mac: symbolen in de volgorde van Apple direct voor de toets, voluit voor schermlezers.
// Dezelfde uitkomsten als `toets_label` in mondig-core.
describe('labels op de Mac', () => {
  it('zet de modifiers als symbolen direct voor de toets', () => {
    expect(labelVanToets('Alt+Space', 'mac')).toBe('\u2325Spatie');
    expect(labelVanToets('Alt+Shift+Space', 'mac')).toBe('\u2325\u21E7Spatie');
    expect(labelVanToets('Ctrl+Super+KeyD', 'mac')).toBe('\u2303\u2318D');
    expect(labelVanToets('Super+Shift+Alt+Ctrl+KeyD', 'mac')).toBe('\u2303\u2325\u21E7\u2318D');
    expect(labelVanToets('Alt+ArrowUp', 'mac')).toBe('\u2325Pijl omhoog');
    expect(labelVanToets('F13', 'mac')).toBe('F13');
  });

  it('noemt de Windows-toets op de Mac Command', () => {
    expect(toetsNaam('MetaLeft', 'mac')).toBe('Command');
    expect(toetsNaam('MetaLeft', 'windows')).toBe('Windows');
    expect(toetsNaam('MetaRight')).toBe('Windows');
  });

  it('leest de schermlezer voluit voor', () => {
    expect(labelVoorlezen('Alt+Space', 'mac')).toBe('Option Spatie');
    expect(labelVoorlezen('Ctrl+Super+KeyD', 'mac')).toBe('Control Command D');
    expect(labelVoorlezen('Ctrl+Space', 'windows')).toBe('Ctrl+Spatie');
  });

  it('toont het live label met symbolen, zonder plus', () => {
    expect(liveLabel(['AltLeft'], 'mac')).toBe('\u2325');
    expect(liveLabel(['AltLeft', 'ControlLeft'], 'mac')).toBe('\u2303\u2325');
    expect(liveLabel(['MetaLeft', 'ShiftLeft', 'KeyD'], 'mac')).toBe('\u21E7\u2318D');
    expect(liveLabel(['AltRight', 'Space'], 'mac')).toBe('\u2325Spatie');
    expect(liveLabel([], 'mac')).toBe('');
    // Links en rechts van dezelfde modifier tellen een keer.
    expect(liveLabel(['AltLeft', 'AltRight'], 'mac')).toBe('\u2325');
  });

  it('leest het live label voluit voor', () => {
    expect(liveVoorlezen(['AltLeft'], 'mac')).toBe('Option');
    expect(liveVoorlezen(['MetaLeft', 'ControlLeft', 'KeyD'], 'mac')).toBe('Control Command D');
    expect(liveVoorlezen(['ControlLeft'], 'windows')).toBe('Ctrl+');
  });

  it('laat Windows en het vangen op de Mac dezelfde plugintoets bouwen', () => {
    // Cmd is `Super` voor de plugin; Rust beslist of het mag.
    expect(bouwToets(['MetaLeft', 'AltLeft', 'KeyD'])).toBe('Super+Alt+KeyD');
  });
});
