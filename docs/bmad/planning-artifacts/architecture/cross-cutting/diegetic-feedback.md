# Cross-Cutting Concern: Diegetic Feedback Contract

The game never tells, only shows. No HUD popups, no progress bars, no explanatory UI. Every system must express state changes through world objects, visual consequences, or NPC reactions. This is an architectural constraint, not a style preference.

## Terrain Texture as Diegetic Channel

Terrain texture is the player's first-read signal about material physical properties. Crystalline, sedimentary, soil, and ice textures are diegetic information — not background art. The rendering system encodes material properties into texture by architectural rule. A crystalline surface looks crystalline because it has the physical properties of a crystal. The player learns material behavior by looking at the world, not by reading a tooltip.

This is a direct consequence of intrinsic material rendering: the same property vector that drives simulation also drives texture. The diegetic channel is the rendering output itself.

## Giant Flora Hazard Communication

Hazard states within giant flora interiors must be communicated entirely through in-world observable behavior:

- Visible flora movement as a seasonal or threat signal (closing petals, retracting tendrils)
- Fauna presence or absence as an environmental indicator
- Chemical visual effects in the atmosphere or on surfaces

No popup. No HUD indicator. No text. If a flower is closing, the player sees it closing. If a chemical is being released, it has a visible presence in the space. The player reads the world; the world does not read itself aloud to the player.

## Found Ship Brokenness

The found ship expresses its repair needs through visible damage and inoperable systems, not through tutorial UI, objective markers, or explanatory text. A broken thruster looks broken. A dark console has no power. A cracked hull has a visible crack.

The brokenness is self-evident by design. The player understands the repair task by inspecting the ship, not by reading a quest objective. This applies to every subsystem: each must have a visible broken state that communicates what is wrong without supplementary UI.
