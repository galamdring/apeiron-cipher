---
stepsCompleted: [1, 2, 3, 4, 5, 6, 7]
inputDocuments:
  - 'docs/bmad/game-brief.md'
  - 'docs/bmad/brainstorming/brainstorming-session-2026-03-13-1600.md'
documentCounts:
  briefs: 1
  research: 0
  brainstorming: 1
  projectDocs: 0
workflowType: 'gdd'
lastStep: 7
project_name: 'opensky'
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

{{player_progression}}

### Difficulty Curve

{{difficulty_curve}}

### Economy and Resources

{{economy_resources}}

---

## Level Design Framework

### Level Types

{{level_types}}

### Level Progression

{{level_progression}}

---

## Art and Audio Direction

### Art Style

{{art_style}}

### Audio and Music

{{audio_music}}

---

## Technical Specifications

### Performance Requirements

{{performance_requirements}}

### Platform-Specific Details

{{platform_details}}

### Asset Requirements

{{asset_requirements}}

---

## Development Epics

### Epic Structure

{{epics}}

---

## Success Metrics

### Technical Metrics

{{technical_metrics}}

### Gameplay Metrics

{{gameplay_metrics}}

---

## Out of Scope

{{out_of_scope}}

---

## Assumptions and Dependencies

{{assumptions_and_dependencies}}
