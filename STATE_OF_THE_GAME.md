# State of the Game Address — 4/26/2026

As we come to this momentous PR, we are proud to show you all the ways this world has grown. What began as an empty room has become a place where materials have weight, fire reveals secrets, and discovery rewards the curious. The ground beneath you now belongs to a planet — one that orbits a star, in a system generated from a single seed.

Here is what awaits you.

---

## Your Workshop

You wake in an enclosed stone workshop — four walls, a single doorway facing south, and a workbench at the center. Three shelves line the walls, each holding colored objects of different shapes and sizes. A glowing burner hums on the workbench. Beyond the doorway, a green expanse stretches outward — the surface of your planet, with rolling terrain shaped by noise-driven elevation and biome-specific mineral deposits scattered across the landscape.

There is no tutorial. No quest marker. No explanation. The world teaches through consequence.

---

## Controls

| Action | Key | Notes |
|--------|-----|-------|
| Move | W / A / S / D | Relative to facing direction |
| Look | Mouse | Click into the window first |
| Pick up / Place | E | Context-sensitive — picks up when empty-handed, places when holding |
| Place (alternate) | R | Same as E when holding an item |
| Examine | Q | Inspect what you're holding or looking at |
| Stash | T | Move held material into carry container |
| Cycle carry | C | Swap held material with next carried item |
| Drop | G | Drop next carried item at your feet |
| Activate fabricator | F | Only when both input slots are filled |
| Open journal | J | Toggle your discovery journal |
| Release cursor | Escape | Frees the mouse from the game window |

All bindings can be changed in `assets/config/input.toml`.

---

## The Materials

Ten distinct materials sit on your shelves. Each one has a name, a color, and hidden properties waiting to be discovered.

**What you can see immediately:**
- **Color** — each material has a unique hue
- **Shape** — light materials are spheres, medium materials are capsules, heavy materials are cubes
- **Sheen** — highly conductive materials have a metallic glint

**What you cannot see** — not yet — are four hidden properties: heat response, reactivity, conductivity, and toxicity. These must be uncovered through experimentation.

---

## Picking Things Up

Look at a material within arm's reach and press **E**. It lifts into your hand, hovering at the edge of your view. You can carry one object at a time in your hand — but now you have pockets.

The crosshair tells you what's possible:
- **White** — nothing in range
- **Green** — you can pick this up
- **Gold** — you're holding something
- **Cyan** — you're aiming at an empty fabricator slot

Press **E** or **R** to set it down. Items land on surfaces you're looking at — the workbench, a shelf, or the floor if nothing else is in range.

---

## Carrying Multiple Materials

Your hand holds one material. Your carry container holds more.

Press **T** to stash what you're holding. It disappears from your hand into carry, freeing you to pick up something else. Press **C** to cycle — the held item goes into carry and the next carried item comes to hand. Press **G** to drop the next carried item at your feet without touching what's in your hand.

Every material has weight. Your carry container has a capacity limit based on your carry strength. If you try to stash something that would push you over, it stays in your hand — no error message, just refusal. The game shows, it doesn't tell.

The cycle order is configurable in `assets/config/carry.toml` — FIFO (oldest first) or LIFO (newest first), defaulting to FIFO.

---

## The Burner

The glowing disc on your workbench radiates heat. Place a material near it and wait.

What happens next depends on the material. Some will begin to glow and shift color. Some will soften and deform, their shape sagging under the heat. Others will barely react at all.

This is how you learn. After enough exposure, the material's **heat response** is revealed — visible the next time you examine it. The language starts uncertain ("Seemed to soften quickly") and grows more confident with repeated testing ("Reliably holds together under heat").

No indicator tells you when a property has been revealed. You discover it by examining the material again and noticing the `???` has been replaced with words.

---

## The Fabricator

Two small dark cylinders sit on the workbench — input slots. Place one material in each, then press **F**.

The slots pulse with violet light as the fabricator works. After a few seconds, both inputs are consumed and a new material appears on the output. It has a new name, a blended color, and a shape determined by its new density.

**The catch:** fabricated materials lose their known properties. Even if you knew everything about the inputs, the output starts as a mystery. You'll need to test it all over again.

Different material pairs produce different results. Some combinations average their properties. Some amplify them. Some produce inert waste. The only way to learn the rules is to experiment — and to write it down.

---

## The Journal

Press **J** to open your journal. Everything you've learned is recorded here automatically:

- **Surface observations** — the color and apparent weight of each material you've examined
- **Heat observations** — what happened when you put it near the burner, described with increasing confidence as you repeat the test
- **Weight observations** — how heavy the material felt when you picked it up, relative to your carry strength
- **Fabrication history** — which materials you combined and what they produced

The journal is built on a typed data model — every observation is categorized, timestamped with real game time, and keyed by material identity. Observations are deduplicated automatically and confidence upgrades as you repeat experiments. The journal is your memory, structured for the long game.

---

## Weight and Stamina

Carrying materials has consequences now.

The more weight you carry, the slower you move. A single light sphere barely affects your pace, but stash a handful of heavy cubes and you'll feel the difference. The speed curve is smooth — you won't hit a wall, just a steady drag that grows with load.

**Sprinting** (hold **Shift**) gives you a burst of speed, but it costs stamina. Stamina drains faster when you're carrying more weight. Stop sprinting and it regenerates — stand still to catch your breath faster. If your stamina runs out, you can't sprint until it recovers.

All of this is tunable in `assets/config/carry.toml` — sprint speed, base stamina, drain and regen rates are per-profile. Creative mode ignores weight and stamina entirely.

---

## The World Outside

Step through the doorway and you're standing on the surface of a planet. The terrain is procedurally generated from a seed — rolling hills shaped by multi-octave noise with detail overlays, wrapped on a torus so the world loops seamlessly. The ground height varies, slopes are computed per-vertex, and surface normals determine whether materials can be placed on a given patch of terrain.

Different regions of the planet belong to different **biomes** — each biome carries its own mineral palette, so the materials you find scattered on the ground change as you explore. The biome system is climate-aware: hot planets skew toward scorched biomes, cold planets toward frost.

Mineral deposits are scattered across the terrain surface-aware — they sit at the correct elevation, respect slope limits (no deposits on cliff faces), and their material identity comes from the biome palette with weighted random selection. Pick them up, carry them inside, test them. They're the same system as your workshop materials.

---

## The Solar System (Under the Hood)

Your planet doesn't exist in isolation. It orbits a star, in a system generated entirely from a single **solar system seed**.

From that seed:
- A **star** is derived — type (red dwarf, sun-like, blue giant, etc.), mass, temperature, luminosity, all from weighted random selection across a configurable registry
- An **orbital layout** distributes planets at increasing distances with minimum separation guarantees
- Each planet gets **environmental parameters** — surface temperature, gravity, atmosphere density, radiation levels — all derived from its distance to the star and the star's properties
- A **habitable zone** is computed from stellar luminosity, and planets inside it are flagged accordingly

The planet you're standing on was selected by index from this layout. Its terrain frequency, elevation amplitude, surface radius, and biome climate all flow from the stellar context. Change the system seed, and you get a different star, different planets, different biomes, different materials.

None of this is visible to the player yet — no star in the sky, no planet selection screen. But the data is there, deterministic and reproducible, waiting for the navigation epics to expose it.

---

## What Lies Ahead

The workshop is functional. You can gather, carry, heat, combine, and record. Your body responds to what you carry. The ground beneath you belongs to a planet in a solar system. The journal knows what you know, typed and timestamped.

**Ring 1 (Make Things)** is in progress — Epics 4 and 5 are complete, Epic 10 (Journal Architecture) has its data model landed, and Epics 11, 12, and 13 are next. **Ring 2 (Go Places)** — navigation, multi-planet travel, the wider universe — comes after.

But for now — ten materials, one burner, one fabricator, a carry container that slows you down, a journal that remembers everything, and a planet with biomes full of minerals. The rest is up to you.
