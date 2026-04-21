# State of the Game Address — 4/20/2026

As we come to this momentous PR, we are proud to show you all the ways this world has grown. What began as an empty room has become a place where materials have weight, fire reveals secrets, and discovery rewards the curious.

Here is what awaits you.

---

## Your Workshop

You wake in an enclosed stone workshop — four walls, a single doorway facing south, and a workbench at the center. Three shelves line the walls, each holding colored objects of different shapes and sizes. A glowing burner hums on the workbench. Beyond the doorway, a green expanse stretches outward.

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
- **Fabrication history** — which materials you combined and what they produced

The journal is your memory. As you test, combine, and re-test, it fills with the knowledge you've earned through play.

---

## What Lies Ahead

The carry system now lets you juggle multiple materials, but you can't yet feel their weight. Soon your steps will slow under a heavy load, your stamina will drain faster, and your carry strength will grow with use. The world beyond the doorway is waiting.

But for now — ten materials, one burner, one fabricator, a carry container, and a journal full of blank pages. The rest is up to you.
