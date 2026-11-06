// Routes waar de app (systeembalk, pil) het venster naartoe mag sturen met het event
// `navigeer`. Alleen deze: het event is een verzoek, geen adres om blind te volgen.
const ROUTES = ['/', '/vergadering', '/bestanden', '/instellingen', '/modellen', '/over'] as const;

export type BekendeRoute = (typeof ROUTES)[number];

/** De route als die bekend is, anders `null`. */
export function bekendeRoute(pad: string): BekendeRoute | null {
  return (ROUTES as readonly string[]).includes(pad) ? (pad as BekendeRoute) : null;
}
