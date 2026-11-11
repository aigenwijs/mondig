import { describe, expect, it } from 'vitest';
import { eenmalig } from './eenmalig';

describe('eenmalig', () => {
  it('negeert een tweede aanroep zolang de eerste loopt', async () => {
    let aantal = 0;
    let klaar!: () => void;
    const wacht = new Promise<void>((r) => {
      klaar = r;
    });
    const stand: boolean[] = [];
    const doe = eenmalig(
      async () => {
        aantal += 1;
        await wacht;
      },
      (b) => stand.push(b)
    );
    const eerste = doe();
    const tweede = doe();
    klaar();
    await Promise.all([eerste, tweede]);
    expect(aantal).toBe(1);
    expect(stand).toEqual([true, false]);
  });

  it('werkt daarna weer, ook als de actie faalde', async () => {
    let aantal = 0;
    const doe = eenmalig(() => {
      aantal += 1;
      return Promise.reject(new Error('mislukt'));
    });
    await expect(doe()).rejects.toThrow('mislukt');
    await expect(doe()).rejects.toThrow('mislukt');
    expect(aantal).toBe(2);
  });
});
