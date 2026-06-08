# Deep-Sign — Example Language Mod for Apeiron Cipher

This is the **reference example for language mods** that ships with Apeiron Cipher's
modding documentation. It adds Deep-Sign — the gestural light-language of the
bioluminescent Veth — as a fully defined language ready to integrate with the game's
language and translation systems.

---

## What this mod does

Deep-Sign defines a complete alien language from the ground up: a phonology (the atoms
of gesture and light), a grammar (spatial, orientation-relative, no word order), a core
vocabulary (50+ seeds, 10 starter words with unlock thresholds), localization hooks (base
game term translations that appear as the player learns words), and a race configuration
(the Veth, who speak it).

This gives modders a concrete, annotated example of every file and field the language system
will consume when it ships. It also demonstrates how a gestural / visual language differs
structurally from a vocal one — useful if you want to build a language that sounds like
nothing any human has heard.

---

## What is playable today vs. Epic 23

The language and race systems are planned for Epic 23+. Before that:

| Capability | Status |
|---|---|
| Language definition file loads, logs `AssetEvent::Added` | Ready when language asset loader ships |
| Vocabulary confidence tracking per word | Ready when language plugin ships |
| Journal shows Deep-Sign word alongside base-game term | Ready when localization hooks ship |
| Veth NPCs spawn and communicate | Requires Epic 23 NPC + language system |
| Language selection screen | Requires Epic 23 UI |
| Trade in Deep-Sign (fluency discount) | Requires Epic 23 economy + language integration |

**Today:** the mod files load cleanly via the base `AssetServer` path. The `schema_version`
field is present on every file. The data is structurally valid. When the language plugin
ships, it discovers these files automatically — no re-authoring needed.

---

## Language summary: Deep-Sign

Deep-Sign is the language of the Veth, a sapient cephalopod species native to the high-pressure
deep zones of ocean worlds. Because sound doesn't carry well at depth, the Veth evolved
communication through chromatic skin pulses and precise limb configurations.

To a player who doesn't speak it yet, a Veth NPC looks like a slow light-dance with no
discernible pattern. The first thing they will likely learn: that violet means "no."

### Phonology

Deep-Sign has no spoken component. Its "phonemes" are:
- **12 primary limb postures** × **4 body-axis orientations** = 48 base gesture atoms
- **5 chromatic pulse states**: deep blue (declarative), amber (interrogative), white
  (emphatic), violet (negation), null / black (sentence boundary / silence)
- **Pulse duration**: short (now / immediate), medium (near past/future), long (habitual /
  ancient)

### Grammar

Topic-Predicate-Space (TPS). Meaning is built from:
1. **Topic** — the entity under discussion, established by gesture or shared reference
2. **Predicate pulse** — the chromatic signal immediately after the topic
3. **Spatial qualifier** — body orientation and distance from observer

There is no word order in the linear sense. Everything is simultaneous or layered.

**Strangeness tier: 0.82 / 1.0** — highly alien. Expect a full in-game session to get
from "no idea what's happening" to "I think that was a question about stone."

### Grammar rules (unlocked progressively)

| Rule | Unlock threshold |
|---|---|
| Topic First — referent before predicate | Visible from first encounter |
| Color Marks Mood — blue/amber/violet/white meanings | 15% confidence |
| Orientation Is Emphasis — toward listener vs. general | 30% confidence |
| Duration Is Time — short/medium/long pulse = tense | 50% confidence |
| Stacked Chroma = Compound Meaning | 70% confidence |

### Starter vocabulary (10 words)

| Word | Meaning | Domain | Unlock |
|---|---|---|---|
| vel | light / visible | core | 10% |
| thar | shape / form | core | 10% |
| vel-thar | the language itself ("light-shape") | core | 20% |
| keth | stone / solid material | materials | 15% |
| sorh | heat source | materials | 20% |
| thess | trade / exchange | trade | 25% |
| zev | danger / caution | survival | 10% |
| nyr | deep / far / long ago | spatial | 30% |
| vel-keth | ore / glowing mineral (lit: light-stone) | materials | 35% |
| thess-nyr | ancient trade agreement | trade | 55% |

### Localization hooks

When Deep-Sign vocabulary is known, the journal annotates base game terms:

| Base term | Deep-Sign | Requires |
|---|---|---|
| Ferrite | Keth-Vel (iron-oxide glow) | vel-keth unlocked |
| Calcium | Nyr-Keth (ancient stone) | keth unlocked |
| Sulfurite | Sorh-Keth (heat-stone) | sorh unlocked |
| Prismate | Vel-Keth (light-stone) | vel-keth unlocked |
| Trade | Thess | thess unlocked |
| Agreement | Thess-Nyr | thess-nyr unlocked |
| Weight (journal) | Keth-Nyr | keth unlocked |
| ThermalBehavior (journal) | Sorh-Vel | sorh unlocked |

---

## Mod layout

```
example.deep-sign/
├── mod.toml                                        <- manifest (required)
├── README.md                                       <- this file
└── assets/
    ├── languages/
    │   ├── deep_sign.toml                         <- language definition (phonology, grammar, vocabulary)
    │   └── deep_sign_localization.toml            <- localization hooks (base game term translations)
    └── races/
        └── veth.toml                              <- race configuration (appearance, economy, language link)
```

---

## How to install

1. Copy the `example.deep-sign/` directory into the game data `mods/` folder.
2. Launch the game. The mod is discovered automatically via the `AssetServer` pipeline.
3. Find a Veth NPC (ocean world, thermal-vent biome). Their communication starts as pure
   light-pattern. Watch for recurring violet bursts — those are negation.
4. As vocabulary confidence builds, the journal's material and trade sections will show
   Deep-Sign terms alongside the base-game names.
5. At high fluency, the journal inverts: Deep-Sign becomes the primary label, base-game
   name appears as a translation note.

---

## How to adapt this template

### Defining a new language

1. Create `assets/languages/your_language.toml`. Copy from `deep_sign.toml`.
2. Set a unique `language.id` (snake_case). This ID ties the language to its
   localization file and any race that speaks it.
3. Set `modality`: `"gestural"` (visual only), `"vocal"` (spoken), or `"written"`.
4. Set `strangeness` (0.0–1.0) to control how alien it feels relative to the player's origin.
5. Define the phonology block appropriate to the modality.
6. Define grammar rules with `unlock_confidence` thresholds — the game reveals these
   progressively so the player builds understanding incrementally.
7. Define vocabulary entries. Each `word_id` is a knowledge-graph node key.

### Defining a vocal language

For a `modality = "vocal"` language, the phonology block changes:

```toml
[phonology]
type = "vocal"

# Consonant inventory — IPA-adjacent symbols the audio engine uses.
consonants = ["p", "t", "k", "x", "m", "n", "l", "r"]

# Vowel inventory.
vowels = ["a", "e", "i", "o", "u", "ä"]

# Whether tones carry semantic meaning (as in Mandarin, Yoruba).
tonal = false

# Average syllable structure (C=consonant, V=vowel).
# "CV" = simple, "CVC" = moderate, "CCVC" = complex.
syllable_structure = "CVC"
```

### Adding localization hooks

1. Create `assets/languages/your_language_localization.toml`.
2. Set `language.id` to match your language definition.
3. Add `[[localization.materials]]`, `[[localization.journal_labels]]`, and/or
   `[[localization.trade_terms]]` entries.
4. Set `unlock_word_id` to a vocabulary word_id from your language definition. The
   translation only surfaces once the player has learned that word.

---

## What's next for language mods (Epic 23+)

When the language and NPC systems ship, language mods will gain:

- **Live NPC communication** — Veth NPCs generate actual gestural animations from the
  phonology block during conversations
- **Language skill UI** — a dedicated screen showing the player's known words, confidence
  per word, and grammar rules unlocked
- **Trade language discount** — fluency in the NPC's language improves trade pricing
  (documented in the GDD: "trusted partner who speaks the language gets insider rates")
- **Procedural dialect variation** — NPCs from different Veth settlements will have
  minor phonological drift, generated from the same schema
- **Cross-family acquisition bonus** — learning one language in a family reduces unlock
  thresholds for others in the same family (Deep-Sign's `family = "spatial"` is the hook)

This mod is structured to receive all of those additions without changes to the existing
files. The phonology, grammar rules, vocabulary, and localization hooks are exactly the
data the language system will consume.

---

## Compatibility notes

- **No source code changes required.** This mod is 100% data-only.
- **No conflict with base game assets.** Deep-Sign uses new asset directories
  (`assets/languages/`, `assets/races/`) not present in the base game today.
- **Forward-compatible schema.** All files carry `schema_version = 1`. When the language
  system ships, it migrates older files forward automatically.
- **Hot-reload supported** in debug builds once the language plugin exists.

---

## License

CC-BY-4.0 — free to use, adapt, and redistribute with attribution.
