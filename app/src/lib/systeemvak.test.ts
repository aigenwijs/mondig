import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { t, zetTaal } from '#lib';

const kern = vi.hoisted(() => ({
  invoke: vi.fn((): Promise<unknown> => Promise.resolve(null)),
  isTauri: vi.fn(() => false)
}));
vi.mock('@tauri-apps/api/core', () => kern);

import { systeemvakZetLabels, systeemvaklabels } from './systeemvak';

// Het menu van het systeemvak volgt de taal van de app (UI-D): de interface geeft de teksten aan
// Rust. De Engelse teksten volgen de woordenlijst (ontwerpsysteem §3).

describe('de teksten van het systeemvak', () => {
  afterEach(() => {
    zetTaal('nl');
  });

  it('zijn Nederlands, zoals Rust ze zonder doorgeven al heeft', () => {
    expect(systeemvaklabels(t())).toEqual({
      openen: 'Mondig openen',
      opnameStarten: 'Opname starten',
      opnameStartenEerstAfmaken: 'Opname starten (eerst het verslag afmaken)',
      opnameLoopt: 'Opname loopt',
      stoppen: 'Stoppen en uitschrijven',
      kopieer: 'Laatste dictaat kopiëren',
      map: 'Map openen',
      instellingen: 'Instellingen',
      over: 'Over Mondig',
      afsluiten: 'Afsluiten',
      tipOnline: 'Mondig neemt een vergadering op ({tijd})',
      tipRuimte: 'Mondig neemt een vergadering in de ruimte op ({tijd})',
      tipAlleen: 'Mondig neemt een notitie op ({tijd})'
    });
  });

  it('zijn Engels volgens de woordenlijst', () => {
    zetTaal('en');
    const en = systeemvaklabels(t());
    expect(en.stoppen).toBe('Stop and transcribe');
    expect(en.map).toBe('Open folder');
    expect(en.instellingen).toBe('Settings');
    expect(en.opnameLoopt).toBe('Recording');
    expect(en.tipOnline).toBe('Mondig is recording a meeting ({tijd})');
    expect(en.tipAlleen).toContain('{tijd}');
    expect(Object.values(en).filter((tekst) => tekst === '')).toEqual([]);
  });
});

describe('systeemvakZetLabels', () => {
  beforeEach(() => {
    kern.invoke.mockClear();
  });

  it('roept het commando van Rust aan met de teksten', async () => {
    const labels = systeemvaklabels(t());
    await systeemvakZetLabels(labels);
    expect(kern.invoke).toHaveBeenCalledWith('systeemvak_zet_labels', { labels });
  });
});
