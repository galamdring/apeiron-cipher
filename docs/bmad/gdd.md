---
stepsCompleted: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
inputDocuments:
  - 'docs/bmad/game-brief.md'
  - 'docs/bmad/brainstorming/brainstorming-session-2026-03-13-1600.md'
documentCounts:
  briefs: 1
  research: 0
  brainstorming: 1
  projectDocs: 0
workflowType: 'gdd'
lastStep: 14
project_name: 'apeiron-cipher'
user_name: 'NullOperator'
date: '2026-03-14'
game_type: 'sandbox'
game_name: 'Apeiron Cipher'
---

# Apeiron Cipher - Game Design Document

**Author:** NullOperator
**Game Type:** Sandbox
**Target Platform(s):** PC (macOS, Windows, Linux); Mobile, VR (aspirational)

---

## Executive Summary

### Game Name

Apeiron Cipher

### Core Concept

You wake inside a wrecked ship. The instrument panel is covered in glyphs you can't read. Something is broadcasting on a frequency you don't recognize. Outside, an alien sky. No tutorial. No quest marker. No explanation. What do you do?

Apeiron Cipher is a procedurally generated open universe sandbox where knowledge is the only progression that matters. Every alien language, material property, cultural taboo, and system interaction is a cipher waiting to be decoded through direct experience. The game never confirms — it only reveals.

**The game is a mirror, not a rail.** The universe observes what players do and deepens the world in that direction. A player who gravitates toward geology finds richer mineral formations, more complex extraction challenges, traders who deal in rare materials. A player who starts learning an alien language encounters more speakers, more complex dialects, more cultural depth. The game presents opportunities, never paths. Engaging or walking past are both valid choices — neither is wrong, and both have consequences that ripple through the simulation. Missed a trade deal because a geological formation caught your eye? The deal resolves without you. The trader finds someone else, or the deal falls through and the economy shifts. The universe kept going. It always keeps going.

The core loop is **LEARN — explore, navigate, interact, try, talk — LEARN.** Five equal verbs, all available from the first moment of play, each running infinitely deep. The richest gameplay emerges at the intersections: exploring plus talking produces cultural geography; experimenting plus exploring produces material cartography. No single verb reveals the full picture. Understanding accelerates understanding.

Every system is infinitely deep. Hour 500 of mining should feel as rich as hour 5 — new layers of geology, exotic compositions, engineering challenges that didn't exist at the surface. If a system can be exhausted, it doesn't belong. If it doesn't expose another layer down, it needs redefinition. This is the standard every system is measured against.

When players shift their focus, the game meets them there. The factory builder who starts engaging with trade doesn't reset — they arrive with a factory builder's perspective on trade. The relationships they built with alien races for automation materials become trade relationships. The geological knowledge they earned feeds into resource valuation. Respecialization is growth, not starting over. Knowledge always carries forward, creating unexpected advantages in new directions.

This is a sandbox built on simulation-depth systems — material science with emergent alloy properties, procedurally generated alien races with learnable languages and discoverable cultures, automation through trainable NPC managers, and an economy where prices are relationships and knowledge artifacts are the most valuable trade goods. The same interconnected systems support radically different lives — miners, diplomats, translators, factory builders, explorers, collectors — each with infinite depth, all running on the same world. The player chooses their own depth, authors their own goals, and expresses their identity through what they build, what they understand, and who they become.

### Game Type

**Type:** Sandbox
**Framework:** This GDD uses the sandbox template with type-specific sections for creation tools, physics and building systems, sharing and community, constraints and rules, tools and editing, and emergent gameplay. The simulation, survival, and knowledge-progression systems aren't extensions to the sandbox — they're what make it a sandbox worth playing.

---

## Target Platform(s)

### Primary Platform

**PC (macOS, Windows, Linux)** — full first-class support across all three operating systems from a single Rust/Bevy codebase. Linux is treated as equal to Windows, not an afterthought. macOS is the development platform.

### Platform Philosophy

No reduced versions. Every platform that ships Apeiron Cipher ships the full experience. If a platform's input method or hardware can't deliver the real game without compromise, that platform waits until it can. The NMS Switch port — a multiplayer game shipped without multiplayer — is the anti-pattern.

### Secondary Platforms (Aspirational)

- **Mobile (iOS/Android):** Full gameplay is the goal, not a companion app. Ships only when touch input and mobile hardware can deliver the complete experience. The automation and delegation systems naturally support shorter sessions, but the full depth must be accessible.
- **VR:** Natural fit for a game about embodiment and presence on alien worlds. Deferred until the Bevy VR ecosystem matures and the core experience is proven.

### Performance Targets

"Runs on a 5-year-old laptop" is the aspiration. Specific frame rate and min-spec targets emerge from prototyping, but accessible hardware requirements are a design constraint, not an afterthought. The simulation depth lives in systems and data, not GPU-intensive rendering.

### Platform-Specific Features

- Steam achievements, cloud saves, and workshop support for community content
- Cross-platform save compatibility where possible
- Platform-specific input adaptation layers (keyboard/mouse, controller, touch) rather than separate clients

### Control Scheme

Keyboard and mouse as the primary input method — the complexity of material experimentation, language learning, factory management, and economic interaction benefits from precise, high-bandwidth input. Controller support for couch play and Steam Deck. Touch input designed only when it can deliver full functionality.

---

## Target Audience

### Demographics

- **Age:** 13+, core demographic 18-40
- **Engagement levels:** Casual through hardcore, unified by curiosity and self-direction rather than genre loyalty or time commitment
- **Cross-genre appeal:** Space sandbox veterans (NMS, Elite Dangerous), factory/automation players (Factorio, Satisfactory), emergent systems players (Dwarf Fortress, Rimworld), puzzle/discovery players, and players who don't identify with any genre but know they want something deeper

### Gaming Experience

**Broad spectrum.** The game provides familiar on-ramps — local economies with currency, libraries and experts, trading, collecting — that don't require system-mastery appetite to engage with. A player whose first instinct is "how do I earn money and buy that ship" has a valid, deep path ahead of them. A player whose first instinct is "what happens if I combine these two materials" has an equally deep path. The game doesn't demand a specific kind of player — it observes what kind of player you are and deepens the world accordingly.

### Genre Familiarity

No genre knowledge required. The game's familiar patterns — currency, buying, selling, collecting, exploring — are accessible to anyone who has played any game. The deeper systems (material science, procedural languages, cultural diplomacy) reveal themselves through play, not prerequisites.

### Session Length

Flexible from 15 minutes to 8+ hours without friction. Short sessions surface meaningful next steps through automation, delegation, and the mirror principle — the game always has something relevant waiting based on what you last cared about. Long sessions reward deep focus with emergent discoveries at system intersections. Pause-anywhere, fast resume, clear session boundaries.

### Secondary Audiences

- **Short-session players:** Adults with limited gaming windows. Automation and delegation systems serve them directly — check on managers, adjust a production line, have one alien conversation, log off knowing things keep moving.
- **Creative mode players:** Younger audiences and players who want expression without knowledge-progression pressure. Simplified systems, full construction freedom, no survival stakes. Separate UX consideration, deferred post-MVP.

### Player Motivations

- **Curiosity** — the unifying trait. Every player archetype starts with wanting to understand something: a language, a material, a culture, an economy, a ship design.
- **Self-direction** — the freedom to pursue your own goals without the game prescribing what matters.
- **Persistent expression** — building something that reflects who you are and what you've learned.
- **Discovery through familiar patterns** — economies, libraries, experts, and trading provide accessible entry points that naturally lead to deeper systems.
- **Growth without ceilings** — every direction runs infinitely deep. The game never runs out of interesting.

---

## Unique Selling Points (USPs)

**1. The Mirror Principle** — The game observes what players do and deepens the world in that direction. It presents opportunities, never paths. It responds to who you are as a player, never prescribes who to be, and never requires you to be the same player you were yesterday. This is the load-bearing architecture — every other USP works because the mirror works. Knowledge progression is compelling because the game mirrors your interest. Depth is meaningful because it deepens in your direction. Consequence matters because the game remembers who you are.

**2. Knowledge as Progression** — No other game makes understanding the sole progression system. Not XP, not unlocks, not gear score — comprehension of languages, materials, cultures, and systems IS the power curve. This requires the entire architecture to be built around it. Competitors cannot bolt it on.

**3. Infinite Procedural Depth** — Not just procedural generation of space but procedural generation of *learnable systems* — languages with grammar, materials with emergent properties, cultures with discoverable rules. Content that teaches itself to the player and never runs out. If a system can be exhausted, it doesn't belong. If it doesn't expose another layer down, it needs redefinition.

**4. Total Freedom, Total Consequence** — *Apeiron Cipher won't tell you no. But it will remember.* Nothing is forbidden — everything is consequential. Choose diplomacy or genocide. Connect your universe to a friend's or seal it from everyone. The difference isn't the principle — every sandbox claims freedom. The difference is the mechanism: the Legend Machine propagates your actions as stories that mutate through cultural telephone across civilizations. Three star systems away, a race you've never met already fears you based on a distorted account of what you did. Allies of the race you destroyed embargo your trade routes. A neutral faction offers asylum — for a price. Consequences don't just punish or reward — they generate new gameplay that wouldn't exist if you'd chosen differently.

**5. Structures as Identity** — Ships and bases wear, adapt, and accumulate history. Your oldest ship is your most reliable — adapted to the life it's lived. Your fleet is your autobiography. Collection is a valid endgame. Structures aren't disposable equipment — they're expression.

**6. Community-Deepened Universe** — Open source isn't the selling point — what it produces is. The game improves in directions players actually want because the people improving it are players themselves. No existing competitor can follow into open source, and no new project can easily replicate the combination of open source + procedural depth + knowledge-as-progression. Every contributor deepens how the universe teaches itself. The advantage accelerates over time.

### Competitive Positioning

Apeiron Cipher isn't competing for a slot in an existing market — it's creating a category. The first open source game where procedural systems teach themselves to the player, where the universe genuinely belongs to the player, and where the community continuously deepens the experience beyond any studio's output. The competition isn't any single game — it's the player's habit of switching between five games to get what one game should provide.

---

## Goals and Context

### Project Goals

**1. Creative Thesis:** Build a game that proves procedural systems can teach themselves to the player — that infinite depth is possible without infinite developer output. **Validation marker:** a new player engages the LEARN loop across two or more verbs within their first session without external guidance. If the core loop is compelling when the systems interact, everything else is iteration.

**2. Player Sovereignty:** Create the game where every player feels like this is *their* sandbox. Not playing in someone else's world under someone else's rules — owning the universe they're in, the choices they make, and the consequences that follow. A game that responds to who you are as a player, never tells you who to be, and never requires you to be the same player you were yesterday.

**3. Community Proof:** Attract contributors who believe in the vision. **Leading indicator:** session depth — how long does someone play before they hit a wall? If the answer is "they don't," the creative thesis is validated and community formation follows. **Lagging indicators:** downloads mean they found it, issues mean they care, contributions mean they want it to exist as much as you do.

**4. Sustainable Independence:** Patreon and Steam revenue sufficient to justify full-time commitment. Self-funded, no external investors, no publisher obligations. The game is always available free from the open source repository — every sale at ~$20 on Steam is a vote of confidence, not a gate to access. Financial independence preserves creative independence.

### Background and Rationale

No game has ever offered the opportunity to play the way its creator actually wants to. Every sandbox is someone else's sandbox — their rules, their limits, their vision of who you should be in their world. And the conviction is that millions of players feel exactly the same way.

The games that come closest each proved something was possible and revealed something that was missing. No Man's Sky proved a massive audience exists for procedural space exploration but left depth-seekers unsatisfied — everything is handed to you, progression is shallow, content is bounded by developer output. EVE Online proved players will invest thousands of hours in emergent systems but gates the experience behind PvP griefing and hostile onboarding. Factorio proved deep automation attracts dedicated communities but chose finite scope deliberately. Dwarf Fortress proved procedural depth finds devoted audiences but never solved accessibility. Star Citizen proved the appetite exists and proved that promising everything without delivering destroys trust.

Nobody combined all of it. Nobody built it open source. Nobody made the sandbox truly belong to the player.

Apeiron Cipher exists because the game its creator wants to play doesn't exist yet — and the conviction that if you build it right, open source it, and trust the players, a community will form around the same hunger.

---

## Core Gameplay

### Game Pillars

Six pillars in two categories. System Design pillars tell contributors how to build features. Player Relationship pillars tell contributors how the game treats the player.

**1. Learning First** — Understanding IS the progression. If a player can skip the learning, the feature is broken. The game never confirms, only reveals. Confidence is earned through experience.

**2. The Mirror** — The game observes what players do and deepens the world in that direction. It presents opportunities, never paths. It never requires you to be the same player you were yesterday. **Architectural implication:** when building a system, optimize for responsiveness to player interest *before* optimizing for depth. Build the observation and adaptation layer first, the infinite depth layer second.

**3. Consequence Over Restriction** — *Apeiron Cipher won't tell you no. But it will remember.* Nothing is directly off limits. The game never forbids an action — it makes consequences real and generates new gameplay from the choices made.

**4. Deeper** — Every system runs infinitely deep. No level caps, no content ceilings. Hour 500 should feel as rich as hour 5. If a system can be exhausted, it doesn't belong. If it doesn't expose another layer down, it needs redefinition.

**5. Systems** — Mechanics interconnect. Language affects trade affects diplomacy affects automation affects exploration. The richest gameplay lives at the intersections. If a system exists in isolation, it's not earning its place.

**6. Emergent Limits** — Constraints come from what you've built and learned, never from arbitrary designer-imposed caps. Your fleet capacity is how many docking bays you've constructed. Your trade reach is how many languages you speak. The game's limits ARE the game.

**Pillar Priority:** When pillars conflict: Learning First > Mirror > Consequence > Deeper > Systems > Emergent Limits. The player relationship comes before system depth. A shallower system that mirrors the player and respects their choices is better than a deeper system that ignores who they are. The systems serve the relationship, not the other way around.

### Core Gameplay Loop

There is no loop. There is a constant.

#### The Accretion Model

Traditional game design describes core loops as cycles: action → feedback → reward → motivation → repeat. Apeiron Cipher replaces this with the **Accretion Model**.

In geology, accretion is the gradual accumulation of material — sediment depositing layer by layer, each layer slightly different from the last, building something larger than any single deposit. No single grain is "the reward." The structure emerges from continuous accumulation over time.

In Apeiron Cipher, every action deposits understanding. There is no reward event. There is no "ding." The player doesn't work toward a moment — they accumulate continuously. The satisfaction isn't a spike, it's the growing awareness that you understand something today that you didn't yesterday. You look at a mineral formation and you *read* it now. That wasn't a quest completion. That was accretion.

**LEARN is not a phase. LEARN is the substrate.**

```
explore ──→ learn
navigate ─→ learn
interact ─→ learn
try ──────→ learn
talk ─────→ learn
```

**The Five Verbs** — all equal, all available from the first moment of play, all running infinitely deep:

- **Explore** — move through space, discover systems, planets, races, ruins. Produces spatial knowledge, geological intuition, awareness of the strangeness gradient.
- **Navigate** — pilot ships, chart routes, manage hazards. Produces route mastery, hazard awareness, efficiency.
- **Interact** — engage with anything — kill a creature, salvage a wreck, contact a government, broker peace, visit a library, consult an expert. Produces cultural knowledge, reputation, relationship.
- **Try** — experiment with anything — materials in a fabricator, an engine mounted backwards, an alloy exposed to radiation. Building IS trying. Produces material science, engineering knowledge, structural understanding.
- **Talk** — communicate with anything that might listen — an alien trader, a hostile patrol, a librarian who can point you toward deeper knowledge. Produces linguistic fluency, contextual understanding, nuance.

**Verb Intersections** — The deepest understanding lives where verbs combine. Explore + talk produces cultural geography. Explore + try produces material cartography. Talk + interact produces diplomatic leverage. No single verb reveals the full picture, naturally motivating players to diversify without the game ever telling them to.

**What keeps it fresh:** Each iteration is different because something has been learned. A new alloy with known base properties but unknown environmental behavior. A word that might mean "alliance" or might mean "surrender." A planet whose geology suggests materials you've never seen. The player is never repeating the same action — they're applying accumulated understanding to new situations, and every new situation produces more understanding.

**Timing:** There is no fixed cycle duration. A material experiment is minutes. Decoding a language relationship is sessions. Building a reputation with a civilization is weeks. The game supports all timeframes simultaneously, and the mirror principle ensures every session — whether 15 minutes or 8 hours — surfaces something relevant to what the player last cared about.

**Design Constraint — The Accretion Test:** When designing a system, don't ask "what's the reward?" Ask "what does the player understand after this action that they didn't understand before?" If the answer is "nothing new," the action isn't earning its place. If the answer requires a popup or notification to communicate, you've built a reward moment instead of accretion. The player should feel the knowledge shift through gameplay, not through UI. A well-designed accreting system changes how the player *perceives the world*, not what the world *gives them*.

**Design Constraint — Observable Consequences:** Every consequence must be eventually traceable by the player back to their action. If a player can't connect cause to effect, the consequence doesn't exist. This applies to all systems — trade embargoes, reputation shifts, Legend Machine stories, ecological changes, economic ripples. Consequences can be delayed, indirect, and distorted through cultural filters — but the thread must be followable. This is both a design principle and a QA testability requirement: if a consequence can't be verified to flow from a player action, the system is broken.

### Win/Loss Conditions

#### Victory Conditions

There is no win state. There is no final boss, no score to reach, no story to complete. Success is player-defined and ongoing. The collector who curates 300 ships has succeeded. The diplomat who brokers peace between ancient enemies has succeeded. The miner who discovers an alloy nobody else has seen has succeeded. Victory is the player looking at what they've built, what they understand, and who they've become, and feeling that it was worth the time.

#### Failure Conditions

There is no failure state. There are only consequences, and the player chooses what to do with them. Accidentally wipe out a civilization with the wrong verb conjugation during peace negotiations? Your reputation as a negotiator tanks. That's not failure — it's a fork. Take the long road of rebuilding your reputation, or pivot into a new line of work. The game generates new gameplay from every consequence.

#### Death

Death is possible. Return is unexplained.

You die. You wake up. You don't know why. Nobody in the universe knows why. The game never tells you. It's the one mystery Apeiron Cipher never resolves — not even for the player. The game never confirms, even about you. *Especially* about you.

Alien civilizations develop their own theories. Some worship you for your return. Some fear you. Some study you as a specimen. One ancient text in a dead language hints at something — but you can't be sure your translation is right. Your inability to permanently die becomes part of your legend. The Legend Machine absorbs your immortality into its stories. New races have myths about "the one who returns" before they've ever met you.

**Mechanically:** Death triggers a consciousness realignment period. Knowledge persists — you still know the Vath word for "alliance," you still recognize the mineral formation. But your *confidence* degrades. Translations arrive slower. Material intuitions become less certain. You re-earn certainty through experience, faster than the first time because the foundation is still there. Equipment must be recovered or replaced. Death changes how you play until you've re-settled — not as punishment, but as the natural lag of consciousness aligning with a new embodiment.

**Open Parameters (to be defined during implementation):** Confidence degradation percentage. Uniform vs. recency-weighted degradation. Whether death circumstances affect realignment duration. Recovery curve shape.

**Design Constraint:** Whatever death looks like in its final form, it must never make the player feel that learning was wasted. Knowledge persists. Confidence rebuilds. The game's trust in the player survives death.

---

## Game Mechanics

### Primary Mechanics

#### Tier 1: Absolute Core (Every Player, Every Session)

**Movement & Navigation**

Three modes of movement, all grounded in real physics simulation:

**Ship Flight — Three Tiers of Travel:**

| Tier | Mechanic | Skill Curve | What It Opens |
|---|---|---|---|
| **Newtonian Flight** | Real physics — momentum, inertia, drift. The ship has mass. | Foundational — always improving | Local maneuvering, docking, combat, asteroid fields |
| **Gravitic Drive** | "Grabbing space" — the pilot grips spacetime and pulls the ship through it. In-system fast travel as a skill, not a button. | Deep — infinite precision improvement. Early attempts overshoot, veer, lose grip. | Anywhere in-system, gravitational mastery, shortcuts |
| **Inter-System Travel** | **Throughways:** Established routes, set-and-forget. Safe, predictable, limited to the network. **Void Navigation:** Travel outside throughways through unspace. Dangerous, requires rare ability or deep training. Goes anywhere. | Throughways: minimal. Void: extreme — the deepest piloting skill. | Throughways: connected civilization. Void: unreachable systems, the true frontier, the strangeness gradient. |

Each tier teaches the player something about the universe. Throughways teach political geography — who's connected to whom. The gravitic drive teaches gravitational physics — star masses, orbital mechanics, system shape. Void navigation teaches something about the nature of space itself — something the game never fully explains.

Warp positioning has gravitational constraints — requires being on the system edge. Skilled pilots learn to land on the middle edge between systems, avoiding crossing either inner system. Navigation mastery through accretion, not skill unlock.

**On-Foot (Planetary):**
- Walking and sprinting with dual stamina — immediate burst cost and an overall pool requiring rest and sustenance.
- Food as exploration. Find vendors or forage in the wild. Foods and spices cause effects — the game presents the consequence, never explains the cause. Another cipher.
- Jump pack — available as equipment, upgradeable to planet-scale traversal.
- Stamina increases through use. No skills screen, no notification. The player notices they can sprint further than last week. Pure accretion applied to the player's own body.

**Vehicles:**
- Living mounts that listen to directions with varying degrees of compliance — another relationship to learn.
- Robotic vehicles with potential AI drive modules — can mine, assist in combat, or transport autonomously.

**Physics Philosophy:** Simulate real physics wherever possible. Low gravity, zero gravity, gravity storms with inversion. The technology exists to do better than competitors who take shortcuts. This is a differentiator and serves the Deeper pillar — physics mastery is its own infinite depth.

**No Stats Screen:** The accretion model extends to the player's own body. Stamina, physical capability, piloting intuition — all felt through gameplay, never presented as numbers. The game never confirms, even about you.

**Construction**

Every structure — shuttle to Dyson sphere — built from the same universal blueprint system of modular components. A ship is a structure with engines. A base is a structure on terrain. One system, infinite expression.

- **Placement:** Toggle between grid-snap and freeform. Interface assistance is acceptable here — help the player place things well, but give them freedom when they want it.
- **Salvage-to-Soul Pipeline:** Every found structure yields extractable components. Alien parts have characteristics you don't fully understand until you use them — or until cultural knowledge lets you read the warnings.
- **Living Structures:** Structures wear, degrade in different environments, and improve with use. Stressed and survived components develop micro-structural improvements. Your oldest ship is your most reliable.
- **Scale as Commitment:** Large constructions (stations, megastructures) require proportional investment — mass, energy infrastructure, time, possibly alien cooperation. Not a blueprint unlock — a project.

#### Tier 2: Core (Near-Universal)

**Material Science**

- Experimental metallurgy — combine materials in fabricators, observe emergent properties. The game doesn't reveal all properties upfront. Add an element to steel, discover it resists radiation when you survive a pulsar pass. Discover it corrodes in humid atmospheres when your hull degrades six hours later.
- Discovery journal records only what the player has personally observed.
- Theoretical learning is valid — libraries, alien technical texts, expert consultation. Slower than hands-on experimentation and potentially costly, but gives buffs to subsequent manual practice. Reading about radiation-resistant alloys before experimenting makes the experimentation more productive.
- Recipes exist as learned knowledge — reminders of combinations that worked — but the tools always let you push further.

**Language & Communication**

- Procedurally generated alien languages with consistent grammar per race. Generated audio voices with per-race sound profiles — the sound IS the language before you understand the words. Text component with ASCII or pictographic writing systems, visually distinct per race.
- Languages near the player's origin share structural similarity with the player's configured native language — basically word replacement. Complexity increases with distance from origin: SOV grammar, agglutinative structures, spatial grammar, temporal grammar. Language difficulty is a natural exploration difficulty curve.
- Confidence spectrum — every word has an invisible confidence score that builds with context. Low confidence shows best guesses, which are sometimes wrong. Misunderstanding is gameplay, not failure.
- Linguistic families share structural DNA — learn one language and the second in its family comes faster. Bilingual artifacts and bridge races accelerate cross-family learning.

**Economy & Trade**

- Available from minute one. Gather resources, find a nearby vendor, sell for local currency, buy food. The accessible on-ramp.
- No universal currency — different races use different value systems. Converting between them requires cultural understanding or a trader who bridges them.
- Relationship-based pricing — stranger pays maximum, trusted partner who speaks the language gets insider rates.
- Physical supply and demand — resource value is spatial. Players can cause market shifts.
- Knowledge artifacts, trained managers, and specialist services are the highest-value trade goods.

**Cultural Interaction**

- First contact spectrum — disposition matrix from territorial to curious to ritualistic. First interaction sets a trajectory, not a state.
- Territorial intelligence — spheres of influence, patrol routes, sacred zones. Rules are discoverable content, not UI markers.
- The Legend Machine — actions create stories that propagate and mutate through cultural telephone. Reputation precedes you across systems.
- All knowledge is transferable. Players can share cultural expertise with other players — including setting up teaching storefronts at trading posts. A player deep in Vath culture can teach others the finer points before they visit Vath space.
- Libraries as in-game institutions — some cultures share knowledge freely, others gate it behind relationship or payment. Library access policies reveal cultural values. Time spent researching subjects accelerates subsequent hands-on learning.

#### Tier 3: Deep Core (95% of Players)

**Automation & Delegation**

- Available early game. Starter kits sold in nearby villages — tourist trap stores offering kits for any profession. Discovery required, but not gated behind progression.
- Automation becomes viable once a system is profitable — managers expect payment. This is an economic decision, not a level gate.
- Trainable managers — entities you train by demonstration. They watch you work, learn the pattern, offer to automate. Imperfect early, reliable over time.
- Process documents produced by managers in their racial language. Quality degrades through transfer. Player language skill enables proofreading and improvement.
- Cross-cultural factory dynamics — multi-racial workforce with emergent interactions based on racial compatibility.
- Declarative logistics — declare intent, drones solve routing. Drone personality from racial traits.

### Mechanic Interactions

Every mechanic category touches at least two others:

- **Material Science + Construction** — what you discover determines what you can build and how it performs
- **Language + Economy** — speaking a race's language gets you better prices and access to restricted goods
- **Language + Cultural Interaction** — understanding culture requires understanding language, and vice versa
- **Economy + Automation** — automation is an economic investment; managers cost money; products generate revenue
- **Movement + Material Science** — what you find depends on where you go; planetary geology varies
- **Construction + Automation** — what you build determines what can be automated
- **Cultural Interaction + Economy** — reputation affects pricing; cultural knowledge unlocks trade goods
- **Navigation + Knowledge** — gravitic drive mastery opens in-system shortcuts; void navigation opens the frontier; throughway knowledge reveals political geography
- **All Knowledge + All Players** — every type of knowledge is transferable, creating a player-driven knowledge economy

### Mechanic Progression

No mechanic has a skill tree, level system, or unlock gate. All mechanics are available from the first moment of play. Progression is accretion:

- **Movement:** You don't unlock better flying. You develop piloting intuition. You learn to read gravitational fields. Your gravitic drive grip gets more precise. Your warp positioning improves through practice. Void navigation opens through rare ability or deep training.
- **Construction:** You don't unlock building tiers. You discover new materials and alien components that expand what's possible. Structural knowledge grows through experimentation.
- **Material Science:** You don't unlock recipes. You discover combinations and learn properties through observation over time. Theoretical study buffs subsequent hands-on work.
- **Language:** You don't unlock vocabulary. Your confidence builds word by word, context by context.
- **Economy:** You don't unlock markets. You build relationships that open access and improve prices.
- **Cultural Interaction:** You don't unlock reputation tiers. Your legend grows through action and propagates through the simulation.
- **Automation:** You don't unlock automation. You earn enough to hire a manager and train them through demonstration.

---

## Controls and Input

### Control Scheme (PC — Primary)

Keyboard and mouse as the primary input method. The complexity of simultaneous systems — piloting while scanning while translating while managing inventory — benefits from high-bandwidth input.

**Design Principles:**
- Frequency equals accessibility — movement and camera on the most natural inputs (WASD + mouse)
- Context-sensitive input — the same key can do different things in ship vs. on-foot vs. in-fabricator contexts, reducing total keybind count
- Fully rebindable — every player customizes to their preference
- No hand gymnastics — avoid uncomfortable modifier combinations for common actions

**Controller Support:** Full controller mapping for Steam Deck and couch play. Radial menus and context wheels compensate for reduced button count. The game must be fully playable on controller without losing functionality.

### Input Feel

- **Ship flight (Newtonian):** Weighty, momentum-based. The ship has mass. Turning takes force. Drift is real. Precision comes from skill, not from the ship snapping to where you point.
- **Ship flight (Gravitic):** Visceral and demanding. The act of gripping spacetime should feel like effort — controlled power, not autopilot. Releasing feels like letting go of something physical.
- **On-foot:** Responsive but grounded. Sprint feels like effort. Jump pack feels like thrust, not teleportation. Weight varies with gravity — low-gravity planets change how movement feels.
- **Construction:** Precise and assistive. Grid-snap for structure, freeform for expression. Placement preview before commitment. Undo available.
- **Fabrication/experimentation:** Tactile and hands-on. Drag materials into fabricators. Observe results physically. The interface is the fabricator, not a menu.

### Accessibility Controls

First-class concern, not a post-launch patch. Specific features defined iteratively with community input. Baseline commitments:
- Subtitles for all alien voice audio with visual language indicators
- Rebindable controls on all platforms
- Colorblind modes for any color-dependent systems
- Haptic feedback options
- Scalable UI for readability
- Session pause-anywhere support

---

## Sandbox Specific Design

### Creation Tools

**The 3D Printer Blueprint System:**

Every player starts with a 3D printer as part of their starter kit — the fundamental creation tool. Building is never accidental. The process is intentional:

1. **Design Phase** — Enter the visual editor to plan your structure in 3D. Whether it's a planetary base or strapped to an engine as a ship, the same editor handles all construction. The design serves as a blueprint with full material estimation — the player sees exactly what's needed before committing.
2. **Confirmation** — The player reviews the design and required materials, then explicitly confirms. UX provides clear feedback when available materials are exceeded. Nothing is built without intentional commitment.
3. **Partial Building & Queuing** — The player can confirm a design and build with available materials while queuing the rest. Start construction while still collecting remaining resources. The build process is visible and physical.
4. **Construction** — Materials move from the printer to the designed location, either manually carried or via drones. Construction is observable — you watch your design materialize in the world.

**Placement:** Grid-snap and freeform toggle. Interface assists without demanding perfection, but gives freedom when the player wants it.

**Templates & Registry:** Designs can be saved as templates. The community design registry is in-game and browsable — players upload and share creations. Other players can browse, download, and build from shared blueprints. A companion app for visual design outside the game is a potential future consideration.

**Undo:** No arbitrary limits on build data — structure data is lightweight. Design phase allows unlimited iteration before confirmation. Once confirmed and construction begins, changes require physical deconstruction.

### Physics and Building Systems

**Structural Physics:**

Physics applies to all structures, always — including creative mode. The simulation doesn't need to be perfectly accurate, but it needs to feel realistic and produce learnable consequences:

- Build too tall without enough structural support? Your tower falls in the wind.
- Build on a low-gravity moon? Height matters less — the physics responds to the environment.
- Forget a roof? Weather gets in. Materials degrade from exposure.
- Build on an unstable surface? The ground shifts. Your foundation matters.

**Destruction Mechanics:** Structures can fail from physics, environmental forces, or combat. Destruction is physical — pieces fall, materials scatter, components can be salvaged. Destruction is never arbitrary — it follows from the physics simulation, making structural engineering a learnable skill.

**Material Properties:** Different materials have different structural properties — weight, tensile strength, thermal resistance, radiation shielding. Material science feeds directly into construction. The alloy you discovered in the fabricator becomes the hull plating that survives a radiation storm because you understood its properties. Building IS applied material science.

**Environmental Interaction:** Structures interact with their environment over time. Corrosive atmospheres degrade certain materials. Extreme temperatures stress joints. Structures on seismically active planets need different engineering than those on stable moons. The environment is a design constraint the player learns to read.

### Sharing and Community

**Base Uploading:** Players choose whether to upload their bases — opt-in, like NMS. Visiting a planet shows uploaded player bases. Your base is your mark on the shared universe.

**Design Registry:** In-game browsable catalog of player-uploaded designs. Players can share ship blueprints, base templates, extractor configurations, and factory layouts. Potential future feature: in-game currency earned for contributing popular designs to the shared ecosystem.

**Knowledge Sharing:** All knowledge is transferable. Players set up teaching storefronts, share cultural expertise, trade process documents, and publish research. The community isn't just building structures — it's building a shared understanding of the universe.

**Open Source Community:** The game itself is open source. Contributors deepen systems, add depth, and expand what's possible. The community design registry is the in-game mirror of the open source development philosophy.

### Constraints and Rules

**Creative Mode:** Zero building limits. Unlimited resources, full construction freedom, no survival stakes. Physics still applies — structures must be physically sound, but the cost of experimentation is zero. Creative mode is a separate UX consideration, deferred post-MVP.

**Survival Mode (Core):** Resources are real. Materials cost time and effort. Construction is an investment. The 3D printer blueprint system makes that investment intentional — you see the cost before you commit.

**PvP — Consent-Based Only:**

The one absolute restriction in a game built on "the game won't tell you no": **no player can grief another player.** PvP requires explicit opt-in from both parties. Opting in is never permanent — a player can opt out at any time.

PvP isn't limited to ship-to-ship combat:
- **Ship combat** — consensual engagement in space
- **Arenas** — dedicated spaces for hand-to-hand or weaponed combat
- **NPC combatants** — arena fighters, gladiatorial events, a potential career path
- **PvP systems/zones** — some systems could be PvP-enabled, requiring confirmation before entering

Combat and arena management could become a full career choice — building arenas, recruiting NPC fighters, running tournaments, earning reputation in the combat circuit.

**Build Limits:** No artificial limits on construction size or complexity. Structure data is lightweight. The constraints are physical — materials, structural integrity, environmental conditions — not designer-imposed caps. This serves the Emergent Limits pillar: the limit is what you've built and what you can sustain.

### Tools and Editing

**Terrain Reshaping:** A learned skill requiring materials. Not available from minute one — the player discovers terrain modification tools and learns to use them. The terrain is part of the world, and reshaping it is an investment with consequences (ecological, diplomatic, structural).

**Automated Behaviors:** Structures and vehicles can have automated behaviors. A block-style visual editor limits the possible logic to prevent exploits while giving players meaningful control — think programmable drones, automated defense systems, production line routing. The exact scope of automation scripting is an open design question with the constraint that it should be learnable through the block editor, not require programming knowledge.

**3D Blueprint Editor:** The in-game visual editor serves as both planning tool and construction interface. Design in 3D, see material requirements, estimate costs, confirm and queue. This is the primary creation interface — building is designing, then watching the design become real.

**Testing/Preview:** The design phase IS the preview. The player sees their structure in the world before confirming. Material feedback shows what's available, what's needed, what's queued. No separate test mode needed — the blueprint is the test.

### Emergent Gameplay

**Intentional Learning:** Every creation tool is designed around intentional action producing learning. The player confirms before building. They see the material cost. They observe the physics. They learn structural engineering by watching what stands and what falls. Nothing sneaks up on them — the accretion model applies to building just as it does to everything else.

**Unintended Creations:** The universal blueprint system — where a ship is a structure with engines and a base is a structure on terrain — means players will combine components in ways the designers never imagined. A base strapped to a dozen engines. A ship with a factory inside it. A bridge between two asteroids. The system doesn't care what you build — it simulates the physics and lets the result stand or fall.

**Community Challenges:** The design registry, PvP arenas, and knowledge economy create natural community dynamics — design competitions, arena tournaments, exploration races, trade wars. These emerge from the systems, not from developer-scripted events.

**Cross-Creation Interaction:** Structures in the shared universe interact. A player's mining operation affects the local economy. A player's uploaded base becomes a landmark others navigate by. A player's arena becomes a destination. The emergent gameplay is the universe responding to what players collectively build.

---

## Progression and Balance

### Player Progression

#### The Accretion Model as Progression

Apeiron Cipher has one progression system: **accretion**. Collection, social standing, content access, and narrative advancement are not separate tracks -- they are facets of the same continuous knowledge accumulation. Knowledge always carries forward; this is stated once and applies everywhere in this document.

**Example -- the interceptor collector path:** A player drawn to fast, angular ships starts with an echo locator tool that pings nearby vessel signatures. They learn to read the pings, build better scanners from materials they've gathered, and eventually internalize the ability to feel harmonic signals without tools. Following those signals doesn't just find ships -- it leads to the shipbuilders' origin system, their culture, their engineering philosophy. The collection path became an exploration path became a cultural path. No track switched. Knowledge accreted.

**Social progression** follows the same model. Learning a language word by word builds into cultural fluency. Cultural fluency builds into diplomatic leverage. Diplomatic leverage builds into trade access. Each layer is knowledge earned, never a tier unlocked.

#### Pacing

Pacing is entirely player-driven. The game offers knowledge opportunities naturally -- environmental affordances, NPC behaviors, material interactions -- but never dictates tempo. A player can spend forty hours in one system learning its geology, or skim across twenty systems in the same time. Both produce meaningful accretion.

Depth and width are both valid. Going deep means more layers within a single domain. Going wide means more connections between domains. Neither is faster or slower -- they're different shapes of the same growth.

#### Cold Start: The First Fifteen Minutes

The first fifteen minutes are the one place where environmental design is front-loaded. The wrecked ship, the unreadable instrument panel, the broadcasting signal -- these are designed affordances that create traction without guidance. The environment is dense with interactable surfaces that reward curiosity: a panel that responds to touch, a material that glows when combined with another, an NPC whose body language communicates before their words do.

This is not a tutorial. Nothing explains what the player should do. The affordances create a natural gradient of "this seems interactable" that bootstraps the mirror -- once the game has enough signal about what the player gravitates toward, the mirror takes over and the designed affordances fade into the simulation's natural density.

The bootstrap is a gradient property of the mirror itself, not a separate phase. The mirror runs one continuous algorithm; the player's signal density is the only variable. Early play means low signal and high environmental lean. Late play means high signal and subtle lean. There is no switch, no state transition -- only a ratio that shifts as the player's behavior gives the mirror more to work with.

**Design tension acknowledged:** "The game never guides" meets "the first minutes must create traction." Resolution: the cold start is environmental design (what's present in the space), not behavioral design (what the game tells you to do). The affordances are physical objects with discoverable properties, not breadcrumbs. After the mirror bootstraps, this front-loading stops.

#### Respecialization

When players shift focus, knowledge carries forward as perspective. The factory builder who starts trading arrives with a factory builder's understanding of supply chains, material costs, and production timing. The geologist who starts learning languages brings spatial thinking to grammar structures. Respecialization is growth, not reset.

The game meets players where they move. Shifting attention triggers the mirror to deepen the new direction while maintaining what was built. No system resets. No starting over. The player's history is their advantage in whatever they do next.

#### Reflection Surfaces: How the World Shows You Your Own Growth

The game provides surfaces where past and present contrast, making growth visible without quantifying it:

- **Discovery journal** -- records only personally observed facts. Flipping back reveals how much the player's understanding has deepened since early entries.
- **NPC memory** -- characters reference shared history. "You've learned a lot since you first stumbled into my shop speaking broken Keth" is a mirror, not a tutorial.
- **Maintenance logs** -- factories, ships, and bases accumulate history. The repair log on a ship tells a story of increasingly sophisticated engineering decisions.
- **Revisiting known spaces** -- returning to early areas with deeper knowledge reveals layers that were always there but invisible. The rock formation was always geologically interesting; now you can read it.
- **NPC behavior shifts** -- merchants offer different goods, experts engage at a different level, alien cultures open doors that were previously invisible. The world reflects what the player has become.
- **Structural evidence** -- the player's constructions, factories, and trade networks are physical proof of accumulated understanding.

Legibility always has a horizon. Deeper knowledge reveals further unknowns -- not because the game is teasing more content, but because infinite depth means every answer exposes questions that weren't previously visible. This is an emergent property of the systems, not a designed breadcrumb trail.

**Economic consequences connected to the Legend Machine:** Market manipulation, trade monopolies, and economic disruption generate stories that propagate through the Legend Machine. A player who corners a local market doesn't just see price changes -- they encounter NPCs discussing the shortage, traders adjusting routes, and cultural attitudes shifting. The economy is a reflection surface too.

**Knowledge economy as felt progress:** Players who develop deep expertise can monetize it -- translation services, cultural consulting, process documents, teaching storefronts at trading posts. Seeing other players seek out your knowledge is progression made tangible through social proof.

#### The Legend Machine as Progression Mirror

The Legend Machine doesn't just track reputation -- it reflects the player's journey back to them through the world's response. A player known for geological expertise gets approached by mining consortiums. A player known for diplomatic skill gets invited to mediations. The universe's response to the player IS the progression feedback.

#### Multiplayer Knowledge Asymmetry

A veteran player's accumulated knowledge isn't just a personal advantage -- it becomes an opportunity for newcomers. Teaching storefronts, player experts selling domain knowledge, and cultural consulting create a natural mentorship economy. The veteran's depth becomes the newcomer's on-ramp, and the transaction benefits both: the veteran earns from their expertise, the newcomer accelerates without the game hand-holding them.

### Difficulty Curve

#### Depth, Not Distance

Difficulty in Apeiron Cipher increases with depth, not with time played or distance traveled. Every domain -- engineering, culture, materials, economy, language -- has its own depth curve. Surface-level engagement in any domain is approachable. Deeper engagement reveals more complex interactions, rarer edge cases, and subtler distinctions.

The strangeness gradient reflects this: systems near the player's origin are more familiar, more structurally similar to the player's configured starting knowledge. Distance from origin correlates with alienness -- languages diverge further from the player's native structure, cultures become less recognizable, material properties become more exotic. But this is spatial depth, not difficulty gating. A player can engage at any depth in any location.

#### No Spikes, Continuous Deepening

There are no difficulty spikes. No boss fights gate progress. No skill checks block advancement. The difficulty curve is continuous -- each layer of understanding prepares the player for the next layer. A player who understands basic metallurgy is equipped to notice that a new alloy behaves unexpectedly. A player who speaks basic Keth is equipped to notice when a dialect shifts.

The curve deepens, never jumps.

#### Exit Hatches

When a player is stuck -- genuinely unable to progress in a domain they care about -- the game provides exit hatches that assist without guiding:

- **Libraries** -- in-game institutions where knowledge is researched. Libraries suggest areas of study based on what the player has been doing, never solutions. Time spent researching buffs subsequent hands-on practice. Some cultures share knowledge freely; others gate it behind relationship or payment. Library access policies are themselves cultural content.
- **Player experts** -- other players who sell domain knowledge through teaching storefronts. A player stuck on Vath diplomacy can pay a player who's deep in Vath culture for a crash course. The expert teaches, doesn't do it for them.
- **NPC apprenticeships** -- guided mentorship where an NPC demonstrates techniques in their domain. The NPC shows, the player practices. Imperfect early, reliable over time. Apprenticeship is a relationship, not a tutorial -- the NPC has personality, expectations, and a teaching style that reflects their culture.

All exit hatches assist. None guide. The player still has to do the work.

#### The Depth Test: Hour 5 / Hour 50 / Hour 500

Every system is measured against the depth test: does engagement at hour 5, hour 50, and hour 500 each feel meaningfully different and equally rich?

**Worked example -- Material Science:**
- **Hour 5:** Player combines copper and tin in a fabricator, discovers bronze. Observes it's harder than either component. Notes it in the discovery journal. Builds a better hull plate.
- **Hour 50:** Player understands thermal coefficients, knows which alloys resist corrosion in specific atmospheres, has discovered a rare composite that conducts energy in unexpected ways. Begins theorizing about combinations they haven't tried. Reads alien technical texts to accelerate research on radiation-resistant composites.
- **Hour 500:** Player is reverse-engineering alien alloys from salvaged artifacts, cross-referencing material properties across three racial engineering traditions, publishing process documents that other players buy, and has discovered emergent properties that exist only at specific temperature-pressure combinations found on a single moon they've mapped extensively.

If any system fails this test -- if hour 500 feels like hour 50 with bigger numbers -- the system needs redesign. Minimum viable depth is a development milestone, not an aspiration.

### Economy and Resources

#### Local Markets, Local Currency

Every economy is local. Markets are tied to geography, culture, and the races that inhabit them. Currency is local -- different races use different value systems based on their cultural priorities. Converting between currencies requires cultural understanding or a border node trader who bridges the gap.

Prices are relationships. A stranger pays maximum. A trusted partner who speaks the language, understands the cultural norms, and has a reputation in the community gets insider rates. Economic knowledge is cultural knowledge.

#### Player Market Influence

Players can meaningfully influence local economies:

- **Corner a local market** -- buy up all supply of a critical material, set your own prices
- **Crash an economy** -- flood a market with cheap goods, disrupting local producers
- **Create artificial scarcity** -- control supply chains through automation and logistics
- **Build trade empires** -- connect multiple local markets through border node traders, arbitraging price differences

The economy is alive and consequential, not balanced. Player actions have real effects that ripple through local systems and propagate through connected markets.

#### Economic Resilience

The economy self-regulates through locality. A player can corner a local market, but similar materials exist elsewhere with their own local markets, their own currencies, their own cultural contexts. Total economic domination requires mastering not just trade but language, culture, and geography across multiple systems. The ceiling is soft and always receding. Market manipulation generates stories through the Legend Machine -- NPCs discuss shortages, traders adjust routes, cultural attitudes shift. Economic disruption is content for everyone it touches.

**Subsistence guarantee:** Foraging is always available in any habitable environment. A player in economic crisis can always feed themselves. Additional supplementary mechanisms -- odd jobs, salvage, basic trade -- ensure that economic failure is a setback that creates interesting gameplay, never a dead end that stops play. Players affected by another player's market manipulation experience economic disruption as content -- scarcity creates opportunities for resourceful players.

### Death

**Lore:** The player returns. The mechanism is unexplained and remains unexplained. This is a deliberate narrative gap -- the universe has properties the player doesn't understand, and death-return is one of them.

**Mechanics:** Death shakes observation confidence -- the player's ability to trust their own perceptual judgment wavers temporarily. This isn't knowledge loss. The player still knows everything they knew. But their confidence in reading environmental cues, interpreting signals, and trusting their instincts is diminished for a period.

Recovery is domain-weighted. Re-engaging with the specific system that killed you -- the material that exploded, the environment that proved hostile, the navigation error that led to a collision -- recovers confidence quickly. The player jumps back in, and the system echoes that: confidence wasn't truly shaken, so it restores fast. Pivoting away to a different domain signals genuine uncertainty -- the player is avoiding what got them, and the system reflects that too. Confidence lingers in the background, recovering slowly, until the player returns to the relevant context. The game reads the player's response to death and mirrors it back.

**What this produces:** Self-directed reflection. The player asks "what did I miss?" -- not because the game prompts them, but because their diminished confidence makes them look more carefully. The death created a perceptual gap that the player fills by paying closer attention. The knowledge was always available; death redirected attention to what was previously unnoticed.

**What this prevents:** Death as a hint mechanism. Because confidence loss is perceptual rather than informational, dying repeatedly in the same way doesn't reveal new information. The game doesn't reward death with clues. The player has to actually observe differently, not just die and retry.

**Design tension:** Death must feel consequential without feeling punishing. The confidence mechanic creates a subjective experience of setback -- the world feels slightly less readable -- without removing any objective capability. The player is never weaker after death, just temporarily less sure.

**Open parameters for implementation:**
- Duration of confidence reduction
- Magnitude of perceptual dampening
- Whether confidence loss is uniform or weighted by domain relevance (weighted by domain relevance, per party discussion)
- Recovery curve shape (player engagement echoed: re-engaging the relevant domain signals confidence, system reflects fast recovery; avoiding it signals genuine uncertainty, system reflects slow recovery)
- Visual/audio representation of reduced confidence

---

## Level Design Framework

### Structure Type

Procedurally generated open universe -- no levels, stages, gates, or unlock sequences. "Level design" in this context means environmental design: the principles governing how spaces are generated, what makes them coherent, and how they reveal depth through the player's growing knowledge.

### Environment Types

Ten environment types across three categories. Every type is subject to the depth test (hour 5/50/500) -- if engagement at any timescale feels exhausted, the environment type needs redesign.

#### Planetary

**Surface** -- terrain, biomes, settlements, flora, fauna, weather. The primary on-foot experience. Surface environments are generated from geological, climatic, and cultural parameters that produce coherent landscapes. A desert settlement uses local materials, reflects the culture that built it, and shows evidence of the climate it endures.

**Underground** -- caves, tunnel systems, geological layers. A completely different experience from surface exploration. Underground environments have their own depth curve: shallow caves are accessible and readable; deep systems require specialized equipment, geological knowledge, and material science understanding. Underground gets its own depth test commitment -- hour 500 underground should feel as rich as hour 5, independent of surface depth.

**Bases** -- player-built structures with their own interior environments. Base interiors are defined by what the player has constructed. A base is a reflection of the player's engineering knowledge, material choices, and functional priorities. Base environments evolve as the player expands and modifies them.

#### Constructed/Interior

**Ship Interiors** -- walkable spaces inside larger ships. Crew habitation is functional, not decorative. Every interior space serves a purpose tied to the ship's operation. If a room exists, someone uses it. If nobody uses it, it doesn't exist. Ship interiors reflect the ship's role, the crew's culture, and the engineering tradition that built it.

**Space Stations** -- inhabited structures with their own culture, economy, and population. Stations are not generic waypoints -- they are communities with history, demographics, economic function, and architectural character derived from their founding culture and subsequent development.

**Special Structures** -- derelicts, ruins, anomalies, weird orbitals. Never generic. Every special structure is contextually tied to the planet, race, or culture it belongs to. A derelict tells a story through its construction materials, its engineering tradition, and its state of decay. Ruins reflect the civilization that built them and the forces that ended them. Anomalies are genuinely anomalous -- they don't fit the expected patterns, and that's the point.

#### Space

**Orbital** -- space around planets, moons, and stations. The immediate context of any celestial body. Orbital environments include traffic patterns, debris fields, resource concentrations, and the gravitational landscape that shapes navigation.

**System** -- full star systems with unique composition. Each system has a stellar type, planetary arrangement, resource distribution, and inhabited zones that form a coherent whole. System-level patterns are readable -- a player who understands stellar physics can predict what a system contains before surveying it.

**Inter-System** -- travel corridors and border zones. The spaces between systems, including throughway networks that connect civilization and the boundary regions where jurisdictions overlap, conflict, or create power vacuums.

**The Void** -- deep frontier beyond established throughway networks. Mappable, dangerous, and genuinely unknown. Void space is where the strangeness gradient is steepest -- the furthest from the player's origin, the most alien, the most rewarding for those with the knowledge and equipment to survive it.

### Anti-Pattern: No Copy-Paste Structures

**Mechanism: Cultural templates with parametric variation.** Each civilization has an architectural grammar -- the template. Local parameters (materials available, terrain constraints, climate conditions, settlement age, economic function) produce variation within the grammar. Two Keth trading posts look recognizably Keth but are never identical -- one uses local stone because it's built on a mineral-rich plateau, the other uses imported composites because the local geology is unsuitable. The grammar is consistent; the expression is always local.

This applies to all generated structures across all environment types. Natural environments use geological and ecological grammars with the same principle: coherent rules producing varied expression.

### Content Progression

Content in Apeiron Cipher follows a model of revelation plus activation, never generation-on-demand.

**The world is deterministically derivable from its seed.** The universe is not pre-instantiated in memory, but is deterministically guaranteed to produce the same world from the same seed. Data materializes when relevant -- when the player arrives at a location or engages with a system, the generation algorithms produce the same result they would always produce for that seed and those parameters. Nothing is random at query time. Everything is derivable.

**Knowledge makes the invisible visible.** Content was always there; the player's perception changed. A mineral formation that looked like a rock at hour 5 reveals its geological significance at hour 50 -- not because the game added information, but because the player can now read what was always present.

**Engagement activates dormant systems.** NPC culture exists from generation, but layers surface through relationship. A civilization's internal politics, artistic traditions, and historical grievances are all derivable from the seed -- they become visible as the player's cultural knowledge and linguistic fluency deepen.

**The mirror reveals, it does not create.** Nothing is fabricated for the player's benefit. The mirror principle identifies what's already present in the generated world that aligns with the player's demonstrated interests, and increases the player's opportunity to encounter it. The content was always there. The mirror adjusts attention, not reality.

**Spaces are emergent narratives.** Coherent generation rules produce narrative as an inevitable byproduct, not through authorship. When geological rules, cultural rules, economic rules, and historical rules all apply consistently to the same location, the result tells a story -- not because a writer crafted it, but because consistent systems produce coherent outcomes. A mining settlement on a resource-rich moon with a dying vein tells its own story through its architecture (once prosperous, now deteriorating), its population (dwindling), its economy (desperate), and its geology (nearly exhausted). No narrative designer touched it. The systems converged.

### Level Design Principles

Seven principles govern all environment design:

1. **Coherence over randomness.** Every generated space must feel like it belongs where it is. Materials match geology. Architecture matches culture. Flora matches climate. If anything feels arbitrary, the generation rules need refinement.

2. **Contextual coherence over variety.** A desert planet should feel like a desert planet -- not like a sampler of biomes stitched together for variety. Variety comes from traveling to different places, not from any single place trying to contain everything.

3. **Every space readable at multiple knowledge levels.** A novice sees a cave. An intermediate geologist sees sedimentary layers indicating ancient water. An expert sees the mineral composition that implies a specific formation process and predicts what lies deeper. The same space serves all knowledge levels simultaneously.

4. **Every space is functional -- no decorative dead space.** A room without a function is a generation bug. If a space exists, something happens there -- someone lives there, something is stored there, a process occurs there. This applies to all generated interiors: ships, stations, bases, ruins. Exteriors have ecological function -- terrain serves geological, biological, or atmospheric purposes.

5. **No tutorial spaces.** No environment is designed to teach. All environments are designed to be coherent simulations that reward observation. Learning happens because the player pays attention, not because the space was arranged for pedagogical purposes.

6. **Scale reflects investment.** Large structures represent proportional effort -- from whoever built them. A massive alien megastructure implies the civilization that built it had the resources, knowledge, and motivation to invest at that scale. A player's large construction represents their accumulated material science, engineering knowledge, and economic capacity. Scale is earned, never arbitrary.

7. **Coherence is testable.** Environmental coherence decomposes into verifiable constraint satisfaction: material consistency (structures use locally available or culturally imported materials), cultural consistency (architecture follows the builder's architectural grammar), geological consistency (terrain formations follow physical rules), and functional consistency (every space serves a purpose). These constraints are individually testable during development -- if a generated environment violates a constraint, the generation rules need correction.

---

## Art and Audio Direction

### Art Style

#### Visual Identity: Stylized Realism

Apeiron Cipher targets the NMS zone -- stylized realism where atmosphere and color do the heavy lifting, not polygon density. The world should feel vast, alien, and beautiful without requiring cutting-edge hardware. "Runs on a 5-year-old laptop" is a visual constraint as much as a performance constraint -- and the constraint becomes identity. The game looks the way it does because it committed to accessibility, and that commitment produced a distinct aesthetic.

#### Camera

Switchable first-person and third-person. First-person for immersion, presence, and the embodied experience of being on an alien world. Third-person for spatial awareness, construction, and appreciating the structures the player has built. The player chooses. Neither mode is primary.

#### Star-Driven Aesthetics

Star color physically drives the visual palette of every system it illuminates. This is physics-derived, not art-directed:

- **Red dwarf systems** -- warm, saturated. Landscapes bathed in deep reds and ambers. Vegetation (where it exists) has evolved for red-spectrum photosynthesis.
- **Yellow/white main sequence** -- balanced, familiar. The closest to Earth-like lighting. The player's origin system lives here.
- **Blue giant systems** -- cold, desaturated. Harsh, high-energy light that washes color out. Landscapes feel stark, exposed, restless.

Star spectral type doesn't just tint the world -- it drives biome distribution. A red dwarf's habitable zone is tight, producing worlds that are tidally locked or narrowly temperate. A blue giant's radiation output shapes what can survive on its planets. The visual palette and the ecological palette emerge from the same physics.

**Design constraint:** No manual color grading per system. The star's physical properties feed into a rendering pipeline that produces the palette deterministically. A contributor adding a new stellar type gets the visual palette for free from the physics.

#### Knowledge-Driven Rendering

The game literally renders differently based on the player's knowledge state. This is the visual expression of the Learning First pillar -- the world doesn't just contain more information for knowledgeable players, it *looks different* to them.

A mineral formation renders as a generic rock to a player with no geological knowledge. The same formation renders with visible crystalline structure, color variation indicating composition, and subtle surface patterns to a player who has earned geological understanding. The rendering change is at the shader level -- not a UI overlay, not a tooltip, not a highlight. The world itself deepens visually as the player's knowledge deepens.

This is a core architectural commitment. Every visual system needs a knowledge-aware rendering layer. This includes:

- **Geological formations** -- mineral composition becomes visible through surface rendering
- **Biological entities** -- anatomical detail deepens with biological knowledge
- **Structural analysis** -- material stress, engineering quality, and construction tradition become visible in buildings and ships
- **Linguistic elements** -- alien text shifts from undifferentiated glyphs to recognizable character groups as language knowledge accretes
- **Cultural markers** -- architectural grammar, decorative patterns, and symbolic elements become readable

**Tier 1 implementation (launch):** Discrete knowledge gates -- binary shifts in rendering when knowledge thresholds are crossed. A rock looks like a rock, then looks like a geological formation. Clear visual feedback for knowledge milestones.

**Tier 2 implementation (post-launch):** Continuous rendering deepening that parallels the accretion model. No binary gates -- the world gradually resolves as knowledge accumulates, like eyes adjusting to darkness.

#### The Alien Design Gradient

Alien visual design follows the strangeness gradient -- distance from the player's origin correlates with visual alienness:

- **Near home** -- bipedal or recognizably biological. Verbal communication. Body language readable through human analogy. The player can look at these beings and intuit something about them before understanding anything.
- **Mid-range** -- divergent body plans. Non-verbal communication channels: antennae signaling, ritual dance, bio-luminescent patterns, whale-groaning clams. Visual design communicates that these beings think differently, not just look different. Communication requires learning entirely new sensory channels.
- **Far frontier** -- abstract. Entities where the boundary between organism and environment blurs. Physics-adjacent beings that challenge the player's assumptions about what constitutes life. Visual design that resists easy categorization.

The gradient is continuous, not stepped. A player traveling outward experiences a gradual departure from the familiar. The visual design of each race is a signal about how deep the player is into the strangeness gradient.

#### The Void

The void -- deep space outside established throughway networks -- has its own visual identity: near-total visual absence.

- **Throughways** -- visible navigation markers, stable visual reference points. The "sidewalks" of void travel.
- **Side paths** -- markers fading. Visual reference points become sparse. The player relies increasingly on instruments.
- **Deep exploration** -- no visual guidance. The void is dark. Not cinematic dark with dramatic lighting -- actually dark. The ship's instruments are the only reference. The player's sense of space collapses to what their equipment can show them.
- **Anomalies** -- variations in the void's "skin." Visual distortions that indicate something is different here. Not dramatic -- subtle wrongness that a player learns to recognize. Anomaly contact introduces visual artifacts: kaleidoscope effects, texture where there should be none, geometry that doesn't follow expected rules.

The void's visual absence is functional. It produces the emotional experience of being genuinely far from anything known. When something appears in the void, it matters precisely because the void is empty.

#### Visual References

| Reference | What It Informs |
|---|---|
| **No Man's Sky** | World baseline -- stylized realism, vast landscapes, atmospheric color |
| **Adrian Tchaikovsky's unspace** | The void -- visual absence, wrongness, the ship as the only reference |
| **Tchaikovsky's alien species** | The strangeness gradient -- from recognizable to incomprehensible |
| **EVE Online** | Player-built systems, fleet ambition, political scale (not visual style) |

### Audio and Music

#### Audio Propagation as Physics System

Sound in Apeiron Cipher is medium-dependent. This is a physics system, not an aesthetic choice:

- **Atmospheric worlds** -- sound propagates normally through atmosphere. The richness of the soundscape depends on what's generating sound: wind, water, fauna, geological processes, settlements, weather.
- **Airless worlds** -- suit-internal sounds only. The player hears their own breathing, servo movements, equipment operations. The planet contributes nothing. Silence is real.
- **Contact vibration** -- the one channel that breaks through on airless worlds. Boots on rock transmit vibration through the suit. Hand on a cave wall transmits geological activity. A moon worm moving through the ground transmits through the terrain before the player ever sees it. Contact vibration is a learnable sensory system -- players who pay attention to what they feel through surfaces gain information that players who don't will miss.

**The moon worm principle:** On an airless world, the first sign of a subterranean creature is vibration through the ground. The player who has learned to attend to contact vibration gets advance warning. The player who hasn't gets surprised. This isn't a gimmick -- it's the physics of sound propagation producing gameplay consequences. The same principle applies to geological events, structural stress, and approaching vehicles.

#### The Void Soundscape

The ship is the sound of the void. In the void, there is no external medium to carry sound. The ship becomes the instrument:

| Engagement State | Audio Character |
|---|---|
| **Throughways** | Steady hum. Ship systems nominal. The sound of routine travel -- mechanical, predictable, almost meditative. |
| **Side paths** | Hull stress. Creaks and groans as the ship handles turbulence outside established routes. The sound of the ship working harder. |
| **Deep exploration** | The ship protests. Pressure groans. Something that sounds like footsteps on the hull (it isn't -- it's thermal expansion from void exposure). The ship's sounds become unpredictable, reflecting the unpredictability of the environment. |
| **Anomaly contact** | Audio distortion. The ship's normal sounds warp, skip, layer. Frequencies that shouldn't exist in the hull's acoustic profile. The anomaly doesn't make sound -- it changes how the ship's sounds behave. |

The void's audio design serves the Consequence pillar -- how deep the player goes into the void is directly reflected in how the ship sounds. The player learns to read ship audio as environmental data.

#### Planetary Audio

- **Habitable worlds** -- atmosphere change lands first. Before the player registers individual sounds, the acoustic character of the air changes. Then: animal sounds, wind, water, geological resonance, settlement noise. Each element arrives as the player's proximity and attention make it relevant.
- **Barren worlds** -- suit-only audio. The planet is silent. Contact vibration is the only external channel. This produces a fundamentally different exploration experience -- quieter, more internal, more focused on what the player feels rather than what they hear.

#### Star-Driven Soundscapes

Star spectral type drives ambient audio across three axes -- frequency, density, and complexity:

- **Red dwarf systems** -- warm frequencies, sparse sound density, meditative complexity. The ambient soundscape is slow, low, and spacious.
- **Yellow/white main sequence** -- balanced across all axes. Familiar acoustic character.
- **Blue giant systems** -- cold frequencies, dense sound, chaotic complexity. The ambient soundscape is restless, high-energy, and layered.

This is physics-derived, paralleling the visual system. A contributor adding a new stellar type gets the ambient audio profile from the same physics that drive the visual palette.

#### Knowledge-Shift Audio

Audio deepens with the player's knowledge state, paralleling the knowledge-driven rendering system:

**Tier 1 (launch):** Discrete audio layer unlocks. Binary knowledge gates that add audio channels when thresholds are crossed. A player who gains geological knowledge starts hearing geological resonance in cave systems that was previously inaudible. Clear, satisfying "the world just got richer" moments.

**Tier 2 (post-launch):** Continuous audio deepening. No binary gates -- the soundscape gradually resolves as knowledge accumulates. Subtler distinctions become audible. The continuous model parallels the visual system's Tier 2 evolution.

**Design constraint (Tier 1):** Discrete audio gates prevent sensory mismatch with the visual knowledge layer. If visual rendering shifts at a knowledge threshold, audio shifts at the same threshold. The player's senses stay synchronized.

#### Alien Voices

Procedurally generated per-race sound profiles. Alien voice is the sound of communication before the player understands the words:

- **Near home** -- recognizable vocal patterns. Mouth-like sounds, tonal variation, rhythm that feels speech-like. The player can hear that this is language even before they understand it.
- **Mid-range** -- non-vocal channels. Antenna clicks, bio-luminescent pulse timing, subsonic rumbles, rhythmic body percussion. Sound that clearly carries information but through unfamiliar channels.
- **Far frontier** -- abstract and electromagnetic. Sound that may or may not be intentional communication. Frequencies that challenge the player's assumption about what communication sounds like.

Voice profiles are consistent within a race and derive from the race's biological parameters. The sound IS the race's identity before the player learns anything else about them.

#### Spacer Pirate Radio

Player-generated music through a ByteBeat-style synthesis system. Players compose, broadcast, and their signals become landmarks in the void:

- **Signal fires in the dark.** A broadcast creates a detectable signal in void space. Other players navigating the void can pick up broadcasts before they see anything. Music becomes navigation data.
- **The black market frequency.** Spacer Pirate Radio is how players find the informal economy. Following broadcasts leads to player-run trading posts, unauthorized service providers, and the social fringe of the universe. The game never tells you this. You follow a signal because you hear music in the void, and you find a market.
- **Player identity.** A player's broadcast style becomes recognizable. Regulars in a region of space develop sonic identities. "That's Null's signal -- good prices on rare alloys, two jumps spinward."

This was a major discovery from Party Mode analysis. Spacer Pirate Radio ties directly to the Mirror pillar -- it reflects player creativity back into the world as functional content. It also serves Emergent Limits -- the broadcast system creates player-made infrastructure with no designer involvement.

**Scope note:** Spacer Pirate Radio is referenced here for its experiential and aesthetic quality. Mechanical details -- synthesis interface, broadcast range, signal detection, black market integration -- are deferred to game mechanics and social systems documentation.

#### Music Style

Ambient and atmospheric in the NMS lineage. Music supports mood without directing attention. The soundtrack doesn't tell the player how to feel -- it creates acoustic space for the player's own emotional response to the world.

Dynamic music responds to context -- exploration, tension, discovery, social interaction -- but the transitions are gradual and the music never leads. The player's experience drives the emotional arc; the music follows.

#### Voice and Dialogue

No voice acting. No narrator. No tutorial voice. All alien communication through procedural audio and the language learning system. Human NPCs (if any) communicate through text with personality expressed through word choice, syntax, and cultural idiom -- not vocal performance.

This serves multiple constraints: performance budget, localization simplicity, and the design principle that the game never tells the player anything directly. A narrator would break the fourth wall. Voice acting would impose emotional interpretation. Procedural alien voices carry information without imposing meaning.

#### Audio Performance Budget

All audio systems must operate within the "5-year-old laptop" performance target. Audio propagation physics, knowledge-shift audio layers, procedural voice generation, and environmental soundscapes must all coexist within a constrained CPU/memory budget.

Graceful degradation is required. On lower-end hardware, audio complexity reduces before audio cuts out. Fewer simultaneous layers, simpler propagation models, reduced procedural voice complexity. The experience degrades gracefully -- the world sounds simpler, never silent.

#### Audio Accessibility

Non-audio fallback channels are required for all gameplay-relevant audio:

- **Contact vibration** -- visual overlay and/or haptic feedback alternative for players who can't perceive vibration through audio
- **Void audio states** -- visual ship-stress indicators that parallel the audio degradation states
- **Knowledge-shift audio** -- visual indicators accompany all audio layer unlocks
- **Alien voice** -- text/subtitle systems with visual language indicators for all procedural speech
- **Spacer Pirate Radio** -- visual signal representation for broadcast detection and navigation

No gameplay-critical information should exist exclusively in the audio channel. Every audio system has a visual or haptic fallback.

### Aesthetic Goals

The art and audio direction ties directly to the six pillars:

| Pillar | Visual Expression | Audio Expression |
|---|---|---|
| **Learning First** | Knowledge-driven rendering -- the world looks different as you learn | Knowledge-shift audio -- the world sounds different as you learn |
| **The Mirror** | Star-driven physics produces visual identity without art direction | Spacer Pirate Radio reflects player creativity as functional world content |
| **Consequence** | Void visual absence produces real navigational consequence | Void audio degradation is real feedback about environmental danger |
| **Deeper** | Rendering layers deepen continuously with knowledge | Audio layers deepen continuously with knowledge |
| **Systems** | Star color drives visual palette AND biome distribution from same physics | Star spectral type drives ambient audio from same physics as visuals |
| **Emergent Limits** | Hardware constraints become visual identity ("5-year-old laptop" aesthetic) | Performance budget shapes audio complexity; graceful degradation is design |

The aesthetic is not applied on top of the systems -- it emerges from them. Star physics produce color. Knowledge state produces rendering fidelity. Void physics produce visual absence. The art direction is the systems made visible. The audio direction is the systems made audible.

---

## Technical Specifications

### Performance Requirements

**Design Philosophy: Graceful Deferral, Not Degradation**

If the system can't deliver the experience yet, it waits until it can deliver it correctly. No clipping, no forcing, no half-rendering. The game defers presentation until it can present correctly, rather than degrading quality to meet a frame deadline.

#### Frame Rate & Resolution Targets

| Tier | Target Hardware | Frame Rate | Resolution |
|---|---|---|---|
| **High** | Desktop PC (dedicated GPU) | 60fps | Up to 4K |
| **Medium** | Desktop PC (integrated GPU), laptops | 30-60fps | 1080p |
| **Low (Stretch)** | Chromebook, mobile, web | 30fps target | 720p-1080p |

#### Load Times & Generation Performance

Generation is math (CPU-cheap). Textures are bytes (the real bottleneck). Context-aware texture packaging solves the bottleneck -- systems load texture tiers appropriate to hardware capability and current knowledge state. Knowledge-driven rendering naturally scales: early-game shaders are simpler, which means low-end hardware runs the early game better by design.

Chunk size is a tuning knob -- smaller chunks on constrained hardware reduce per-frame generation cost at the expense of draw call count. This is a per-platform build-time configuration, not a runtime setting.

#### Audio Deferral

If audio can't load in time, the experience delays until the audio is loaded, then cues the associated visual changes. Audio and visual knowledge-shifts are always synchronized -- the game never shows a visual change without its audio counterpart being ready.

#### Deferral Telemetry (Recommended)

Measure actual deferral frequency per platform tier to validate that graceful deferral delivers real accessibility, not just theoretical promise. If deferral events exceed acceptable thresholds on a given tier, that's a signal to revisit performance budgets for that tier.

### Platform-Specific Details

#### PC (Primary Platform)

- **Distribution:** Direct download always available. Steam (~$10), itch.io, Epic as paid convenience options. **No paid-exclusive version ever exists** -- any version on a monetized platform must also be available free via direct download or web.
- **Input:** Keyboard/mouse primary, controller support.
- **Mod Support:** First-class. Full modding API available on all platforms.
- **Cloud Saves:** Steam handles as paid convenience. Player-controlled save storage (Google Drive or similar) as option.

#### Android (Stretch Goal)

- Covers mobile phones and Chromebooks via Play Store.
- Same modding API as PC.
- Same monetization philosophy -- free, option to pay to support developer.

#### Web/WASM (Stretch Goal)

- Covers browser play and Chromebook access.
- **Browser Targets:** Chromium primary, Firefox and Safari as goals.
- **Modding:** Browser plays canonical build. Modded play via self-hosted instances -- run a web server (even a Raspberry Pi) that serves the WASM build plus mod assets as static files. The Pi is a static file server / "modded CDN"; the browser does all compute.
- Web saves may carry subscription cost for infrastructure.

#### Compile Target Strategy

Bevy supports PC, Android, and WASM as compile targets. Platform differences are resolved at build time, not runtime. The open source community handles testing burden across platforms.

#### Multiplayer Architecture

**Peer Model: Server-to-Server.** Every player runs a server instance. Connections are server-to-server peer relationships, not client-to-server. No single privileged host.

**Connection Priority Hierarchy:**

1. **Local network first** -- Bonjour/mDNS discovery, direct LAN connection, zero internet required.
2. **Internet direct** -- STUN-based NAT hole-punching, peer-to-peer, no relay (~90-95% of connections).
3. **Future: TURN relay (deferred)** -- additive layer on the same ICE framework, introduced when community scale justifies the infrastructure cost. Clear explanation and workaround for the ~5-10% edge case until then.

**Capability-Aware Leader Election:**

- Peers advertise compute profiles (CPU, GPU, memory, storage speed).
- Most capable peer assumes leadership -- handles bulk generation work, syncs completed state to lighter peers.
- Leadership migrates dynamically: if leader drops, next most capable assumes; when a more capable peer reconnects, leadership reclaims automatically.
- A phone peered with a gaming PC offloads generation work -- the network becomes a computational resource, not just a social one.
- When alone, the lighter peer handles all work -- slower but correct, per graceful deferral philosophy.

**Quality Standard:** If a player is present, they are present. No ghost states, no flickering. The NMS pattern of players randomly disappearing is the explicit anti-pattern.

**Hosting:** No port forwarding required. Central orchestration handles matchmaking and signaling only -- no game traffic, no world state on central servers.

#### Persistent World State & Ownership

**Base Persistence:** Uploading makes bases, planets, and radio stations globally visible. Multiplayer participation is implicit distribution -- joining distributes content to all peers' caches. Offline player's creations persist in peers' caches. Nothing disappears.

**Permission Model (Asynchronous):**

| Level | Scope | Capability |
|---|---|---|
| **Visit** | Read-only | Explore, observe, interact non-destructively |
| **Modify** | Additive | Add new elements; cannot alter or remove existing |
| **Owner** | Admin | Full control, conflict resolution authority |

Permissions are grantable regardless of online status, synced as world state.

**Conflict Resolution -- Player-Driven, Git-Style:**

When disconnected peers produce conflicting deltas, both versions render simultaneously highlighted in red. The owner resolves manually -- no auto-resolution, no last-write-wins, no silent overwrites.

Conflict is not a separate complex system. It is a flag on an existing delta element. The seed produces canonical world state; player modifications are deltas on that seed. A conflict is a delta with a "conflicting" state that triggers a different render path. Single data element in the existing delta structure. Conflict resolution only applies to shared base modification with write permissions -- it does not apply to multiplayer as a whole.

### Asset Requirements

#### Generation Philosophy

Authored base meshes plus procedural variation. All original content -- no asset store purchases.

#### Quality Tiers

Three discrete quality tiers -- Low (mobile/Chromebook/web), Medium, High -- baked at build time per platform.

#### Asset Categories

| Category | Strategy |
|---|---|
| **3D Models** | Authored base meshes + procedural variation |
| **Textures** | Three quality tiers, context-aware packaging per platform |
| **Shaders** | Knowledge-gated -- unified system, knowledge state as input, lower knowledge = simpler output |
| **Audio** | Primarily procedural -- authored audio limited to core SFX and foundational layers |
| **UI** | Minimal -- ship instruments, glyphs, interaction surfaces. No HUD clutter |

#### Mod Content

Same monetization spirit as the base game -- free access always, payment optional to support creator. Mod assets follow the same pipeline and tier system as base game assets.

### Technical Constraints

- **Deterministic Generation:** All procedural generation must be deterministic from seeds. Same seed, same world, every time.
- **No Central Game Infrastructure:** No game servers hold world state. Central infrastructure is limited to matchmaking/signaling.
- **Knowledge-Driven Rendering is Architectural:** Every visual and audio system must accept knowledge state as an input parameter. This is not a post-processing layer -- it is a core architectural commitment that shapes every rendering pipeline decision.
- **Open Source:** The codebase is open source. Platform testing burden is shared with the community.
- **Licensing: Open Source with Monetization Parity.** The repository license must explicitly permit monetization (selling on Steam, Epic, itch.io, etc.) while requiring that any monetized distribution also make the same version available as a free direct download or web build. No paid-exclusive versions may exist. This applies to both the base game and mod content. The license selection must be validated against this requirement before first public release.

---

## Development Epics

### Epic Overview

| Epic | Name | Scope | Dependencies | Est. Stories |
|------|------|-------|--------------|-------------|
| 4 | Inventory (MVP) | Carry multiple items, retrieve and return | POC (1-3) | 5 |
| 5 | Deterministic Exterior World Gen | Seed-based chunks, persistence deltas, delta-sync validation | POC (1-3) | 7 |
| 6 | Planets | Planet types, biomes, geology, material palettes, basic life | 5 | 7 |
| 7 | Starter Ship | Discoverable wreck, repair through crafting, transportation (not freeform construction) | 12, 13 | 5 |
| 8 | Ship Take Off | Ground to flight, orbital mechanics, Newtonian controls | 7 | 5 |
| 9 | Contiguous Progression | Airlock, EVA, docking, seamless transition | 7, 8 | 5 |
| 10 | Journal Architecture | Extensible knowledge framework, interaction model, diegetic UI framework, knowledge-driven rendering contract | POC (1-3) | 7 |
| 11 | Material Science Depth | Expanded materials, alloys, procedural generation from biome/stellar context | POC (1-3), 10 | 7 |
| 12 | Crafting | Material combination, recipe discovery, quality variation | 11 | 5 |
| 13 | Base Building / Construction | Spatial structures, enclosed spaces, base-ship unification | 12 | 6 |
| 14 | Alien Languages | Procedural language generation, pattern recognition, translation | 10 | 6 |
| 15 | First Contact Through Friction | Cultural encounters, interpretation, consequence-driven learning | 14 | 5 |
| 16 | Cultural Systems | Customs, taboos, diplomacy, inter-cultural variation | 15 | 5 |
| 17 | Adaptive Regional Economy | Regional markets, logistics, trust-weighted information | 11, 12, 16 | 5 |
| 18 | Non-Traditional Propulsion | Briciator, Unspace, Throughways | 8 | 5 |
| 19 | Void-Based Space Travel | Dynamic void, anomaly generation, route stabilization | 18 | 6 |
| 20 | Hazard Cartography | Environmental hazards, ship interactions, diegetic navigation | 11, 18 | 4 |
| 21 | Automation / NPC Managers | Trainable NPCs, delegation, absence management | 16, 17 | 6 |
| 22 | Multiplayer | Server-to-server, delta sync, conflict resolution | 5, 9 | 6 |
| 23 | Modding / Community Tools | Mod pipeline, asset extensibility, workshop | 13 | 5 |
| 24 | Art/Audio Depth | Procedural audio, graceful deferral, mod asset pipeline | 10, 11 | 3 |

### Recommended Sequence

The epic order follows a spiral model: each system is built to a playable layer, then the next system is built, and later rings deepen each system in ways that allow them to connect. The sequence is strictly sequential — solo development, one epic at a time.

1. **Ring 1 — Make Things** (Epics 4, 5, 10, 11, 12, 13): Foundation. Inventory MVP, exterior world with delta-sync validation, knowledge architecture with diegetic UI and rendering contracts, material science depth, crafting, construction.
2. **Ring 2 — Go Places** (Epics 6, 7, 8, 9): Expand the world. Planets with biomes and life, starter ship repair, flight, seamless interior-to-space traversal.
3. **Ring 3 — Meet Someone** (Epics 14, 15, 16, 17): Encounter alien civilizations. Language framework, first contact, cultural depth, regional economy.
4. **Ring 4 — Cross the Void** (Epics 18, 19, 20): Deep space. Propulsion mastery, void navigation, hazard cartography.
5. **Ring 5 — Scale Up** (Epics 21, 22, 23, 24): Systems that multiply everything. Automation, multiplayer, modding, procedural audio depth.

Within each ring, the spiral continues — later rings revisit earlier systems with new depth. New feature areas (like the void travel spec and Spacer Pirate Radio) emerge during development and are folded in as they crystallize.

### Vertical Slice

The first playable milestone beyond the POC is completion of Ring 1: a player who can carry items, explore exterior terrain, discover materials through the journal, understand them through experimentation, craft components, and build a structure. This proves the core accretion model — LEARN through explore, interact, try — in a tangible, self-directed way.

### Cross-Cutting Concerns

Three architectural systems are implemented within every epic rather than as standalone deliverables:

- **Mirror System:** Each system implements behavioral observation hooks. The mirror deepens the world in the direction of player engagement. The pattern is documented during Epic 10, enforced during every subsequent epic.
- **Journal Integration:** Epic 10 establishes the architecture. Every subsequent system implements its own journal entries following that framework.
- **Inventory Depth:** Epic 4 delivers MVP carry. Later rings revisit inventory UX and management as the systems it serves mature.

### Post-1.0

- **First Living Ship** (GitHub Issue #76): Separate biological ship system requiring full attention.
- **Creative Mode:** Simplified systems, full construction freedom, no survival stakes. Separate UX consideration.

*For detailed epic breakdowns including goals, scope, deliverables, and high-level stories, see [epics.md](epics.md). GitHub issues are the authoritative source for epic scope, status, and story details.*

---

## Success Metrics

### Technical Metrics

**Instrumentation Posture: Emit by default, prune by evidence.** If a metric *can* be emitted, it *must* be emitted. Silence requires justification — noise does not. Metrics that prove uninformative are removed only after dataset analysis confirms they carry no signal. Raw signals are emitted with contextual dimensions (biome, planet type, star system, session, player) attached as metadata for post-hoc slicing.

**Collection Architecture:** Telemetry is implemented as a dedicated Bevy plugin with a `TelemetryEvent` resource. Systems write to the centralized collection point; the backend (local file, network, disabled) is swappable without touching gameplay code. A compile-time feature flag controls emission in release builds — all instrumentation stays in source, overhead stays zero when the flag is off. From day one, events are written as structured JSON lines (one event per line) with a consistent schema: timestamp, session ID, event type, dimensional metadata, and value.

**Performance**
- Frame time distribution (p50 / p95 / p99)
- Chunk generation time (with planet type, biome, and seed dimensions)
- Delta persistence write/read latency
- Memory usage by system
- Entity count over time
- System schedule duration per frame
- System ordering bottleneck detection — not just duration, but *why* a system waited (data contention, exclusive system lock, resource conflict)

**Determinism**
- Seed replay validation (same seed must produce identical output — binary pass/fail)
- Determinism drift attribution — system-level checksums so failures are immediately attributable to the diverging system
- Delta integrity checks

**Seamless Transitions**
- Transition stutter duration at boundary crossings
- Asset streaming latency during scale transitions

**Procedural Generation**
- Material generation coherence per seed
- Language generation consistency per seed
- Audio generation latency

**Stability**
- Crash count — **target: zero. Any crash is a priority-zero bug, not a metric to optimize.** This is an invariant, not a KPI.
- Panic / unwrap hits — also zero (enforced by Rust coding rules)

**Graceful Deferral (Epic 24)**
- Deferral frequency per system
- Deferral duration

**Build Health**
- Cargo clippy warnings (target: zero)
- Test coverage
- Build time trend

#### Key Technical KPIs

| Metric | Target | Measurement Method |
|--------|--------|--------------------|
| Frame time p99 | TBD per platform | In-engine telemetry plugin |
| Crash count | 0 (invariant) | Crash reporting / panic hooks |
| Clippy warnings | 0 | CI pipeline (`cargo clippy -- -D warnings`) |
| Seed determinism | 100% replay match | Automated seed replay tests with system-level checksums |
| Chunk gen time | TBD per biome complexity | Per-frame system profiling |

### Gameplay Metrics

**All gameplay metrics are behavioral observations, not scores.** A player who spends 40 hours on one planet's material science is not "behind" a player who has visited twelve star systems. Metrics describe what happens — they do not judge it.

**Knowledge Accretion**
- Journal entries discovered over time (rate curve, not count)
- Material properties catalogued vs. available in current context
- Language fragments decoded per civilization encounter
- Crafting recipes discovered through experimentation vs. available
- Knowledge density — depth in one domain vs. breadth across many

**Core Verb Distribution**
- Time and action share across explore / navigate / interact / try / talk
- Verb transition patterns — what players do *after* each verb
- Which verb a player gravitates toward first in a new environment

**Mirror System Observations**
- Behavioral patterns detected per session
- World-deepening events triggered and in which direction
- Player specialization drift over time (does the mirror converge or stay broad?)
- Mirror event correlation during repeated failure sequences — did the world respond when the player struggled?

**Material & Crafting**
- Material interaction discovery rate (with biome, planet, star system as sliceable dimensions — not filters)
- Experimentation attempts (successes, failures, and repeated failures on the same combination)
- Construction scale progression (small objects to structures to mobile structures)

**World Engagement**
- Unique biomes / planets / star systems visited vs. available
- Revisit frequency — do players return to known locations?
- Time spent per scale tier (surface, orbit, interplanetary, interstellar)
- Boundary crossing frequency

**Social / Cultural**
- Alien encounter frequency and duration
- Language comprehension curve per civilization
- Trade interaction patterns
- Cultural understanding depth progression

**Session Shape**
- Session duration distribution
- Cold start behavior (first 5 minutes)
- Session end triggers (natural stopping point vs. frustration signal)
- Time between sessions

#### Key Gameplay KPIs

| Metric | Target | Measurement Method |
|--------|--------|--------------------|
| Knowledge accretion | Zero KA events in any active session is a P1 investigation trigger | Journal telemetry — session-end summary event |
| Core verb coverage | Try, explore, interact, navigate observed within first 3 sessions (talk excluded until Ring 3) | Action event logging |
| Discovery rate | Sustained curve; prolonged flatlines trigger review | Material/recipe event stream |
| Revisit rate | TBD — presence indicates knowledge-driven rendering is working | Location visit logging |
| Mirror trigger rate | TBD — presence indicates behavioral observation is functioning | Mirror system event hooks |
| Repeated failure correlation | Mirror event fires during sequences of 3+ failures on same combination | Failure count + mirror event join |

### Qualitative Success Criteria

These are assessed through playtesting observation and player feedback — not telemetry. Quantitative metrics can flag where to look; only human observation reveals whether the design is working.

- **"I figured it out"** — Players describe discoveries as their own insight, not the game's tutorial. The accretion model is working when players feel ownership over knowledge.
- **"I went back because I understood more"** — Revisiting a location with new knowledge produces a genuinely different experience. The knowledge-driven rendering contract is fulfilling its promise.
- **"I didn't realize I was learning a language"** — Alien language acquisition feels emergent, not like a skill tree.
- **"My base flew"** — The ship=base+engine unification feels natural, not like a mode switch.
- **"The world noticed what I care about"** — Mirror system deepening feels organic. Players shouldn't be able to articulate the mirror — they should feel *seen*.
- **"I don't know what the game wants me to do" (said positively)** — Absence of directed goals feels like freedom, not confusion.

### Metric Review Cadence

- **Per-commit:** Build health metrics (clippy, tests, determinism) enforced by CI. No exceptions.
- **Weekly:** Batch review of session shape, verb distribution, discovery rate, and failure correlation from playtest sessions. Pair with qualitative observation notes.
- **Per-epic completion:** Full metric review against epic goals. Prune metrics confirmed as noise. Add metrics for newly instrumented systems.
- **Per-ring completion:** Cross-system metric review. Evaluate whether qualitative success criteria are being met across the ring's systems.

---

## Out of Scope

**Post-1.0 Features**
- **First Living Ship** (GitHub Issue #76) — Biological ship system requiring dedicated design and implementation attention. Deferred to post-1.0.
- **Creative Mode** — Simplified systems, full construction freedom, no survival stakes. Separate UX consideration deferred to post-1.0.

**Platforms**
- Console ports (PlayStation, Xbox, Switch)
- Mobile (iOS, Android)
- VR

**Input**
- Controller support

**Audio**
- Traditional recorded audio — no voice acting, no orchestral score. Audio is entirely procedural.

**Localization**
- AI-generated translations — explicitly rejected. Localization to non-English languages is a community-driven effort post-launch.

**Distribution**
- Steam storefront (TBD — not committed for v1.0. Initial distribution is GitHub releases.)

### Deferred to Post-Launch

- First Living Ship (#76)
- Creative Mode
- Console / Mobile / VR platform ports
- Controller support
- Community-driven localization
- Steam (or equivalent storefront) distribution

---

## Assumptions and Dependencies

### Key Assumptions

**Technical**
- Bevy engine remains stable and actively developed through all five rings of implementation
- Rust 2024 edition remains the target toolchain
- Procedural audio generation is viable at the quality level required — no fallback to recorded assets
- Deterministic world generation scales to star-system level without prohibitive compute cost
- Delta persistence (storing changes, not full world state) is sufficient for the save system at scale

**Solo Developer**
- Single developer capacity through all 21 epics across 5 rings
- No external art or audio contractors — all assets are procedural or programmer art
- Alpha release on GitHub; community feedback shapes iteration

**Platform**
- PC only (macOS, Windows, Linux) for v1.0
- Players have keyboard + mouse
- GitHub releases as initial distribution mechanism

**Market**
- Sandbox/exploration genre has sustained audience interest
- "No hand-holding" design philosophy has a viable niche audience
- Alpha release model — playable builds available on GitHub, iterated with community feedback

### External Dependencies

| Dependency | Role | Risk |
|-----------|------|------|
| Bevy engine + ecosystem crates | Game engine, ECS, rendering, audio | Bevy is pre-1.0; breaking changes between versions |
| Rust compiler toolchain | Build system | Low risk — stable release channel |
| GitHub | Source control, issue tracking, releases | Low risk |
| Graphite | Stacked PR / branch management | Low risk — workflow tooling only |

No backend services required for single-player. Multiplayer networking stack (Ring 5, Epic 22) is a future technical decision with no current dependency.

### Risk Factors

- **Bevy stability:** Bevy is pre-1.0. Major version upgrades may require significant migration effort. Mitigated by pinning versions per ring and upgrading between rings.
- **Solo developer throughput:** 21 epics across 5 rings is a multi-year effort for one developer. Mitigated by the spiral model — each ring produces a playable layer, so the game is shippable at progressively richer states.
- **Procedural audio quality:** No fallback to recorded assets means procedural generation must meet the quality bar. Mitigated by Epic 24 (Art/Audio Depth) being in Ring 5 — baseline audio established early, refined last.
- **Multiplayer complexity:** Epic 22 is a Ring 5 commitment with no current networking architecture. Risk of scope explosion. Mitigated by deferring architectural decisions until Rings 1-4 establish the single-player systems.

---

## Document Information

**Document:** Apeiron Cipher - Game Design Document
**Version:** 1.0
**Created:** 2026-03-14
**Author:** NullOperator
**Status:** Complete

### Change Log

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-14 | Initial GDD complete |
