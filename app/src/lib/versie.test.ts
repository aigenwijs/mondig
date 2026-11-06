import { describe, expect, it } from 'vitest';
import {
  kaartBesluit,
  korteDatum,
  versieGegevensTekst,
  versieZonderAchtervoegsel,
  type VersieInfo
} from './versie';

const info: VersieInfo = {
  versie: '0.3.0-dev',
  bouw: 'afd2907',
  datum: '2026-11-01',
  systeem: 'Windows 11 (build 26200)',
  processor: 'x86-64',
  model: { id: 'parakeet-ultra-q8', naam: 'Parakeet Ultra' }
};

describe('korteDatum', () => {
  it('schrijft de maand kort en haalt de nul weg', () => {
    expect(korteDatum('2026-11-01')).toBe('1 nov 2026');
    expect(korteDatum('2027-03-15')).toBe('15 mrt 2027');
  });

  it('laat onleesbare invoer staan', () => {
    expect(korteDatum('gisteren')).toBe('gisteren');
    expect(korteDatum('2026-13-01')).toBe('2026-13-01');
  });
});

describe('versieGegevensTekst', () => {
  it('bevat versie, bouw, systeem, processor en model', () => {
    const tekst = versieGegevensTekst(info, '0.3.0-dev+afd2907 (protocol 4)');
    expect(tekst).toContain('Mondig 0.3.0-dev');
    expect(tekst).toContain('Werkproces: 0.3.0-dev+afd2907 (protocol 4)');
    expect(tekst).toContain('afd2907');
    expect(tekst).toContain('Windows 11 (build 26200), x86-64');
    expect(tekst).toContain('Parakeet Ultra');
  });

  it('meldt het als het werkproces nog niet gestart is', () => {
    expect(versieGegevensTekst(info, null)).toContain('Werkproces: nog niet gestart');
  });

  it('noemt waarop Mondig rekent, ook na een terugval', () => {
    expect(versieGegevensTekst(info, null, 'videokaart')).toContain('Rekent op: videokaart');
    expect(versieGegevensTekst(info, null, 'terugval')).toContain(
      'Rekent op: processor (de videokaart gaf een fout)'
    );
    expect(versieGegevensTekst(info, null)).toContain('Rekent op: nog geen model geladen');
  });
});

describe('kaartBesluit', () => {
  it('toont niets bij een eerste installatie, maar bewaart de versie', () => {
    expect(kaartBesluit(null, '0.3.0')).toBe('bewaar');
  });

  it('toont de kaart na een update', () => {
    expect(kaartBesluit('0.2.0', '0.3.0')).toBe('toon');
  });

  it('toont niets als de versie al gezien is', () => {
    expect(kaartBesluit('0.3.0', '0.3.0')).toBe('niets');
  });
});

describe('versieZonderAchtervoegsel', () => {
  it('haalt alleen -dev en de bouwcode weg', () => {
    expect(versieZonderAchtervoegsel('0.3.0-dev')).toBe('0.3.0');
    expect(versieZonderAchtervoegsel('0.3.0-dev+abc123')).toBe('0.3.0');
    expect(versieZonderAchtervoegsel('0.3.0')).toBe('0.3.0');
    expect(versieZonderAchtervoegsel('1.0.0-rc.1')).toBe('1.0.0-rc.1');
    expect(versieZonderAchtervoegsel('1.0.0-rc.1+abc123')).toBe('1.0.0-rc.1');
  });
});
