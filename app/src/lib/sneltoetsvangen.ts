// Een nieuwe sneltoets vangen in het instellingenscherm. Hier staat alleen het vangen van de
// toetsen, het omzetten van `KeyboardEvent.code` naar de schrijfwijze van de plugin en de
// labels in gewone taal, als pure functies zodat het zonder venster te testen is. De regels
// (welke toetsen mogen, wat gereserveerd is, wat een waarschuwing vraagt) staan op een
// plek: `crates/mondig-core/src/sneltoets.rs`. De interface vraagt het oordeel aan Rust via
// `sneltoetsControleer` in `sneltoets.ts`.
//
// De plugin (global-hotkey) herkent dezelfde toetsnamen als `KeyboardEvent.code`
// (`KeyA`, `Digit1`, `Space`, `F9`, `ArrowUp`), met `Ctrl`, `Alt` en `Shift` ervoor,
// in die volgorde. De standaarden en het platform komen uit Rust (`sneltoets_info`); de
// weergave krijgt het platform als parameter, zonder parameter geldt Windows.
//
// Op de Mac staan de modifiers als symbolen in de volgorde van Apple, direct voor de toets
// (`⌥Spatie`, `⌃⌘D`); voor schermlezers voluit (`Option Spatie`).

import type { Platform } from './sneltoets';

/** De symbolen en voluit-namen van de modifiers op de Mac, in de volgorde van Apple. */
const MAC_MODIFIERS = [
  { sleutel: 'ctrl', symbool: '\u2303', voluit: 'Control' },
  { sleutel: 'alt', symbool: '\u2325', voluit: 'Option' },
  { sleutel: 'shift', symbool: '\u21E7', voluit: 'Shift' },
  { sleutel: 'super', symbool: '\u2318', voluit: 'Command' }
] as const;

type ModifierSleutel = (typeof MAC_MODIFIERS)[number]['sleutel'];

const CTRL = ['ControlLeft', 'ControlRight'];
const ALT = ['AltLeft', 'AltRight'];
const SHIFT = ['ShiftLeft', 'ShiftRight'];
// `OSLeft` en `OSRight` zijn de oude namen van de Windows-toets in sommige browsers.
const WINDOWS = ['MetaLeft', 'MetaRight', 'OSLeft', 'OSRight'];

const VASTE_TOETSEN: Record<string, string> = {
  Space: 'Spatie',
  Backquote: '`',
  Backslash: '\\',
  BracketLeft: '[',
  BracketRight: ']',
  Comma: ',',
  Equal: '=',
  Minus: '-',
  Period: '.',
  Quote: "'",
  Semicolon: ';',
  Slash: '/',
  Insert: 'Insert',
  Delete: 'Delete',
  Home: 'Home',
  End: 'End',
  PageUp: 'Page Up',
  PageDown: 'Page Down',
  Pause: 'Pause',
  ScrollLock: 'Scroll Lock',
  ArrowUp: 'Pijl omhoog',
  ArrowDown: 'Pijl omlaag',
  ArrowLeft: 'Pijl links',
  ArrowRight: 'Pijl rechts'
};

/** Is dit een modifier (Ctrl, Alt, Shift of de Windows-toets)? */
export function isModifier(code: string): boolean {
  return [...CTRL, ...ALT, ...SHIFT, ...WINDOWS].includes(code);
}

/** Is dit de Windows-toets? */
export function isWindowsToets(code: string): boolean {
  return WINDOWS.includes(code);
}

/** Een toets in gewone Nederlandse taal: `Space` wordt Spatie, `KeyA` wordt A. */
export function toetsNaam(code: string, platform: Platform = 'windows'): string {
  if (CTRL.includes(code)) return 'Ctrl';
  if (ALT.includes(code)) return 'Alt';
  if (SHIFT.includes(code)) return 'Shift';
  if (WINDOWS.includes(code)) return platform === 'mac' ? 'Command' : 'Windows';
  if (code === 'Escape') return 'Esc';
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^Numpad[0-9]$/.test(code)) return `Num ${code.slice(6)}`;
  return VASTE_TOETSEN[code] ?? code;
}

/** Welke modifier is dit deel van een sneltoets van de plugin (`Ctrl`, `Option`, `Cmd`)? */
function modifierVanDeel(deel: string): ModifierSleutel | null {
  switch (deel.trim().toLowerCase()) {
    case 'ctrl':
    case 'control':
      return 'ctrl';
    case 'alt':
    case 'option':
      return 'alt';
    case 'shift':
      return 'shift';
    case 'super':
    case 'cmd':
    case 'command':
    case 'meta':
      return 'super';
    default:
      return null;
  }
}

/** Welke modifier is deze `KeyboardEvent.code`? */
function modifierVanCode(code: string): ModifierSleutel | null {
  if (CTRL.includes(code)) return 'ctrl';
  if (ALT.includes(code)) return 'alt';
  if (SHIFT.includes(code)) return 'shift';
  if (WINDOWS.includes(code)) return 'super';
  return null;
}

/** Zet modifiers en gewone toetsen op de manier van de Mac: symbolen direct voor de toets. */
function macWeergave(modifiers: Set<ModifierSleutel>, rest: string[], voluit: boolean): string {
  const aan = MAC_MODIFIERS.filter((m) => modifiers.has(m.sleutel));
  if (voluit) return [...aan.map((m) => m.voluit), ...rest].join(' ');
  return aan.map((m) => m.symbool).join('') + rest.join('+');
}

function macVanToets(toets: string, voluit: boolean): string {
  const modifiers = new Set<ModifierSleutel>();
  const rest: string[] = [];
  for (const deel of toets.split('+')) {
    const modifier = modifierVanDeel(deel);
    if (modifier !== null) modifiers.add(modifier);
    else rest.push(toetsNaam(deel.trim(), 'mac'));
  }
  return macWeergave(modifiers, rest, voluit);
}

/**
 * Een sneltoets van de plugin (`Ctrl+Space`) in gewone taal (`Ctrl+Spatie`). Op de Mac met
 * symbolen: `Alt+Space` wordt `⌥Spatie`.
 */
export function labelVanToets(toets: string, platform: Platform = 'windows'): string {
  if (platform === 'mac') return macVanToets(toets, false);
  return toets
    .split('+')
    .map((deel) => {
      const klein = deel.trim().toLowerCase();
      if (klein === 'ctrl' || klein === 'control') return 'Ctrl';
      if (klein === 'alt') return 'Alt';
      if (klein === 'shift') return 'Shift';
      return toetsNaam(deel.trim());
    })
    .join('+');
}

/** Dezelfde sneltoets voluit voor een schermlezer: op de Mac `Option Spatie`. */
export function labelVoorlezen(toets: string, platform: Platform = 'windows'): string {
  return platform === 'mac' ? macVanToets(toets, true) : labelVanToets(toets, platform);
}

/** Wat het venster tijdens het vangen van de toetsen bijhoudt. */
export interface Vangstand {
  /** De toetsen die nu ingedrukt zijn (`KeyboardEvent.code`). */
  vast: string[];
  /** De grootste combinatie die tijdens deze poging tegelijk vast was. */
  piek: string[];
}

export const LEGE_VANG: Vangstand = { vast: [], piek: [] };

/**
 * Een toets gaat in. De piek volgt de grootste combinatie die tegelijk vast was; zo telt
 * Ctrl+Alt+Spatie ook als je Alt eerder loslaat dan Spatie.
 */
export function drukIn(stand: Vangstand, code: string): Vangstand {
  if (stand.vast.includes(code)) return stand;
  const vast = [...stand.vast, code];
  const piek = vast.length >= stand.piek.length ? vast : stand.piek;
  return { vast, piek };
}

/** Een toets gaat los. */
export function laatLos(stand: Vangstand, code: string): Vangstand {
  if (!stand.vast.includes(code)) return stand;
  return { vast: stand.vast.filter((c) => c !== code), piek: stand.piek };
}

/** Zijn alle toetsen los na een poging? Dan wordt de piek vastgelegd. */
export function isAfgerond(stand: Vangstand): boolean {
  return stand.vast.length === 0 && stand.piek.length > 0;
}

/** De volgorde waarin een combinatie wordt opgebouwd: Ctrl, Alt, Shift, dan de gewone toets. */
function gesorteerd(codes: string[]): string[] {
  const rang = (code: string): number => {
    if (CTRL.includes(code)) return 0;
    if (ALT.includes(code)) return 1;
    if (SHIFT.includes(code)) return 2;
    return 3;
  };
  return [...codes].sort((a, b) => rang(a) - rang(b));
}

/** De modifiers en de gewone toetsen (op naam, zonder dubbelen) van de gevangen toetsen. */
function liveDelen(
  codes: string[],
  platform: Platform
): { modifiers: Set<ModifierSleutel>; rest: string[] } {
  const modifiers = new Set<ModifierSleutel>();
  const rest: string[] = [];
  for (const code of codes) {
    const modifier = modifierVanCode(code);
    if (modifier !== null) modifiers.add(modifier);
    else {
      const naam = toetsNaam(code, platform);
      if (!rest.includes(naam)) rest.push(naam);
    }
  }
  return { modifiers, rest };
}

/**
 * Wat de gebruiker op dit moment ziet terwijl hij toetsen indrukt: `Ctrl+Alt+` zolang er
 * alleen modifiers zijn, `Ctrl+Alt+Spatie` met een gewone toets. Op de Mac staan de symbolen
 * direct voor de toets (`⌃⌥`, `⌃⌥Spatie`).
 */
export function liveLabel(codes: string[], platform: Platform = 'windows'): string {
  if (codes.length === 0) return '';
  if (platform === 'mac') {
    const { modifiers, rest } = liveDelen(codes, platform);
    return macWeergave(modifiers, rest, false);
  }
  const namen: string[] = [];
  for (const code of gesorteerd(codes)) {
    const naam = toetsNaam(code, platform);
    if (!namen.includes(naam)) namen.push(naam);
  }
  const alleenModifiers = codes.every(isModifier);
  return alleenModifiers ? `${namen.join('+')}+` : namen.join('+');
}

/** Het live label voluit voor een schermlezer: op de Mac `Control Option Spatie`. */
export function liveVoorlezen(codes: string[], platform: Platform = 'windows'): string {
  if (codes.length === 0) return '';
  if (platform !== 'mac') return liveLabel(codes, platform);
  const { modifiers, rest } = liveDelen(codes, platform);
  return macWeergave(modifiers, rest, true);
}

/**
 * Maakt van de gevangen toetsen een sneltoets in de schrijfwijze van de plugin
 * (`Ctrl+Alt+Shift+KeyD`): eerst de modifiers in vaste volgorde, dan de andere toetsen zoals
 * ze zijn. De Windows-toets wordt `Super`. Of het mag, beslist Rust.
 */
export function bouwToets(codes: string[]): string {
  const windows = codes.some(isWindowsToets) ? ['Super'] : [];
  const modifiers = [
    ...(codes.some((c) => CTRL.includes(c)) ? ['Ctrl'] : []),
    ...(codes.some((c) => ALT.includes(c)) ? ['Alt'] : []),
    ...(codes.some((c) => SHIFT.includes(c)) ? ['Shift'] : [])
  ];
  const gewone = codes.filter((code) => !isModifier(code));
  return [...windows, ...modifiers, ...gewone].join('+');
}
