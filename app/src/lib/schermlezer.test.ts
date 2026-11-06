import { describe, expect, it } from 'vitest';
import { bereikteDrempel } from './schermlezer';

describe('bereikteDrempel', () => {
  it('meldt precies bij 25, 50, 75 en 100 procent', () => {
    expect(bereikteDrempel(24, 25)).toBe(25);
    expect(bereikteDrempel(49, 50)).toBe(50);
    expect(bereikteDrempel(74, 75)).toBe(75);
    expect(bereikteDrempel(99, 100)).toBe(100);
  });

  it('zwijgt tussen de drempels', () => {
    for (let procent = 1; procent < 100; procent++) {
      if (procent % 25 === 0) continue;
      expect(bereikteDrempel(procent - 1, procent), `${String(procent)} procent`).toBeNull();
    }
    expect(bereikteDrempel(25, 26)).toBeNull();
    expect(bereikteDrempel(100, 100)).toBeNull();
  });

  it('meldt een drempel maar één keer', () => {
    expect(bereikteDrempel(24, 25)).toBe(25);
    expect(bereikteDrempel(25, 25)).toBeNull();
  });

  it('meldt bij een sprong alleen de hoogste gepasseerde drempel', () => {
    expect(bereikteDrempel(40, 60)).toBe(50);
    expect(bereikteDrempel(0, 100)).toBe(100);
    expect(bereikteDrempel(10, 80)).toBe(75);
  });

  it('meldt niets als er terug of stil gezet wordt', () => {
    expect(bereikteDrempel(60, 60)).toBeNull();
    expect(bereikteDrempel(60, 40)).toBeNull();
  });

  it('meldt niets over drempels die al voorbij waren toen het scherm openging', () => {
    // Een hervatte download bij 60 procent: begin met vorig = 60.
    expect(bereikteDrempel(60, 61)).toBeNull();
    expect(bereikteDrempel(74, 75)).toBe(75);
  });
});
