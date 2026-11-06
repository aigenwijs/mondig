// Een nieuwe sneltoets vangen in het instellingenscherm. Hier staat alleen het vangen van de
// toetsen, het omzetten van `KeyboardEvent.code` naar de schrijfwijze van de plugin en de
// labels in gewone taal, als pure functies zodat het zonder venster te testen is. De regels
// (welke toetsen mogen, wat gereserveerd is, wat een waarschuwing vraagt) staan op een
// plek: `crates/mondig-core/src/sneltoets.rs`. De interface vraagt het oordeel aan Rust via
// `sneltoetsControleer` in `sneltoets.ts`.
//
// De plugin (global-hotkey) herkent dezelfde toetsnamen als `KeyboardEvent.code`
// (`KeyA`, `Digit1`, `Space`, `F9`, `ArrowUp`), met `Ctrl`, `Alt` en `Shift` ervoor,
// in die volgorde.

/** De standaardsneltoets in de schrijfwijze van de plugin. */
export const STANDAARD_SNELTOETS = 'Ctrl+Space';

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
export function toetsNaam(code: string): string {
  if (CTRL.includes(code)) return 'Ctrl';
  if (ALT.includes(code)) return 'Alt';
  if (SHIFT.includes(code)) return 'Shift';
  if (WINDOWS.includes(code)) return 'Windows';
  if (code === 'Escape') return 'Esc';
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^Numpad[0-9]$/.test(code)) return `Num ${code.slice(6)}`;
  return VASTE_TOETSEN[code] ?? code;
}

/** Een sneltoets van de plugin (`Ctrl+Space`) in gewone taal (`Ctrl+Spatie`). */
export function labelVanToets(toets: string): string {
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

/**
 * Wat de gebruiker op dit moment ziet terwijl hij toetsen indrukt: `Ctrl+Alt+` zolang er
 * alleen modifiers zijn, `Ctrl+Alt+Spatie` met een gewone toets.
 */
export function liveLabel(codes: string[]): string {
  if (codes.length === 0) return '';
  const namen: string[] = [];
  for (const code of gesorteerd(codes)) {
    const naam = toetsNaam(code);
    if (!namen.includes(naam)) namen.push(naam);
  }
  const alleenModifiers = codes.every(isModifier);
  return alleenModifiers ? `${namen.join('+')}+` : namen.join('+');
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
