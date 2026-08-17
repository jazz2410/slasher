"""Generate assets/tiles/village.png from the locked palette.

Procedural, not hand-drawn: noise, dithering and geometry. Good enough to design
and play levels against, and every tile sits on the ID the spec assigns it, so a
hand-drawn sheet can replace this file without touching a level.

Usage:  python3 tools/make_tileset.py
"""
import random

from PIL import Image

T = 16                      # tile size
COLS, ROWS = 16, 8
OUT = "assets/tiles/village.png"

# --- palette (docs/tileset-spec.md) -----------------------------------------
S1, S2, S3, S4, S5, S6 = "1A1714", "2C2722", "423A33", "5A4F45", "77685A", "948373"
T1, T2, T3, T4 = "3A1912", "5C2A1C", "8E4529", "B25E35"
C1, C2, C3, C4 = "0B0908", "1C1512", "33261E", "4E3A2B"
B1, B2, B3, B4 = "2A0907", "4C110E", "7A1512", "A31C18"
Z1, Z2, Z3, Z4 = "4A2E17", "7A5A30", "A87F45", "D8B267"
F1, F2, F3 = "C24A16", "E8842A", "F7C64F"
N1, N2, N3 = "0D0F13", "171B22", "262E38"


def rgb(h):
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), 255)


CLEAR = (0, 0, 0, 0)
sheet = Image.new("RGBA", (COLS * T, ROWS * T), CLEAR)


class Tile:
    """A 16x16 scratch tile with a deterministic RNG."""

    def __init__(self, index):
        self.px = [[CLEAR] * T for _ in range(T)]
        self.rng = random.Random(1000 + index)

    def set(self, x, y, colour):
        if 0 <= x < T and 0 <= y < T:
            self.px[y][x] = rgb(colour) if isinstance(colour, str) else colour

    def fill(self, weighted):
        """Fill with a weighted random mix of colours — the base texture."""
        colours = [c for c, _ in weighted]
        weights = [w for _, w in weighted]
        for y in range(T):
            for x in range(T):
                self.set(x, y, self.rng.choices(colours, weights)[0])

    def rect(self, x0, y0, x1, y1, colour):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.set(x, y, colour)

    def hline(self, y, colour, x0=0, x1=T - 1):
        self.rect(x0, y, x1, y, colour)

    def vline(self, x, colour, y0=0, y1=T - 1):
        self.rect(x, y0, x, y1, colour)

    def speckle(self, colour, count):
        for _ in range(count):
            self.set(self.rng.randrange(T), self.rng.randrange(T), colour)

    def blob(self, cx, cy, radius, colour, ragged=0.45):
        """An irregular round splat — blood, rubble, ash."""
        for y in range(T):
            for x in range(T):
                d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5
                if d <= radius - self.rng.random() * radius * ragged:
                    self.set(x, y, colour)

    def blit(self, index):
        cx, cy = (index % COLS) * T, (index // COLS) * T
        for y in range(T):
            for x in range(T):
                if self.px[y][x] != CLEAR:
                    sheet.putpixel((cx + x, cy + y), self.px[y][x])


# --- terrain ----------------------------------------------------------------

# Packed earth reads dark and desaturated. An earlier mix leaned on the
# terracotta ramp and came out orange, which stole the eye from the blood —
# the one thing on screen that is meant to be saturated.
DIRT = [(C3, 38), (C4, 24), (S2, 22), (T1, 16)]
STONE = [(S3, 44), (S4, 36), (S2, 14), (S5, 6)]


def terrain(index, mix, top, lip, edge, *, sides, top_edge, bottom, variant=0):
    """One cell of a nine-slice.

    `sides` is a string of 'lr', `top_edge`/`bottom` are booleans. Lit lip on the
    upper surface only — that reads as ground, not as directional lighting.
    """
    t = Tile(index)
    t.fill(mix)
    if variant == 1:                       # cracked
        x = t.rng.randrange(3, 12)
        for y in range(3, T - 2):
            t.set(x, y, S1)
            x += t.rng.choice((-1, 0, 0, 1))
    if variant == 2:                       # rubble-strewn
        for _ in range(5):
            t.blob(t.rng.randrange(3, 13), t.rng.randrange(3, 13), 2.0, edge, 0.6)
    if top_edge:
        t.hline(0, lip)
        t.hline(1, top)
        for x in range(T):                 # broken second row, so it is not a ruler line
            if t.rng.random() < 0.4:
                t.set(x, 2, top)
    if "l" in sides:
        t.vline(0, edge)
    if "r" in sides:
        t.vline(T - 1, edge)
    if bottom:
        t.hline(T - 1, S1)
    return t


def nine_slice(base, mix, top, lip, edge):
    """Lay a 16-tile terrain block down starting at `base`."""
    spec = [
        (0, "l", True, False), (1, "", True, False), (2, "r", True, False),
        (3, "l", False, False), (4, "", False, False), (5, "r", False, False),
        (6, "l", False, True), (7, "", False, True), (8, "r", False, True),
        (9, "l", True, False), (10, "r", True, False),
        (11, "l", False, True), (12, "r", False, True),
    ]
    for offset, sides, top_edge, bottom in spec:
        terrain(base + offset, mix, top, lip, edge,
                sides=sides, top_edge=top_edge, bottom=bottom).blit(base + offset)
    for offset, variant in ((13, 1), (14, 2)):
        terrain(base + offset, mix, top, lip, edge,
                sides="", top_edge=False, bottom=False, variant=variant).blit(base + offset)
    terrain(base + 15, mix, top, lip, edge,
            sides="lr", top_edge=True, bottom=True).blit(base + 15)


nine_slice(0, DIRT, C4, Z2, C1)
nine_slice(16, STONE, S5, S6, S1)


def masonry(index):
    """Overlay courses of blocks on the stone fills so they read as built."""
    t = Tile(index)
    t.fill(STONE)
    for row, y in enumerate(range(0, T, 5)):
        t.hline(y, S2)
        offset = 0 if row % 2 == 0 else 4
        for x in range(offset, T, 8):
            t.vline(x, S2, y, min(T - 1, y + 4))
    return t


for i in (20, 22, 25):                      # a few stone fills get courses
    masonry(i).blit(i)

# --- columns and architecture (row 2) ---------------------------------------

def fluted(index, *, cap=False, base=False, cracked=False, snapped=False):
    t = Tile(index)
    x0, x1 = (1, 14) if (cap or base) else (3, 12)
    t.rect(x0, 0, x1, T - 1, S3)
    for x in range(x0, x1 + 1):             # vertical flutes
        step = (x - x0) % 3
        t.vline(x, (S4, S5, S3)[step], 0, T - 1)
    t.vline(x0, S2)
    t.vline(x1, S2)
    if cap:
        t.rect(0, 0, T - 1, 3, S5)
        t.hline(0, S6)
        t.hline(4, S2)
    if base:
        t.rect(0, T - 4, T - 1, T - 1, S5)
        t.hline(T - 1, S1)
        t.hline(T - 5, S2)
    if cracked:
        y = t.rng.randrange(4, 12)
        for x in range(x0, x1 + 1):
            t.set(x, y + t.rng.choice((-1, 0, 0, 1)), S1)
    if snapped:
        for x in range(x0, x1 + 1):
            for y in range(0, t.rng.randrange(2, 7)):
                t.set(x, y, CLEAR)
        for x in range(x0, x1 + 1):
            t.set(x, t.rng.randrange(3, 8), S6)
    return t


fluted(32, base=True).blit(32)
fluted(33).blit(33)
fluted(34, cap=True).blit(34)
fluted(35, cracked=True).blit(35)
fluted(36, snapped=True).blit(36)

t = Tile(37)                                # toppled column, lying down
t.rect(0, 5, T - 1, 11, S3)
for y in range(5, 12):
    t.hline(y, (S4, S5, S3)[(y - 5) % 3], 0, T - 1)
t.hline(5, S2)
t.hline(11, S2)
t.blit(37)

t = Tile(38)                                # step
t.rect(0, 6, T - 1, T - 1, S4)
t.hline(6, S6)
t.hline(7, S5)
t.hline(T - 1, S1)
t.blit(38)

for idx, (x0, x1) in ((39, (2, 15)), (40, (0, 15)), (41, (0, 13))):
    t = Tile(idx)                           # arch pieces
    t.rect(x0, 0, x1, 6, S4)
    t.hline(0, S5, x0, x1)
    t.hline(6, S2, x0, x1)
    t.blit(idx)

t = Tile(42)                                # lintel
t.rect(0, 3, T - 1, 10, S4)
t.hline(3, S5)
t.hline(10, S1)
t.blit(42)

t = Tile(43)                                # plinth
t.rect(1, 4, 14, T - 1, S4)
t.hline(4, S5, 1, 14)
t.blit(43)

for idx, n, r in ((44, 4, 2.2), (45, 7, 2.8), (46, 1, 4.0)):
    t = Tile(idx)                           # rubble piles / loose block
    for _ in range(n):
        t.blob(t.rng.randrange(3, 13), t.rng.randrange(6, 14), r, t.rng.choice((S3, S4, S2)), 0.5)
    t.blit(idx)

# --- walls and plaster (row 3) ----------------------------------------------

PLASTER = [(S4, 50), (S5, 32), (S3, 18)]

def wall(index, *, cracked=False, holed=False, bloodied=False):
    t = Tile(index)
    t.fill(PLASTER)
    t.speckle(S6, 6)
    if cracked:
        x = t.rng.randrange(4, 12)
        for y in range(T):
            t.set(x, y, S2)
            x += t.rng.choice((-1, 0, 0, 1))
    if holed:
        t.blob(8, 8, 5.5, S2, 0.5)
        t.blob(8, 8, 3.5, S1, 0.6)
    if bloodied:
        for _ in range(3):
            t.blob(t.rng.randrange(4, 12), t.rng.randrange(2, 9), 3.0, B3, 0.7)
        t.speckle(B2, 10)
    return t


wall(48).blit(48)
wall(49, cracked=True).blit(49)
wall(50, holed=True).blit(50)
wall(51, bloodied=True).blit(51)

t = Tile(52)                                # coping
t.fill(PLASTER)
t.rect(0, 0, T - 1, 3, S5)
t.hline(0, S6)
t.blit(52)

t = Tile(53)                                # corner
t.fill(PLASTER)
t.vline(0, S2)
t.hline(0, S5)
t.blit(53)

for idx, side in ((54, "l"), (55, "r")):    # door jambs
    t = Tile(idx)
    t.fill(PLASTER)
    if side == "l":
        t.rect(10, 0, T - 1, T - 1, N1)
        t.vline(9, S2)
    else:
        t.rect(0, 0, 5, T - 1, N1)
        t.vline(6, S2)
    t.blit(idx)

t = Tile(56)                                # door lintel
t.fill(PLASTER)
t.rect(0, 6, T - 1, T - 1, N1)
t.hline(5, S2)
t.blit(56)

t = Tile(57)                                # window
t.fill(PLASTER)
t.rect(3, 3, 12, 12, N1)
t.rect(3, 3, 12, 3, S2)
t.blit(57)

t = Tile(58)                                # broken shutter
t.fill(PLASTER)
t.rect(3, 3, 12, 12, N1)
for y in range(4, 12, 3):
    t.hline(y, C4, 3, 12)
t.set(7, 6, CLEAR)
t.blit(58)

t = Tile(59)                                # chain on wall
t.fill(PLASTER)
for y in range(0, T, 3):
    t.set(8, y, Z2)
    t.set(8, y + 1, Z1)
t.blit(59)

# --- burnt timber and roof (row 4) ------------------------------------------

def beam(index, orientation):
    t = Tile(index)
    if orientation == "v":
        t.rect(5, 0, 10, T - 1, C3)
        t.vline(5, C2); t.vline(10, C2); t.vline(7, C4)
    elif orientation == "h":
        t.rect(0, 5, T - 1, 10, C3)
        t.hline(5, C2); t.hline(10, C2); t.hline(7, C4)
    else:
        for i in range(T):
            y = i if orientation == "d" else T - 1 - i
            for w in range(-2, 3):
                t.set(i, y + w, C3 if w else C4)
    return t


beam(64, "v").blit(64)
beam(65, "h").blit(65)
beam(66, "d").blit(66)
beam(67, "u").blit(67)

t = Tile(68)                                # broken beam end
t.rect(5, 4, 10, T - 1, C3)
t.vline(5, C2); t.vline(10, C2)
for x in range(5, 11):
    for y in range(4, 4 + t.rng.randrange(0, 4)):
        t.set(x, y, CLEAR)
t.blit(68)

t = Tile(69)                                # charred plank pile
for y in (13, 10, 7):
    t.rect(t.rng.randrange(0, 3), y, t.rng.randrange(12, 16), y + 2, C3)
    t.hline(y, C4, 0, T - 1)
t.blit(69)

def rooftiles(index, state):
    t = Tile(index)
    t.fill([(T2, 50), (T1, 30), (T3, 20)])
    for y in range(0, T, 4):
        t.hline(y, T1)
        for x in range(0, T, 6):
            t.vline((x + (y // 4) * 3) % T, T1, y, y + 3)
    if state == "slip":
        for x in range(T):
            for y in range(0, t.rng.randrange(0, 5)):
                t.set(x, y, CLEAR)
    if state == "heap":
        t.px = [[CLEAR] * T for _ in range(T)]
        for _ in range(9):
            t.blob(t.rng.randrange(2, 14), t.rng.randrange(8, 15), 2.4,
                   t.rng.choice((T1, T2, T3)), 0.5)
    return t


rooftiles(70, "flat").blit(70)
rooftiles(71, "slip").blit(71)
rooftiles(72, "heap").blit(72)

t = Tile(73)                                # burnt thatch
t.fill([(C2, 45), (C3, 35), (C1, 20)])
for _ in range(14):
    x, y = t.rng.randrange(T), t.rng.randrange(T)
    t.set(x, y, C4)
t.blit(73)

t = Tile(74)                                # smouldering thatch
t.fill([(C2, 45), (C3, 33), (C1, 22)])
for _ in range(7):
    t.set(t.rng.randrange(T), t.rng.randrange(T), t.rng.choice((F1, F2)))
t.blit(74)

t = Tile(75)                                # rafter silhouette
t.rect(0, 6, T - 1, 8, C1)
t.vline(4, C1, 0, T - 1)
t.vline(11, C1, 0, T - 1)
t.blit(75)

# --- blood and aftermath (row 5) --------------------------------------------

def splat(index, drops, radius, colour, y0=2, y1=14):
    t = Tile(index)
    for _ in range(drops):
        t.blob(t.rng.randrange(2, 14), t.rng.randrange(y0, y1), radius, colour, 0.6)
    t.speckle(colour, 6)
    return t


splat(80, 2, 2.2, B2).blit(80)
splat(81, 4, 3.4, B2).blit(81)

t = Tile(82)                                # pool
t.blob(8, 12, 6.5, B2, 0.35)
t.blob(8, 12, 4.0, B1, 0.4)
t.blit(82)

t = Tile(83)                                # pool with wet highlight
t.blob(8, 12, 6.5, B2, 0.35)
t.blob(8, 12, 4.0, B3, 0.4)
t.rect(6, 10, 9, 10, B4)
t.blit(83)

for idx, lean in ((84, -1), (85, 1)):       # wall spray
    t = Tile(idx)
    x = 8
    for y in range(2, 14):
        for w in range(3):
            t.set(x + w * lean, y, B3 if w else B2)
        x += lean if t.rng.random() < 0.6 else 0
    t.speckle(B2, 10)
    t.blit(idx)

t = Tile(86)                                # drips from above
for x in (3, 7, 12):
    for y in range(0, t.rng.randrange(5, 13)):
        t.set(x, y, B2)
    t.set(x, y, B3)
t.blit(86)

t = Tile(87)                                # handprint
t.blob(8, 9, 3.2, B2, 0.3)
for x in (5, 7, 9, 11):
    for y in range(3, 7):
        t.set(x, y, B2)
t.blit(87)

t = Tile(88)                                # drag mark
for x in range(T):
    y = 9 + int(1.6 * ((x / 3.0) % 2))
    t.rect(x, y, x, y + 2, B1)
t.speckle(B2, 8)
t.blit(88)

t = Tile(89)                                # scorch
t.blob(8, 9, 6.0, C2, 0.5)
t.blob(8, 9, 3.5, C1, 0.5)
t.blit(89)

t = Tile(90)                                # ash pile
for _ in range(6):
    t.blob(t.rng.randrange(4, 12), t.rng.randrange(10, 15), 2.6, t.rng.choice((C2, C3, S2)), 0.5)
t.blit(90)

t = Tile(91)                                # bone
t.rect(5, 8, 10, 10, S6)
t.set(4, 8, S6); t.set(4, 10, S6); t.set(11, 8, S6); t.set(11, 10, S6)
t.blit(91)

for idx, upright in ((92, True), (93, False)):   # corpses
    t = Tile(idx)
    if upright:
        t.rect(5, 5, 10, T - 1, Z1)
        t.blob(8, 4, 2.6, Z2, 0.3)
        t.rect(4, 8, 11, 10, Z2)
    else:
        t.rect(2, 11, 13, 14, Z1)
        t.blob(3, 10, 2.4, Z2, 0.3)
    t.blob(t.rng.randrange(4, 12), 13, 3.0, B2, 0.6)
    t.blit(idx)

t = Tile(94)                                # shield and arm
t.blob(8, 8, 5.5, B2, 0.15)
t.blob(8, 8, 4.2, Z2, 0.15)
t.rect(11, 9, T - 1, 11, Z1)
t.blit(94)

# --- props (row 6) ----------------------------------------------------------

t = Tile(96)                                # amphora
t.blob(8, 9, 4.4, T2, 0.15)
t.rect(7, 2, 9, 5, T1)
t.rect(6, 5, 10, 6, T3)
t.blit(96)

t = Tile(97)                                # broken amphora
t.blob(8, 11, 4.2, T2, 0.3)
for x in range(4, 13):
    for y in range(11, 8 - t.rng.randrange(0, 3), -1):
        t.set(x, y, T2)
t.blit(97)

t = Tile(98)                                # shards
for _ in range(7):
    x, y = t.rng.randrange(2, 14), t.rng.randrange(9, 15)
    t.rect(x, y, x + 1, y, t.rng.choice((T2, T3)))
t.blit(98)

t = Tile(99)                                # basket
t.rect(4, 7, 11, T - 2, C4)
for y in range(7, T - 1, 2):
    t.hline(y, C3, 4, 11)
t.blit(99)

for idx, broken in ((100, False), (101, True)):  # shields
    t = Tile(idx)
    t.blob(8, 8, 6.2, Z2, 0.1)
    t.blob(8, 8, 5.0, B2, 0.1)
    for i in range(5):                      # lambda
        t.set(6 + i, 11 - i, Z4)
        t.set(10 - i, 11 - i, Z4)
    if broken:
        for y in range(T):
            for x in range(9 + (y // 4), T):
                t.set(x, y, CLEAR)
    t.blit(idx)

t = Tile(102)                               # spear in ground
t.vline(8, C4, 0, 12)
t.set(8, 0, S6); t.set(7, 1, S6); t.set(9, 1, S6); t.set(8, 2, S6)
t.blit(102)

t = Tile(103)                               # spear bundle
for x, top in ((5, 1), (8, 0), (11, 2)):
    t.vline(x, C4, top, 13)
    t.set(x, top, S6)
t.blit(103)

t = Tile(104)                               # helmet
t.blob(8, 10, 4.6, Z2, 0.15)
t.rect(4, 10, 11, 11, Z1)
t.rect(7, 3, 9, 7, B3)
t.blit(104)

for idx, lit in ((105, False), (106, True)):  # brazier
    t = Tile(idx)
    t.rect(5, 8, 10, 13, Z1)
    t.rect(4, 7, 11, 8, Z2)
    t.rect(7, 13, 8, T - 1, Z1)
    if lit:
        t.blob(8, 5, 3.4, F1, 0.4)
        t.blob(8, 5, 2.2, F2, 0.4)
        t.set(8, 2, F3)
    t.blit(idx)

t = Tile(107)                               # small fire
t.blob(8, 12, 4.0, F1, 0.45)
t.blob(8, 11, 2.6, F2, 0.45)
t.set(8, 7, F3)
t.blit(107)

t = Tile(108)                               # embers
for _ in range(9):
    t.set(t.rng.randrange(3, 13), t.rng.randrange(9, 15), t.rng.choice((F1, F2, C2)))
t.blit(108)

t = Tile(109)                               # trough
t.rect(1, 8, 14, 13, C4)
t.rect(2, 9, 13, 11, N2)
t.blit(109)

t = Tile(110)                               # well rim
t.rect(2, 7, 13, 12, S4)
t.rect(4, 9, 11, 12, N1)
t.hline(7, S5, 2, 13)
t.blit(110)

# --- banners and overlays (row 7) -------------------------------------------

def banner(index, state):
    t = Tile(index)
    t.rect(3, 0, 12, T - 1, B2)
    t.vline(3, B1); t.vline(12, B1)
    for i in range(5):                      # lambda
        t.set(5 + i, 10 - i, Z3)
        t.set(9 - i, 10 - i, Z3)
    if state == "torn":
        for x in range(3, 13):
            for y in range(T - 1, T - 1 - t.rng.randrange(0, 6), -1):
                t.set(x, y, CLEAR)
    if state == "burning":
        for x in range(3, 13):
            cut = t.rng.randrange(0, 7)
            for y in range(T - 1, T - 1 - cut, -1):
                t.set(x, y, CLEAR)
            if cut:
                t.set(x, T - 1 - cut, t.rng.choice((F1, F2)))
    return t


banner(112, "flat").blit(112)
banner(113, "torn").blit(113)
banner(114, "burning").blit(114)

t = Tile(115)                               # hanging cloth
t.rect(4, 0, 11, 12, S3)
for x in range(4, 12, 3):
    t.vline(x, S2, 0, 12)
t.blit(115)

t = Tile(116)                               # rope
t.vline(8, C4, 0, T - 1)
for y in range(0, T, 2):
    t.set(7, y, C3)
t.blit(116)

t = Tile(117)                               # chain
for y in range(0, T, 3):
    t.set(8, y, Z2); t.set(8, y + 1, Z1); t.set(9, y + 1, Z2)
t.blit(117)

t = Tile(118)                               # dead vine
x = 8
for y in range(T):
    t.set(x, y, C3)
    if t.rng.random() < 0.4:
        t.set(x + t.rng.choice((-1, 1)), y, C4)
    x += t.rng.choice((-1, 0, 0, 1))
t.blit(118)

t = Tile(119)                               # smoke wisp
for _ in range(24):
    t.set(t.rng.randrange(4, 12), t.rng.randrange(0, T), t.rng.choice((N2, N3)))
t.blit(119)

sheet.save(OUT)
used = sum(1 for y in range(ROWS * T) for x in range(COLS * T)
           if sheet.getpixel((x, y))[3] > 0)
filled = sum(
    1 for i in range(COLS * ROWS)
    if any(sheet.getpixel(((i % COLS) * T + x, (i // COLS) * T + y))[3] > 0
           for y in range(T) for x in range(T))
)
print(f"wrote {OUT}  {sheet.size}  ({COLS}x{ROWS} tiles of {T}px)")
print(f"  {filled} tiles drawn, {used} opaque pixels")
