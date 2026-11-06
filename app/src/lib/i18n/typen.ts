import type { nl } from './nl';

/**
 * Berichttype afgeleid van de Nederlandse bron: functiesignaturen blijven
 * behouden, teksten worden verbreed naar `string`. Zo is een ontbrekende of
 * verkeerd getypte sleutel in een andere taal een compileerfout.
 */
export type Berichten = {
  [K in keyof typeof nl]: (typeof nl)[K] extends (...args: infer A) => infer R
    ? (...args: A) => R
    : string;
};

/** Talen die de app kent. */
export type Taal = 'nl' | 'en';
