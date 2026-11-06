// Alleen voor ontwikkeling in de gewone browser: een zacht geluidje om het afspelen van een regel
// te proberen (stroom V, V-1). Rust geeft in de app een WAV met het echte stuk; hier is het een
// toon van de lengte van de regel, hooguit 4 seconden, met een hoogte die bij de regel past.

const SAMPLERATE = 8000;

/** Een WAV (mono, 16 bit, 8 kHz) met een zachte toon van `beginMs` tot `eindMs`. */
export function maakTestWav(beginMs: number, eindMs: number): ArrayBuffer {
  const duurMs = Math.min(4000, Math.max(500, eindMs - beginMs));
  const aantal = Math.round((SAMPLERATE * duurMs) / 1000);
  const buffer = new ArrayBuffer(44 + aantal * 2);
  const weergave = new DataView(buffer);
  const tekst = (plek: number, waarde: string): void => {
    for (let i = 0; i < waarde.length; i += 1) weergave.setUint8(plek + i, waarde.charCodeAt(i));
  };
  tekst(0, 'RIFF');
  weergave.setUint32(4, 36 + aantal * 2, true);
  tekst(8, 'WAVE');
  tekst(12, 'fmt ');
  weergave.setUint32(16, 16, true);
  weergave.setUint16(20, 1, true);
  weergave.setUint16(22, 1, true);
  weergave.setUint32(24, SAMPLERATE, true);
  weergave.setUint32(28, SAMPLERATE * 2, true);
  weergave.setUint16(32, 2, true);
  weergave.setUint16(34, 16, true);
  tekst(36, 'data');
  weergave.setUint32(40, aantal * 2, true);
  const hoogte = 220 + ((Math.floor(beginMs / 1000) * 37) % 200);
  for (let i = 0; i < aantal; i += 1) {
    // Een zachte aanloop en uitloop, zodat het niet klikt.
    const omhulling = Math.min(1, i / 400, (aantal - i) / 400);
    const waarde = Math.sin((2 * Math.PI * hoogte * i) / SAMPLERATE) * 0.15 * omhulling;
    weergave.setInt16(44 + i * 2, Math.round(waarde * 32767), true);
  }
  return buffer;
}
