# Tileset Spec — Raided Spartan Village

Art brief for the first level. Everything here is fixed by the game already:
the grid is 16px, the palette is derived from the spartan sprite, and the tile
IDs are what `village.ldtk` and the loader will index.

**Target:** `assets/tiles/village.png` — 256×128, a 16×8 grid of 16px tiles.

---

## 1. Palette — use these 28 colours and nothing else

Load `assets/palettes/slasher.gpl` (GIMP/Aseprite) or `slasher.hex`. Preview:
`docs/palette.png`.

| Ramp | Colours | Use |
| --- | --- | --- |
| **Stone** | `1A1714` `2C2722` `423A33` `5A4F45` `77685A` `948373` | Masonry, columns, walls, rubble |
| **Terracotta** | `3A1912` `5C2A1C` `8E4529` `B25E35` | Roof tiles, pottery, packed earth |
| **Charred** | `0B0908` `1C1512` `33261E` `4E3A2B` | Burnt timber, ash, scorch |
| **Blood** | `2A0907` `4C110E` `7A1512` `A31C18` | Old → fresh |
| **Bronze** | `4A2E17` `7A5A30` `A87F45` `D8B267` | Shields, fittings, braziers |
| **Fire** | `C24A16` `E8842A` `F7C64F` | Flame and embers **only** |
| **Night** | `0D0F13` `171B22` `262E38` | Background, fog, deep shadow |

The first five ramps are pulled from the spartan's own 126 colours, so he sits
in the world rather than on top of it. Fire and Night are additions.

**Two rules that matter more than any individual tile:**

- **Only blood and fire may be saturated.** Everything else is grey-brown. That
  contrast is what makes violence read in a dark frame — if the pottery is as
  colourful as the blood, the blood stops meaning anything.
- **Keep tiles inside the middle value band** (roughly the Stone 2–5 range). Do
  not paint deep black shadow or bright highlights *into* the tiles. See §4.

---

## 2. Sheet layout

16 columns × 8 rows. LDtk numbers tiles left-to-right, top-to-bottom from 0, so
these IDs are what the level file will reference. **Keep tiles at their IDs** —
moving one silently rearranges any level already painted.

### Row 0 — Dirt ground *(IDs 0–15)*

Nine-slice plus inner corners. This is the shape LDtk auto-rules expect.

| ID | Tile | ID | Tile |
| --- | --- | --- | --- |
| 0 | corner top-left | 8 | corner bottom-right |
| 1 | edge top | 9 | inner corner top-left |
| 2 | corner top-right | 10 | inner corner top-right |
| 3 | edge left | 11 | inner corner bottom-left |
| 4 | fill A | 12 | inner corner bottom-right |
| 5 | edge right | 13 | fill B — cracked |
| 6 | corner bottom-left | 14 | fill C — rubble-strewn |
| 7 | edge bottom | 15 | isolated single block |

### Row 1 — Stone masonry *(IDs 16–31)*

Identical 16-tile layout, cut stone instead of packed earth. Use for platforms,
terraces, and anything built rather than trodden.

### Row 2 — Columns & architecture *(IDs 32–47)*

`32` column base · `33` column shaft · `34` column capital · `35` shaft cracked ·
`36` shaft snapped (jagged top) · `37` toppled column, horizontal · `38` step ·
`39` arch left · `40` keystone · `41` arch right · `42` lintel · `43` plinth ·
`44` rubble pile small · `45` rubble pile large · `46` loose block · `47` spare

Columns are the main vertical language of the level. Shaft (`33`) must tile
seamlessly with itself — it will be stacked 6–10 high.

### Row 3 — Walls & plaster *(IDs 48–63)*

`48` plaster fill · `49` plaster cracked · `50` plaster holed, stone behind ·
`51` plaster + blood spray · `52` wall coping/top · `53` wall corner ·
`54` doorway jamb left · `55` doorway jamb right · `56` doorway lintel ·
`57` window opening (near-black) · `58` shutter, broken · `59` wall with chain ·
`60–63` spare variants

### Row 4 — Burnt timber & collapsed roof *(IDs 64–79)*

`64` beam vertical · `65` beam horizontal · `66` beam diagonal `/` ·
`67` beam diagonal `\` · `68` beam broken end · `69` charred plank pile ·
`70` roof tiles intact · `71` roof tiles slipping · `72` roof collapsed heap ·
`73` thatch burnt · `74` thatch smouldering (ember specks) · `75` rafter
silhouette · `76–79` spare

### Row 5 — Blood & aftermath *(IDs 80–95)*

`80` splatter small, ground · `81` splatter large, ground · `82` pool ·
`83` pool with wet highlight · `84` wall spray, leaning left · `85` wall spray,
leaning right · `86` drips from above · `87` handprint · `88` drag mark ·
`89` scorch · `90` ash pile · `91` bone fragment · `92` corpse slumped,
armoured · `93` corpse prone · `94` shield with severed arm · `95` spare

### Row 6 — Props *(IDs 96–111)*

`96` amphora intact · `97` amphora broken · `98` shards · `99` basket ·
`100` discarded shield (lambda) · `101` shield split · `102` spear stuck in
ground · `103` spear bundle · `104` helmet · `105` brazier unlit ·
`106` brazier lit · `107` small fire · `108` embers · `109` trough · `110` well
rim · `111` spare

### Row 7 — Banners & overlays *(IDs 112–127)*

`112` banner intact · `113` banner torn · `114` banner burning · `115` hanging
cloth · `116` rope · `117` chain · `118` dead vine · `119` smoke wisp ·
`120–127` spare

---

## 3. Which tiles collide

The level's IntGrid carries collision; the tiles are only paint. Values:

| Value | Name | Meaning |
| --- | --- | --- |
| 1 | Solid | Full block. Ground, walls, column bases. |
| 2 | Platform | One-way — jump up through it, land on top. Roofs, beams. |
| 3 | Hazard | Damages on contact. Fire, spikes, embers. |

Paint collision first, art second. The auto-layer derives rows 0–1 from value 1,
so you draw the level's *shape* and the terrain appears.

---

## 4. Atmosphere — the part that isn't tiles

God of War 1/2's darkness is not dark textures. It is **low ambient light with
hot, local firelight**, and it is built in layers, not painted into the ground.

**Paint the tiles flat and mid-value.** If you bake deep shadow into a tile, it
is dark everywhere forever — under a brazier, in daylight, in fog. Flat tiles
can be darkened by the engine; pre-darkened tiles cannot be lit. This is the
single most common way a "dark" tileset ends up looking muddy instead of moody.

The atmosphere then comes from three things stacked:

1. **Background layer** — the village behind, drawn in the Night ramp at low
   contrast. Silhouettes of intact rooftops and distant smoke. Scrolls slower.
2. **Play layer** — the tiles in this spec, full palette, flat lighting.
3. **Foreground layer** — near-black silhouettes (`0B0908`) of beams, hanging
   cloth, doorway frames, scrolling *faster* than the play layer. This is what
   sells depth, and it costs almost nothing.

Then a global dark tint over the play layer with additive warm pools at each lit
brazier. That is the whole trick.

**On the gore:** make it sparse and specific. A village uniformly covered in
blood reads as texture and stops registering. One overturned cradle, one drag
mark leading into a dark doorway, one shield still gripped — restraint is what
makes it land. The Blood ramp should occupy maybe 2–3% of any given screen.

---

## 5. If you are generating this with an image model

Generate **one row at a time**, not the whole sheet — models lose grid alignment
across a large canvas, and a misaligned tile is unusable.

A prompt shape that works:

> 16x16 pixel art tileset row, 16 tiles in a single horizontal strip, 256x16
> pixels, dark ancient Greek village, weathered limestone masonry, muted
> grey-brown palette, flat even lighting, no shadows baked in, no outline glow,
> transparent background, sharp pixel edges, no anti-aliasing

Then state the tiles for that row explicitly.

Afterwards, hand me the strips. I have a pipeline for exactly this: earlier in
this project `tools/process_sprite.py` recovered the spartan off an opaque
backdrop, re-anchored every frame, and quantised out ~75k colours of resampling
noise. The same treatment — plus snapping to this 28-colour palette and checking
each 16px cell is actually aligned — is mechanical, and I can assemble the strips
into the final `village.png` with the IDs above verified.

---

## 6. First level shape

Keep it to roughly one screen: **40 × 22 tiles (640 × 352)**. Small enough to
retry instantly, which is the loop this game is built around.

Suggested read, left to right:

1. **Village edge** — intact-ish, quiet. Establishes normality. One body.
2. **Burning house** — collapsed roof forcing a jump, fire as a hazard.
3. **Agora** — open, columns, the first real fight.
4. **Exit** — a dark doorway or a breach in the wall.

Give the player 3–4 seconds of nothing before the first enemy. The quiet is what
makes the raid land.
