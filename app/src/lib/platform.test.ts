import { beforeEach, describe, expect, it, vi } from 'vitest';

const kern = vi.hoisted(() => ({ info: vi.fn() }));
vi.mock('./sneltoets', () => ({ haalSneltoetsInfo: kern.info }));

import { haalPlatform, vergeetPlatform } from './platform';

describe('haalPlatform', () => {
  beforeEach(() => {
    kern.info.mockReset();
    vergeetPlatform();
  });

  it('vraagt Rust één keer, ook bij meerdere aanroepen', async () => {
    kern.info.mockResolvedValue({ platform: 'mac' });
    const [a, b] = await Promise.all([haalPlatform(), haalPlatform()]);
    expect([a, b]).toEqual(['mac', 'mac']);
    await haalPlatform();
    expect(kern.info).toHaveBeenCalledTimes(1);
  });

  it('probeert het na een mislukte vraag opnieuw', async () => {
    kern.info.mockRejectedValueOnce(new Error('weg')).mockResolvedValue({ platform: 'windows' });
    expect(await haalPlatform()).toBeNull();
    expect(await haalPlatform()).toBe('windows');
    expect(kern.info).toHaveBeenCalledTimes(2);
  });

  it('geeft buiten de app (geen info) null', async () => {
    kern.info.mockResolvedValue(null);
    expect(await haalPlatform()).toBeNull();
  });
});
