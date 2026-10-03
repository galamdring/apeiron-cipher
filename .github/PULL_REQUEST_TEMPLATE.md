## Summary

<!-- What changed, and why a player (or the build) should care. -->

## Design tests

Required on every game PR. CI covers confirmation / skip-learning copy (`make design-check`). The rest are human.

- [ ] **Accretion:** After this change, the player understands something they did not before — through the world, not a panel.
- [ ] **Confirmation:** This UI does not tell the player they are correct, legal, or finished. (`Confirmed`, green-legal chrome, empty `???` slots, `(N obs)` counts fail `make design-check`.)
- [ ] **Skip-learning:** The player cannot skip experimenting via a spawn name, recipe string, legality crosshair, or listed unknown stats.
- [ ] **Verb:** This strengthens explore, navigate, interact, try, or talk. If it is only infrastructure, it waits until a verb needs it.
- [ ] **Mirror:** The world responds to player interest, or this change does not pretend to be the Mirror.
- [ ] **Playable in 30 minutes of boot:** The result is visible in-game without a debugger.

If `make design-check` fails: fix the feature. Do **not** add rows to `scripts/design-principles/baseline.txt`. See `docs/bmad/epics/25-design-principle-ci.md`.

## Test plan

- [ ] `make check` (includes `design-check`)
