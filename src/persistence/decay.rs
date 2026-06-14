//! Modification decay system with anchor protection.
//!
//! # Design overview
//!
//! The world accumulates player-authored changes (chunk additions and chunk
//! removals) over time.  Left unchecked, these modifications would grow the
//! save file without bound.  The decay system imposes a byte budget and evicts
//! the lowest-priority modifications whenever the budget is exceeded.
//!
//! ## Priority model
//!
//! Every modification carries:
//! - **`weight`** — a [`Weight`] tier.  `Low` mods are decorative / transient;
//!   `High` mods are core-base structures that must never be evicted.
//! - **`created_at`** — a [`Timestamp`] (Unix-epoch seconds).
//! - **`position`** — world-space location, used to compute anchor proximity.
//! - **`anchor_ids`** — explicit association list (informational; spatial
//!   distance to [`AnchorRecord`] entries is what drives the math).
//!
//! Effective age is `raw_age * distance_factor(position, anchors)`.  A
//! modification inside an anchor's radius has a reduced effective age, meaning
//! it is treated as "younger" than its raw age and therefore survives longer.
//!
//! ## Eviction order
//!
//! Candidates are sorted by `(weight asc, effective_age desc, anchor_distance desc)`:
//!
//! 1. Lowest weight first — `Low` before `Medium`; `High` is never a candidate.
//! 2. Oldest effective age first within the same weight tier.
//! 3. Furthest from any anchor as a tiebreaker.
//!
//! ## Anchor rules
//!
//! Anchors (`Weight::High`) are stored in [`WorldState::anchors`] and never
//! appear in the candidate list — they cannot be auto-evicted by this system.
//!
//! ## Integration
//!
//! Call [`decay_tick`] once per persistence tick, passing the current
//! [`WorldState`], the allowed [`ByteBudget`], and the current wall-clock
//! [`Timestamp`].  The function returns immediately when the state is already
//! within budget.

use serde::{Deserialize, Serialize};

// ── Public primitive types ───────────────────────────────────────────────────

/// Opaque identifier for a decay anchor.
///
/// Carried by both [`AnchorRecord`] and [`ModificationRecord::anchor_ids`] so
/// the decay system can trace which anchor is closest to each modification
/// without re-scanning every time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnchorId(pub u64);

/// Unix-epoch seconds at the time a modification was created.
///
/// `u64` is sufficient for ~585 billion years of game time.
pub type Timestamp = u64;

/// Maximum allowable byte footprint for all tracked modifications.
///
/// Passed to [`decay_tick`] to tell the eviction loop when to stop.
pub type ByteBudget = usize;

// ── Weight ───────────────────────────────────────────────────────────────────

/// Priority tier for a world modification.
///
/// Variants are ordered so that `Weight::Low < Weight::Medium < Weight::High`
/// and the derived `Ord` sorts lower-priority (lower-value) variants first —
/// which is exactly the eviction order we want when sorting ascending.
///
/// `Weight::High` modifications (including anchors) are **never** auto-evicted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Weight {
    /// Decorative or transient world changes.  Evicted first.
    Low = 1,
    /// Significant player actions worth preserving under moderate pressure.
    Medium = 2,
    /// Core base structures and player-placed anchors.  Never auto-evicted.
    High = 3,
}

// ── AnchorRecord ─────────────────────────────────────────────────────────────

/// A player-placed anchor that slows decay of nearby modifications.
///
/// Anchors always carry `Weight::High` and live in [`WorldState::anchors`],
/// not in `modifications`.  This structural separation guarantees they are
/// never considered for eviction even under maximum budget pressure.
pub struct AnchorRecord {
    /// Stable unique identifier for this anchor.
    pub id: AnchorId,
    /// World-space position `[x, y, z]` (plain floats to avoid a Bevy dep).
    pub position: [f32; 3],
    /// Radius in world units within which this anchor provides protection.
    /// Modifications closer than this distance receive a reduced effective age.
    pub radius: f32,
}

// ── ModificationKind ─────────────────────────────────────────────────────────

/// What kind of world mutation a [`ModificationRecord`] represents.
///
/// Each variant carries the byte count needed by [`WorldState::estimated_size`].
/// The eviction logic does not care which kind a modification is — it only
/// matters for the caller, which must apply the appropriate undo operation after
/// [`decay_tick`] removes a record.
///
/// - **`ChunkAddition` evicted** → remove the player-placed object from the world.
/// - **`ChunkRemoval` evicted** → restore the generated object (forget the removal).
pub enum ModificationKind {
    /// A player-placed world object.
    ChunkAddition {
        /// Estimated byte size of this record's payload data.
        data_bytes: usize,
    },
    /// A generated world object that the player removed.
    ChunkRemoval {
        /// Estimated byte size of this record's payload data.
        data_bytes: usize,
    },
}

impl ModificationKind {
    /// Returns the estimated byte size for this modification kind.
    fn data_bytes(&self) -> usize {
        match self {
            ModificationKind::ChunkAddition { data_bytes }
            | ModificationKind::ChunkRemoval { data_bytes } => *data_bytes,
        }
    }
}

// ── ModificationRecord ───────────────────────────────────────────────────────

/// A single tracked world modification eligible for decay eviction.
///
/// Records are stored in [`WorldState::modifications`].  Each record is either
/// a chunk addition or chunk removal (see [`ModificationKind`]) and carries the
/// metadata the decay algorithm needs to score it for eviction.
pub struct ModificationRecord {
    /// Session-unique monotonic ID.  Used by [`WorldState::evict`] to remove
    /// the right record.
    pub id: u64,
    /// Kind determines undo semantics and contributes to estimated size.
    pub kind: ModificationKind,
    /// Eviction priority tier.  `High` modifications are never evicted.
    pub weight: Weight,
    /// Wall-clock time (Unix-epoch seconds) when this modification was created.
    pub created_at: Timestamp,
    /// Explicit anchor association list (informational; actual protection is
    /// computed from spatial distance to [`WorldState::anchors`]).
    pub anchor_ids: Vec<AnchorId>,
    /// World-space position `[x, y, z]` of the modification.  Used to compute
    /// anchor proximity and therefore effective age.
    pub position: [f32; 3],
}

impl ModificationRecord {
    /// Returns the estimated byte size of this record's payload.
    fn data_bytes(&self) -> usize {
        self.kind.data_bytes()
    }
}

// ── WorldState ───────────────────────────────────────────────────────────────

/// In-memory world state managed by the decay system.
///
/// Holds both the evictable modification list and the anchor list.  The two
/// collections are separate by design: anchors never enter the eviction
/// candidate pool regardless of budget pressure.
pub struct WorldState {
    /// All currently tracked world modifications.  Sorted by insertion order;
    /// [`decay_tick`] does not rely on the collection being sorted.
    pub modifications: Vec<ModificationRecord>,
    /// Player-placed anchors.  Read-only from the decay system's perspective —
    /// only the anchor placement story mutates this list.
    pub anchors: Vec<AnchorRecord>,
}

impl WorldState {
    /// Creates an empty [`WorldState`].
    pub fn new() -> Self {
        Self {
            modifications: Vec::new(),
            anchors: Vec::new(),
        }
    }

    /// Returns the estimated total byte footprint of all tracked modifications.
    ///
    /// The decay loop uses this to decide when to stop evicting.  The estimate
    /// is the sum of [`ModificationRecord::data_bytes`] across all records.
    pub fn estimated_size(&self) -> usize {
        self.modifications.iter().map(|m| m.data_bytes()).sum()
    }

    /// Removes the modification with the given `id` from `modifications`.
    ///
    /// Called by [`decay_tick`] once per evicted record.  After this returns,
    /// the caller is responsible for applying the undo operation (removing an
    /// addition from the world, or restoring a generated object).
    fn evict(&mut self, id: u64) {
        self.modifications.retain(|m| m.id != id);
    }
}

impl Default for WorldState {
    fn default() -> Self {
        Self::new()
    }
}
// ── Distance helper ──────────────────────────────────────────────────────────

/// Euclidean distance between two world-space positions.
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Computes the distance factor that scales raw age to effective age.
///
/// # Semantics
///
/// A modification sitting at the centre of an anchor's protective radius has a
/// factor of `0.0` (effective age = 0 — never evicted, as if just created).
/// A modification exactly at the edge of the radius has a factor of `0.5` (half
/// the raw age).  A modification beyond all anchor radii has a factor of `1.0`
/// (full raw age — no protection).
///
/// The interpolation is linear across `[0, radius]`:
///
/// ```text
///   factor = 0.5 * (dist / radius)    for dist ≤ radius
///   factor = 1.0                       for dist > radius (no anchor nearby)
/// ```
///
/// When multiple anchors overlap at a position, the smallest (most protective)
/// factor wins.
///
/// # Why 0.5 at the boundary?
///
/// Graduating from full protection at the centre to half-protection at the edge
/// creates a smooth gradient rather than a cliff.  Modifications just inside
/// the radius are meaningfully younger (in effective terms) than those outside
/// it, incentivising players to extend radius coverage to protect work they
/// care about.
fn distance_factor(position: [f32; 3], anchors: &[AnchorRecord]) -> f32 {
    let mut best: f32 = 1.0;
    for anchor in anchors {
        let dist = distance(position, anchor.position);
        if dist <= anchor.radius {
            // Scale to [0.0, 0.5]: 0.0 at centre, 0.5 at edge.
            let factor = 0.5 * (dist / anchor.radius);
            if factor < best {
                best = factor;
            }
        }
    }
    best
}

// ── decay_tick ───────────────────────────────────────────────────────────────

/// Evicts the lowest-priority modifications until the state fits within budget.
///
/// # Parameters
///
/// - `state`  — mutable world state containing modifications and anchors.
/// - `budget` — maximum allowed byte footprint (from [`WorldState::estimated_size`]).
/// - `now`    — current wall-clock time in Unix-epoch seconds.
///
/// # Eviction algorithm
///
/// 1. If `state.estimated_size() <= budget`, returns immediately — no work.
/// 2. Candidates are all modifications with `weight != Weight::High`.
/// 3. Each candidate's **effective age** is:
///    `effective_age = (now - created_at) * distance_factor(position, anchors)`
///    (floored at 0 if `now < created_at` — clock skew guard).
/// 4. Candidates are sorted by `(weight asc, effective_age desc, anchor_distance desc)`.
/// 5. The function evicts in sorted order, stopping as soon as
///    `estimated_size() <= budget`.
///
/// # Anchors
///
/// `Weight::High` modifications and all records in [`WorldState::anchors`] are
/// never evicted.  The caller owns the semantics of "revert" — after
/// [`decay_tick`] removes a record, the caller must apply the corresponding
/// undo operation to the live world (remove a placed object, or restore a
/// generated one).
pub fn decay_tick(state: &mut WorldState, budget: ByteBudget, now: Timestamp) {
    // Fast path: already within budget.
    if state.estimated_size() <= budget {
        return;
    }

    // Snapshot anchor list reference once to avoid repeated borrows.
    let anchors: &[AnchorRecord] = &state.anchors;

    // Build a scored candidate list: (score, modification_id).
    // Score is a tuple that sorts ascending so index 0 is evicted first.
    // We capture everything needed to rank without borrowing `state.modifications`
    // inside the eviction loop.
    struct Candidate {
        id: u64,
        weight: Weight,
        /// Effective age in seconds (float for sort purposes).
        effective_age: f64,
        /// Minimum distance to any anchor (for tiebreaking).
        anchor_distance: f32,
    }

    let mut candidates: Vec<Candidate> = state
        .modifications
        .iter()
        .filter(|m| m.weight != Weight::High)
        .map(|m| {
            let raw_age = now.saturating_sub(m.created_at) as f64;
            let factor = distance_factor(m.position, anchors) as f64;
            let effective_age = raw_age * factor;

            // Minimum distance to any anchor for tiebreaking.
            let anchor_distance = if anchors.is_empty() {
                f32::MAX
            } else {
                anchors
                    .iter()
                    .map(|a| distance(m.position, a.position))
                    .fold(f32::MAX, f32::min)
            };

            Candidate {
                id: m.id,
                weight: m.weight,
                effective_age,
                anchor_distance,
            }
        })
        .collect();

    // Sort: weight asc (low first), then effective_age desc (oldest first),
    // then anchor_distance desc (furthest from anchor first).
    candidates.sort_by(|a, b| {
        a.weight
            .cmp(&b.weight)
            .then_with(|| {
                b.effective_age
                    .partial_cmp(&a.effective_age)
                    .expect("effective_age is finite (saturating_sub from u64)")
            })
            .then_with(|| {
                b.anchor_distance
                    .partial_cmp(&a.anchor_distance)
                    .expect("anchor_distance is finite")
            })
    });

    // Evict in priority order until we are within budget.
    for candidate in &candidates {
        if state.estimated_size() <= budget {
            break;
        }
        state.evict(candidate.id);
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a [`ModificationRecord`] with sensible defaults.  Callers only
    /// override fields that are relevant to each test.
    fn make_mod(
        id: u64,
        weight: Weight,
        created_at: Timestamp,
        position: [f32; 3],
        data_bytes: usize,
    ) -> ModificationRecord {
        ModificationRecord {
            id,
            kind: ModificationKind::ChunkAddition { data_bytes },
            weight,
            created_at,
            anchor_ids: vec![],
            position,
        }
    }

    /// Constructs an [`AnchorRecord`] at the given position.
    fn make_anchor(id: u64, position: [f32; 3], radius: f32) -> AnchorRecord {
        AnchorRecord {
            id: AnchorId(id),
            position,
            radius,
        }
    }

    // ── distance_factor ───────────────────────────────────────────────────────

    #[test]
    fn distance_factor_no_anchors_returns_one() {
        assert_eq!(
            distance_factor([0.0, 0.0, 0.0], &[]),
            1.0,
            "no anchors → full raw age"
        );
    }

    #[test]
    fn distance_factor_at_anchor_centre_returns_zero() {
        let anchor = make_anchor(1, [0.0, 0.0, 0.0], 100.0);
        let f = distance_factor([0.0, 0.0, 0.0], &[anchor]);
        assert!(
            f.abs() < 1e-6,
            "at anchor centre factor should be ~0.0, got {f}"
        );
    }

    #[test]
    fn distance_factor_at_anchor_edge_returns_half() {
        let anchor = make_anchor(1, [0.0, 0.0, 0.0], 10.0);
        // Position exactly at radius along the X axis.
        let f = distance_factor([10.0, 0.0, 0.0], &[anchor]);
        assert!(
            (f - 0.5).abs() < 1e-5,
            "at anchor edge factor should be ~0.5, got {f}"
        );
    }

    #[test]
    fn distance_factor_outside_all_anchors_returns_one() {
        let anchor = make_anchor(1, [0.0, 0.0, 0.0], 5.0);
        let f = distance_factor([100.0, 0.0, 0.0], &[anchor]);
        assert_eq!(f, 1.0, "outside anchor radius → factor 1.0");
    }

    #[test]
    fn distance_factor_best_anchor_wins() {
        // Two anchors; position is inside the smaller one's radius.
        let far_anchor = make_anchor(1, [0.0, 0.0, 0.0], 5.0);
        let near_anchor = make_anchor(2, [8.0, 0.0, 0.0], 10.0);
        let pos = [8.0, 0.0, 0.0]; // exactly at near_anchor's centre
        let f = distance_factor(pos, &[far_anchor, near_anchor]);
        // near_anchor gives 0.0 at its centre — that should win.
        assert!(f.abs() < 1e-6, "closest anchor should dominate, got {f}");
    }

    // ── decay_tick: no-op cases ───────────────────────────────────────────────

    #[test]
    fn decay_tick_noop_when_already_within_budget() {
        let mut state = WorldState::new();
        state
            .modifications
            .push(make_mod(1, Weight::Low, 0, [0.0, 0.0, 0.0], 100));
        decay_tick(&mut state, 200, 1000);
        assert_eq!(state.modifications.len(), 1, "no eviction needed");
    }

    #[test]
    fn decay_tick_empty_state_is_noop() {
        let mut state = WorldState::new();
        decay_tick(&mut state, 0, 1000);
        assert_eq!(state.modifications.len(), 0);
    }

    // ── decay_tick: basic eviction ────────────────────────────────────────────

    #[test]
    fn decay_tick_evicts_oldest_low_weight_first() {
        let now: Timestamp = 10_000;
        let mut state = WorldState::new();
        // Two low-weight mods: id=1 is old, id=2 is recent.
        state
            .modifications
            .push(make_mod(1, Weight::Low, 0, [0.0, 0.0, 0.0], 100)); // age 10_000s
        state
            .modifications
            .push(make_mod(2, Weight::Low, 9_900, [0.0, 0.0, 0.0], 100)); // age 100s
        // Budget allows one mod (100 bytes).
        decay_tick(&mut state, 100, now);
        assert_eq!(
            state.modifications.len(),
            1,
            "should have evicted one record"
        );
        // The surviving mod must be the younger one (id=2).
        assert_eq!(
            state.modifications[0].id, 2,
            "oldest Low mod (id=1) should be evicted first"
        );
    }

    #[test]
    fn decay_tick_prefers_low_over_medium_weight() {
        let now: Timestamp = 1_000;
        let mut state = WorldState::new();
        // Low mod is newer but still lower priority than Medium.
        state
            .modifications
            .push(make_mod(1, Weight::Low, 900, [0.0, 0.0, 0.0], 100));
        state
            .modifications
            .push(make_mod(2, Weight::Medium, 0, [0.0, 0.0, 0.0], 100));
        decay_tick(&mut state, 100, now);
        // Low must be evicted even though Medium is older.
        assert_eq!(
            state.modifications[0].id, 2,
            "Low weight must be evicted before Medium regardless of age"
        );
    }

    #[test]
    fn decay_tick_never_evicts_high_weight() {
        let mut state = WorldState::new();
        state
            .modifications
            .push(make_mod(1, Weight::High, 0, [0.0, 0.0, 0.0], 1_000_000));
        // Budget is zero — force maximum eviction pressure.
        decay_tick(&mut state, 0, 999_999);
        assert_eq!(
            state.modifications.len(),
            1,
            "High weight modification must never be auto-evicted"
        );
    }

    // ── decay_tick: anchor protection ─────────────────────────────────────────

    #[test]
    fn decay_tick_protects_mods_near_anchor() {
        let now: Timestamp = 10_000;
        let mut state = WorldState::new();
        // Anchor at origin with radius 50.
        state.anchors.push(make_anchor(1, [0.0, 0.0, 0.0], 50.0));
        // Protected mod: right at anchor centre (id=1).
        state
            .modifications
            .push(make_mod(1, Weight::Low, 0, [0.0, 0.0, 0.0], 100));
        // Distant unprotected mod (id=2): same age.
        state
            .modifications
            .push(make_mod(2, Weight::Low, 0, [1000.0, 0.0, 0.0], 100));
        // Budget for one mod.
        decay_tick(&mut state, 100, now);
        // id=1 is at anchor centre (effective_age ≈ 0), id=2 is unprotected
        // (effective_age = 10_000).  id=2 should be evicted first.
        assert_eq!(
            state.modifications[0].id, 1,
            "mod at anchor centre should survive eviction (id=1)"
        );
    }

    // ── simulation: 10k modifications, 5 anchors ──────────────────────────────

    /// 24-hour soak simulation.
    ///
    /// Creates 10 000 modifications and 5 anchors.  Modifications near anchors
    /// represent "core base" structures; distant ones are "junk".  We then
    /// call `decay_tick` and verify that:
    ///
    /// 1. The state is within budget after the tick.
    /// 2. All anchor-adjacent `High` modifications survive (anchors never evict).
    /// 3. No modifications with `Weight::High` are ever evicted.
    #[test]
    fn simulation_10k_mods_5_anchors_core_base_persists() {
        let now: Timestamp = 86_400; // 24 hours in seconds.
        let mut state = WorldState::new();

        // Place 5 anchors in a rough pentagon pattern.
        let anchor_positions: [[f32; 3]; 5] = [
            [0.0, 0.0, 0.0],
            [200.0, 0.0, 0.0],
            [100.0, 0.0, 173.0],
            [-100.0, 0.0, 173.0],
            [-200.0, 0.0, 0.0],
        ];
        for (i, pos) in anchor_positions.iter().enumerate() {
            state.anchors.push(make_anchor(i as u64, *pos, 30.0));
        }

        // 500 High-weight "core base" mods clustered at anchor centres.
        for i in 0_u64..500 {
            let anchor_pos = anchor_positions[(i % 5) as usize];
            // Jitter within anchor radius.
            let pos = [
                anchor_pos[0] + (i % 10) as f32 * 2.0,
                0.0,
                anchor_pos[2] + (i / 10 % 10) as f32 * 2.0,
            ];
            state.modifications.push(ModificationRecord {
                id: i,
                kind: ModificationKind::ChunkAddition { data_bytes: 512 },
                weight: Weight::High,
                created_at: 0,
                anchor_ids: vec![AnchorId(i % 5)],
                position: pos,
            });
        }

        // 9 500 Low-weight "junk" mods scattered far away.
        for i in 500_u64..10_000 {
            let x = ((i * 37) % 5_000) as f32 + 500.0;
            let z = ((i * 53) % 5_000) as f32 + 500.0;
            state.modifications.push(ModificationRecord {
                id: i,
                kind: ModificationKind::ChunkAddition { data_bytes: 256 },
                weight: Weight::Low,
                created_at: 0,
                anchor_ids: vec![],
                position: [x, 0.0, z],
            });
        }

        // Total bytes before tick:
        // 500 * 512 + 9500 * 256 = 256_000 + 2_432_000 = 2_688_000
        assert_eq!(state.estimated_size(), 2_688_000);

        // Budget: allow only 320_000 bytes (small enough to force eviction).
        let budget: ByteBudget = 320_000;
        decay_tick(&mut state, budget, now);

        // Post-conditions:
        // 1. Within budget.
        assert!(
            state.estimated_size() <= budget,
            "state should be within budget after tick, got {} bytes",
            state.estimated_size()
        );

        // 2. No High-weight mods were evicted.
        let high_surviving = state
            .modifications
            .iter()
            .filter(|m| m.weight == Weight::High)
            .count();
        assert_eq!(
            high_surviving, 500,
            "all 500 High-weight core-base mods must survive"
        );

        // 3. Distant junk was preferentially evicted: all remaining Low mods have a
        //    smaller count than the original 9 500 and the budget is satisfied by
        //    the combination of surviving High + Low records.
        let surviving_low = state
            .modifications
            .iter()
            .filter(|m| m.weight == Weight::Low)
            .count();
        // Without the decay system all 9 500 Low mods would survive; with it at
        // most (budget - high_bytes) / low_bytes Low mods can remain.
        let high_bytes: usize = 500 * 512;
        let low_bytes_each: usize = 256;
        let max_low = (budget.saturating_sub(high_bytes)) / low_bytes_each;
        assert!(
            surviving_low <= max_low,
            "too many Low mods survived: {surviving_low} > {max_low}"
        );
    }
}
