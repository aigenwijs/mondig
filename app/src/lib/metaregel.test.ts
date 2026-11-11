import { describe, expect, it } from 'vitest';
import { datumKort, duurKort } from './metaregel';

describe('duurKort', () => {
  it('schrijft seconden, minuten en uren zoals het ontwerpsysteem (§4)', () => {
    expect(duurKort(45_000)).toBe('45 s');
    expect(duurKort(0)).toBe('0 s');
    expect(duurKort(38 * 60_000)).toBe('38 min');
    expect(duurKort(60 * 60_000 + 12 * 60_000)).toBe('1 u 12 min');
    expect(duurKort(2 * 3_600_000)).toBe('2 u');
  });

  it('rondt af op hele minuten, ook aan de rand van het uur', () => {
    expect(duurKort(23 * 60_000 + 14_000)).toBe('23 min');
    expect(duurKort(59 * 60_000 + 40_000)).toBe('1 u');
    expect(duurKort(59_600)).toBe('1 min');
  });

  it('gebruikt in het Engels h voor het uur', () => {
    expect(duurKort(72 * 60_000, 'en')).toBe('1 h 12 min');
  });
});

describe('datumKort', () => {
  const nu = new Date(2026, 10, 10, 12, 0).getTime();

  it('laat het jaar weg als de datum dit jaar is', () => {
    expect(datumKort(new Date(2026, 9, 30, 23, 14).getTime(), 'nl', nu)).toBe('30 okt');
  });

  it('zet het jaar erbij als het niet dit jaar is', () => {
    expect(datumKort(new Date(2025, 9, 30).getTime(), 'nl', nu)).toBe('30 okt 2025');
  });

  it('volgt de taal', () => {
    expect(datumKort(new Date(2026, 9, 30).getTime(), 'en', nu)).toBe('30 Oct');
  });

  it('zet bij een opname de tijd erachter, ook met het jaar', () => {
    expect(datumKort(new Date(2026, 10, 2, 14, 0).getTime(), 'nl', nu, true)).toBe('2 nov, 14:00');
    expect(datumKort(new Date(2025, 9, 30, 9, 5).getTime(), 'nl', nu, true)).toBe(
      '30 okt 2025, 09:05'
    );
  });
});
