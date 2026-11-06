import { describe, expect, it } from 'vitest';
import { cn, tv } from './utils';

// tailwind-merge kent de eigen tekstmaten uit app.css niet vanzelf en hield `text-klein` voor
// een kleur. Dan verdween de tekstkleur van een knop zodra er een maat bij kwam (wit op groen).
describe('klassen samenvoegen', () => {
  it('houdt een tekstkleur naast een eigen tekstmaat', () => {
    expect(cn('text-op-actie', 'text-midden').split(' ')).toEqual(['text-op-actie', 'text-midden']);
    expect(cn('text-gedempt text-basis', 'text-klein').split(' ')).toEqual([
      'text-gedempt',
      'text-klein'
    ]);
  });

  it('laat een latere tekstmaat de eerdere vervangen', () => {
    expect(cn('text-basis', 'text-klein')).toBe('text-klein');
  });

  it('doet hetzelfde in varianten van tailwind-variants', () => {
    const knop = tv({
      base: 'text-basis',
      variants: { variant: { primair: 'text-op-actie' }, size: { groot: 'text-midden' } }
    });
    const klassen = knop({ variant: 'primair', size: 'groot' }).split(' ');
    expect(klassen).toContain('text-op-actie');
    expect(klassen).toContain('text-midden');
    expect(klassen).not.toContain('text-basis');
  });
});
