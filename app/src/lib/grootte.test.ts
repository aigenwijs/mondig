import { describe, expect, it } from 'vitest';
import { leesbareGrootte, leesbareVoortgang } from './grootte';

describe('leesbareGrootte', () => {
  it('rondt megabytes af op hele getallen', () => {
    expect(leesbareGrootte(139_764)).toBe('0 MB');
    expect(leesbareGrootte(487_601_967)).toBe('488 MB');
    expect(leesbareGrootte(740_000_000)).toBe('740 MB');
  });

  it('schrijft gigabytes met een decimaalteken', () => {
    expect(leesbareGrootte(1_624_555_275)).toBe('1,6 GB');
    expect(leesbareGrootte(1_100_000_000)).toBe('1,1 GB');
    expect(leesbareGrootte(1_100_000_000, 'en')).toBe('1.1 GB');
  });

  it('laat de decimaal weg vanaf 100 GB', () => {
    expect(leesbareGrootte(212_000_000_000)).toBe('212 GB');
    expect(leesbareGrootte(99_900_000_000)).toBe('99,9 GB');
  });

  it('zet de grens tussen MB en GB bij een miljard bytes', () => {
    expect(leesbareGrootte(999_499_999)).toBe('999 MB');
    expect(leesbareGrootte(1_000_000_000)).toBe('1,0 GB');
  });
});

describe('leesbareVoortgang', () => {
  it('zet de eenheid alleen achter het totaal', () => {
    expect(leesbareVoortgang(310_000_000, 740_000_000)).toEqual({
      binnen: '310',
      totaal: '740 MB'
    });
  });

  it('gebruikt gigabytes voor beide getallen als het totaal een gigabyte of meer is', () => {
    expect(leesbareVoortgang(650_000_000, 1_624_555_275)).toEqual({
      binnen: '0,6',
      totaal: '1,6 GB'
    });
    expect(leesbareVoortgang(650_000_000, 1_624_555_275, 'en')).toEqual({
      binnen: '0.6',
      totaal: '1.6 GB'
    });
  });
});
