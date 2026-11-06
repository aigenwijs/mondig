# Zet "mondig." in Sora 600 om naar SVG-paden, met kerning (HarfBuzz) en -0.045em spatiering.
import io
import sys

import uharfbuzz as hb
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont

bron, uit = sys.argv[1], sys.argv[2]
inst = instantiateVariableFont(TTFont(bron), {"wght": 600}, inplace=False)
inst.flavor = None  # als gewone TTF opslaan; HarfBuzz leest geen WOFF2
buf = io.BytesIO()
inst.save(buf)
hbf = hb.Font(hb.Face(buf.getvalue()))

gs = inst.getGlyphSet()
order = inst.getGlyphOrder()
upm = inst["head"].unitsPerEm
asc = inst["OS/2"].sTypoAscender
desc = inst["OS/2"].sTypoDescender
SPATIERING = -0.045 * upm


def paden(tekst):
    b = hb.Buffer()
    b.add_str(tekst)
    b.guess_segment_properties()
    hb.shape(hbf, b, {"kern": True, "liga": True})
    x, delen, grenzen = 0.0, [], []
    for i, (inf, p) in enumerate(zip(b.glyph_infos, b.glyph_positions)):
        naam = order[inf.codepoint]
        tf = (1, 0, 0, -1, x + p.x_offset, asc - p.y_offset)
        pen = SVGPathPen(gs)
        gs[naam].draw(TransformPen(pen, tf))
        bp = BoundsPen(gs)
        gs[naam].draw(TransformPen(bp, tf))
        if bp.bounds:
            grenzen.append(bp.bounds)
        delen.append((tekst[inf.cluster], pen.getCommands()))
        x += p.x_advance + (SPATIERING if i < len(b.glyph_infos) - 1 else 0)
    links = min(g[0] for g in grenzen)
    boven = min(g[1] for g in grenzen)
    rechts = max(g[2] for g in grenzen)
    onder = max(g[3] for g in grenzen)
    return delen, (links, boven, rechts, onder)


# Woordmerk: strak op de inktgrenzen, met de ascender- en descenderruimte van het font.
delen, (l, _, r, _) = paden("mondig.")
for (tekst, punt), naam in [
    (("#F0F0F0", "#00FF95"), "donker"),
    (("#1A1815", "#7100F6"), "licht"),
    (("#FFFFFF", "#FFFFFF"), "wit"),
    (("#1A1815", "#1A1815"), "inkt"),
]:
    g = "\n".join(f'<path fill="{punt if t == "." else tekst}" d="{d}"/>' for t, d in delen)
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{l:.1f} 0 {r - l:.1f} {asc - desc}" '
        f'role="img" aria-label="mondig.">\n{g}\n</svg>\n'
    )
    with open(f"{uit}/mondig-woordmerk-{naam}.svg", "w", encoding="utf-8") as f:
        f.write(svg)

# App-icoon: "m." gecentreerd op een vierkant met 22 procent afronding.
mdelen, (ml, mb, mr, mo) = paden("m.")
ZIJDE = 1024
schaal = ZIJDE * 0.6 / (mr - ml)
tx = (ZIJDE - (mr - ml) * schaal) / 2 - ml * schaal
ty = (ZIJDE - (mo - mb) * schaal) / 2 - mb * schaal
for (bg, fg, punt), naam in [
    (("#00FF95", "#0A0A0A", "#0A0A0A"), "groen"),
    (("#0A0A0A", "#F0F0F0", "#00FF95"), "zwart"),
]:
    g = "\n".join(f'<path fill="{punt if t == "." else fg}" d="{d}"/>' for t, d in mdelen)
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {ZIJDE} {ZIJDE}">\n'
        f'<rect width="{ZIJDE}" height="{ZIJDE}" rx="{ZIJDE * 0.22:.0f}" fill="{bg}"/>\n'
        f'<g transform="translate({tx:.1f} {ty:.1f}) scale({schaal:.4f})">\n{g}\n</g>\n</svg>\n'
    )
    with open(f"{uit}/mondig-icoon-{naam}.svg", "w", encoding="utf-8") as f:
        f.write(svg)

print("upm", upm, "breedte", round(r - l), "hoogte", asc - desc)
