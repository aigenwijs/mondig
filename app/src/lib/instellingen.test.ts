import { describe, expect, it } from 'vitest';
import {
  RUSTTIJD_SLEUTELS,
  STANDAARD_RUSTTIJD,
  gekozenWaarde,
  isRusttijdSleutel,
  meterBreedte,
  microfoonIsWeg,
  volumeVeilig,
  zelfdeLijst,
  type MicrofoonLijst
} from './instellingen';

const lijst = (opgeslagen: string | null, aanwezig: boolean): MicrofoonLijst => ({
  microfoons: [
    { id: 'a', naam: 'AKG USB-headset' },
    { id: 'b', naam: 'Microfoon (Realtek)' }
  ],
  standaardNaam: 'AKG USB-headset',
  opgeslagen,
  opgeslagenAanwezig: aanwezig
});

describe('microfoonkeuze', () => {
  it('kiest de standaard als er niets bewaard is', () => {
    expect(gekozenWaarde(lijst(null, false))).toBe('');
    expect(microfoonIsWeg(lijst(null, false))).toBe(false);
  });

  it('kiest de bewaarde microfoon als die er nog is', () => {
    expect(gekozenWaarde(lijst('b', true))).toBe('b');
    expect(microfoonIsWeg(lijst('b', true))).toBe(false);
  });

  it('valt terug op de standaard met uitleg als de bewaarde microfoon weg is', () => {
    expect(gekozenWaarde(lijst('b', false))).toBe('');
    expect(microfoonIsWeg(lijst('b', false))).toBe(true);
  });

  it('herkent een ongewijzigde lijst', () => {
    expect(zelfdeLijst(lijst('a', true), lijst('a', true))).toBe(true);
    expect(zelfdeLijst(lijst('a', true), lijst('b', true))).toBe(false);
  });
});

describe('meterbalk', () => {
  it('houdt de meter binnen de balk', () => {
    expect(meterBreedte(-0.5)).toBe(0);
    expect(meterBreedte(0.4)).toBe(0.4);
    expect(meterBreedte(7)).toBe(1);
    expect(meterBreedte(Number.NaN)).toBe(0);
  });
});

describe('rusttijd', () => {
  it('kent vier keuzes en de standaard is er één van', () => {
    expect(RUSTTIJD_SLEUTELS).toEqual(['5m', '15m', '1u', 'nooit']);
    expect(RUSTTIJD_SLEUTELS).toContain(STANDAARD_RUSTTIJD);
    expect(STANDAARD_RUSTTIJD).toBe('5m');
  });

  it('herkent alleen bekende sleutels, "nooit" is de keuze Altijd', () => {
    expect(isRusttijdSleutel('nooit')).toBe(true);
    expect(isRusttijdSleutel('7m')).toBe(false);
    expect(isRusttijdSleutel('meteen')).toBe(false);
  });
});

describe('volume van het geluidje', () => {
  it('blijft binnen 0 tot 100 in hele stappen', () => {
    expect(volumeVeilig(42.6)).toBe(43);
    expect(volumeVeilig(-5)).toBe(0);
    expect(volumeVeilig(250)).toBe(100);
    expect(volumeVeilig(Number.NaN)).toBe(0);
  });
});
