import { describe, expect, it } from 'vitest';
import { SPREKERMODEL, bijAanzetten, naModelwijziging, schakelaarAan, weergave } from './sprekers';

describe('de schakelaar Sprekers herkennen', () => {
  it('staat alleen aan met de instelling aan en het model helemaal op de computer', () => {
    expect(schakelaarAan(true, 'compleet')).toBe(true);
    // Standaard staat de instelling in Rust aan, maar zonder model gebeurt er niets.
    expect(schakelaarAan(true, 'ontbreekt')).toBe(false);
    expect(schakelaarAan(true, 'gedeeltelijk')).toBe(false);
    expect(schakelaarAan(true, undefined)).toBe(false);
    expect(schakelaarAan(false, 'compleet')).toBe(false);
  });

  it('bewaart meteen als het model er is en vraagt anders eerst om de download', () => {
    expect(bijAanzetten('compleet')).toBe('bewaar');
    expect(bijAanzetten('ontbreekt')).toBe('vraag');
    expect(bijAanzetten('gedeeltelijk')).toBe('vraag');
    expect(bijAanzetten(undefined)).toBe('vraag');
  });

  it('kent het id van het sprekermodel uit de catalogus', () => {
    expect(SPREKERMODEL).toBe('sortformer-4spk-v2.1-q8');
  });
});

describe('wachten op het sprekermodel', () => {
  it('zet aan zodra het model er helemaal is', () => {
    expect(naModelwijziging('wacht', 'compleet', false, true)).toBe('aanzetten');
    // Ook als de gebeurtenis van de download eerder binnenkomt dan het nieuwe overzicht.
    expect(naModelwijziging('wacht', 'compleet', true, true)).toBe('aanzetten');
  });

  it('wacht zolang de download loopt', () => {
    expect(naModelwijziging('wacht', 'gedeeltelijk', true, true)).toBe('blijf');
    expect(naModelwijziging('wacht', 'ontbreekt', true, true)).toBe('blijf');
  });

  it('wacht ook voordat de download is gestart', () => {
    expect(naModelwijziging('wacht', 'ontbreekt', false, false)).toBe('blijf');
  });

  it('geeft op als de download stopt zonder dat het model er is', () => {
    expect(naModelwijziging('wacht', 'gedeeltelijk', false, true)).toBe('opgeven');
    expect(naModelwijziging('wacht', 'ontbreekt', false, true)).toBe('opgeven');
  });

  it('doet niets buiten de fase wacht', () => {
    expect(naModelwijziging('rust', 'compleet', false, true)).toBe('blijf');
    expect(naModelwijziging('vraag', 'compleet', false, true)).toBe('blijf');
  });
});

describe('wat onder de schakelaar staat', () => {
  it('toont de vraag, de stand van de download, de bevestiging of niets', () => {
    expect(weergave('vraag', false)).toBe('vraag');
    // Vanaf de klik staat er altijd iets, ook als Rust nog niets meldde.
    expect(weergave('wacht', false)).toBe('voortgang');
    expect(weergave('rust', true)).toBe('aangezet');
    expect(weergave('rust', false)).toBe('niets');
  });
});
