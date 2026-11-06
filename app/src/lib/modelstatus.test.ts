import { beforeEach, describe, expect, it, vi } from 'vitest';

const kern = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => kern);

import { haalModelStatus } from './modelstatus';

describe('haalModelStatus', () => {
  beforeEach(() => {
    kern.invoke.mockReset();
    kern.isTauri.mockReset();
  });

  it('geeft buiten de app een vaste terugval zonder het commando aan te roepen', async () => {
    kern.isTauri.mockReturnValue(false);
    const status = await haalModelStatus();
    expect(status.status).toBe('ontbreekt');
    expect(kern.invoke).not.toHaveBeenCalled();
  });

  it('geeft in de app het antwoord van het commando door', async () => {
    kern.isTauri.mockReturnValue(true);
    const antwoord = { id: 'x', status: 'compleet', grootteBytes: 1, aanwezigBytes: 1 };
    kern.invoke.mockResolvedValue(antwoord);
    expect(await haalModelStatus()).toBe(antwoord);
    expect(kern.invoke).toHaveBeenCalledWith('model_status');
  });

  it('laat in de app een fout door in plaats van "ontbreekt" te verzinnen', async () => {
    kern.isTauri.mockReturnValue(true);
    kern.invoke.mockRejectedValue('Er is geen gegevensmap gevonden op deze computer.');
    await expect(haalModelStatus()).rejects.toBe(
      'Er is geen gegevensmap gevonden op deze computer.'
    );
  });
});
