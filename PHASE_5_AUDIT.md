# Phase 5: Rules Engine Expansion - Audit Report

Generated: 2026-10-01

## Executive Summary

- **Current Coverage**: 13,706 of 34,898 cards (39.3%)
- **AbilityKind::Native Instances**: 0 (all existing abilities are compiled from grammar)
- **Major Unimplemented Areas**: 10+ high-impact keyword abilities + multi-target grammar patterns
- **Claude's Work Status**: DONE, UNCOMMITTED (~20 ability implementations + test extensions)
- **Shared Changes Needed**: 3 files (format.rs, bot.rs, mtg-store)

---

## Task 1: Rules Coverage Audit

### 1.1 AbilityKind::Native Instances

**Finding**: 0 instances found.

All abilities are currently compiled from Oracle text grammar patterns. The `AbilityKind::Native` variant exists but is unused in the codebase, indicating that the architecture successfully compiles abilities rather than hand-coding them.

**Implication**: New abilities must be added through:
1. Oracle grammar compilation (preferred)
2. Direct `Ability` construction (if grammar is insufficient)
3. NOT through `AbilityKind::Native` markers

### 1.2 Unhandled Effect Types in resolve.rs

**File**: `crates/mtg-engine/src/resolve.rs` (3,421 lines)

**Finding**: 1 location with `panic!("treated as free is worse than an unimplemented one")`

This is a **deliberate design choice** in cost estimation, not a true unhandled case. All Effect types have defined behavior paths in the main match statement (line 187).

**Coverage**: Resolve.rs appears complete for existing effect types. New effects added by Claude (prevention shields, granted abilities, copy mechanics) are already integrated.

### 1.3 Grammar Patterns Not Yet Parsed

**Coverage**: 13,706 / 34,898 = 39.3%

**Top 10 Blocking Patterns** (most common across all cards):

1. **Saga without chapters** (74 cards) — Multi-line saga parsing
2. **Fuse** (44 cards) — Split spell mode selection
3. **{M}{M} — N/N cost modes** (44 cards) — Flexible cost modes
4. **"enters prepared"** (38 cards) — New card state mechanic
5. **Station** (36 cards) — Attraction interaction pattern
6. **Ascend** (28 cards) — City's Blessing conditional
7. **Soulbond** (24 cards) — Pairing mechanic
8. **Exploit** (22 cards) — Sacrifice trigger pattern
9. **Bargain** (22 cards) — Alternative cost syntax
10. **"Starting intensity N"** (20 cards) — Dungeon intensity tracking

**Top 10 Blocking Individual Cards** (single-card obstacles):

1. **Dredge N** (12 cards cannot play without it)
2. **Conspire** (11 cards)
3. **Learn.** (11 cards)
4. **Assist** (10 cards)
5. **Banding** (10 cards)
6. **Cipher** (10 cards)
7. **Splice onto Arcane** (10 cards)
8. **"block additional creature"** (10 cards)
9. **Cost reduction conditionals** (10 cards)
10. **Mana production mode selection** (10 cards)

---

## Task 2: Test Infrastructure Status

### 2.1 Existing Engine Tests

**Location**: `crates/mtg-engine/tests/`

**Current Coverage** (20 test files):
- casting.rs (16.5 KB) — spell casting, costs
- combat.rs (19.5 KB) — combat mechanics
- choices.rs (13 KB) — game choices, modes
- countering.rs (12.6 KB) — counter/bounce mechanics
- rules.rs (13.9 KB) — core rules
- trigger_ordering.rs (20.5 KB) — trigger resolution
- mana.rs (11.9 KB) — mana production
- cleanup.rs (2.7 KB) — cleanup phase
- And 12 more...

**Status**: Comprehensive, but does NOT include:
- Tests for granted abilities (Claude's work)
- Tests for prevention shields (Claude's work)
- Tests for copy effects (Claude's work)
- Tests for multi-target abilities

### 2.2 Existing Oracle Compiled Tests

**Location**: `crates/mtg-oracle/tests/compiled/`

**Current Coverage** (22 test files):
- main.rs (105 KB) — primary keyword test suite
- keywords.rs (94 KB) — keyword ability tests
- combat.rs (15 KB) — combat keywords
- granted.rs (19.4 KB) — granted ability tests (Claude extended)
- prevention.rs (17.4 KB) — prevention mechanics (Claude extended)
- copies.rs (11.2 KB) — copy effect tests (Claude extended)
- damage.rs (6.4 KB) — damage prevention tests
- And 16 more...

**Status**: Comprehensive. Claude added new tests to `granted.rs` (uncommitted).

### 2.3 Test Infrastructure for Claude's Work

**Files Requiring Test Extensions** (NOT YET IN PLACE):

1. **crates/mtg-engine/tests/abilities_extension.rs** — MISSING
   - Tests for granted activated abilities
   - Tests for granted mana abilities
   - Tests for granted triggered abilities
   - Tests for prevention shields
   - Tests for copy effects (clones, token copies)

2. **crates/mtg-oracle/tests/compiled/abilities_extended.rs** — MISSING
   - Tests for morph/megamorph compilation
   - Tests for cascade compilation
   - Tests for madness compilation
   - Tests for multi-target nouns

**Note**: Claude's tests in `granted.rs` are uncommitted (new test case added at end of file).

---

## Task 3: Shared Changes Audit

### 3.1 mtg-app/src/format.rs

**Required Change**: One line (new `Characteristics` field)

**Location**: Line 343-359 (chars() test helper)

**Current State**: 
```rust
fn chars(name: &str) -> Characteristics {
    Characteristics {
        name: name.into(),
        mana_cost: ManaCost::FREE,
        colors: mtg_core::ColorSet::COLORLESS,
        card_types: vec![CardType::Creature],
        subtypes: Vec::new(),
        supertypes: Vec::new(),
        abilities: Vec::new(),
        granted_keywords: Vec::new(),
        every_creature_type: false,
        granted_abilities: Vec::new(),
        power: Some(2),
        toughness: Some(3),
        loyalty: None,
        defense: None,
    }
}
```

**Analysis**: The Characteristics struct already includes:
- `every_creature_type: bool` (changeling flag)
- `granted_keywords: Vec<Keyword>` (granted keywords)
- `granted_abilities: Vec<GrantedAbility>` (granted non-keyword abilities)

**Status**: ✅ ALL REQUIRED FIELDS ALREADY PRESENT

No changes needed to format.rs. Characteristics struct is complete.

### 3.2 mtg-policy/src/bot.rs

**Required Change**: Two lines (bot awareness)

**Location**: Line 24 (imports)

**Current State**:
```rust
use mtg_core::{AbilityId, CardId, CardType, CounterKind, ObjectId, PlayerId, Step, Target};
use crate::{
    actions::Action,
    ...
};
```

**Issues Found**:
- `PlayerId` — UNUSED (line 24)
- `Step` — UNUSED (line 24)
- `actions::Action` — UNUSED (line 27)

**Analysis**: These imports are flagged by `cargo build` as warnings. They may be:
1. Leftover from previous code (should be removed)
2. Intentional for future use (should be kept with `#[allow(unused)]` comment)

**Status**: ⚠️ NEEDS VERIFICATION

The imports don't break anything, but should be cleaned up. Need to check if they're needed for Claude's bot enhancements.

### 3.3 mtg-store Changes

**Required Change**: Intern every creature type on store open (token-only types like Scion)

**Location**: `crates/mtg-store/src/cards.rs`

**Function Available**: `pub fn intern_subtype(&self, name: &str) -> Result<u16>` (line 437)

**Status**: ✅ INFRASTRUCTURE READY

The `intern_subtype` function exists and is ready to use. The store needs to call this for all creature types during initialization/open.

**Action Needed**: Audit where store opens and ensure all creature types are pre-interned.

---

## Task 4: Claude's Uncommitted Work Summary

### 4.1 Completed Implementations (Not Yet Committed)

**mtg-core** (~30 kb changes):
- Changeling support (via `Characteristics::every_creature_type`)
- Prevention shield mechanics
- Granted abilities tracking
- Copy effect support

**mtg-ir** (~20 kb changes):
- Ability Kind definitions for granted abilities
- Effect types for prevention
- Copy effect markers

**mtg-engine** (~50 kb changes):
- `prevention.rs` — prevention shield resolution
- `abilities.rs` — granted ability handling (IDs from `AbilityId::GRANTED_BASE`)
- Copy effect resolution (`GameObject::original`, `Event::BecameCopy`)
- Storm, echo, buyback, unearth, graveyard abilities
- Affinity, modular, evolve, fading, cumulative upkeep
- Bloodthirst, fabricate, living weapon

**mtg-oracle** (~40 kb changes):
- Compiler support for changeling
- Tests in `granted.rs` (new test case at end)
- Tests in `combat.rs` (extended for prevention)

### 4.2 Next Priorities (From Claude's Plan)

In order:
1. Morph/megamorph (requires `Action::CastFaceDown` + one line each in format.rs and board.rs)
2. Cascade, rebound, madness (casting during resolution)
3. Exile mechanics ("exile … until ~ leaves the battlefield")
4. Multi-target noun parsing
5. Saga chapter parsing
6. Grammar pattern improvements

---

## Coordination Checkpoints

### Conflict Analysis

**Files Where Both Teams Might Work**:
1. `mtg-app/src/format.rs` — Characteristics struct (already complete, no conflict)
2. `mtg-app/src/board.rs` — May need changes for morph display
3. `mtg-engine/src/lib.rs` — May need new module exports
4. `mtg-oracle/src/compile/mod.rs` — Grammar pattern coordination

**Safe Areas for Parallel Work**:
1. New test files (no conflicts)
2. New ability implementations (different modules)
3. New oracle grammar patterns (different match arms)

### Recommended Coordination Strategy

1. ✅ Claude commits existing work (changeling, prevention, granted, copies) before Phase 5 begins
2. ✅ Codex creates test infrastructure in new files (no conflicts)
3. ✅ Codex audits shared changes (format.rs, bot.rs, mtg-store)
4. ⚠️ Discuss morph/megamorph implementation (touches format.rs and board.rs)
5. ✅ Coordinate oracle grammar patterns (different keywords can be independent)

---

## Next Steps (Codex Phase 5)

### Priority 1: Test Infrastructure
- [ ] Create `crates/mtg-engine/tests/abilities_extension.rs`
  - Structure tests for granted abilities (no implementation yet)
  - Structure tests for prevention shields
  - Structure tests for copy effects
- [ ] Create `crates/mtg-oracle/tests/compiled/abilities_extended.rs`
  - Structure tests for morph compilation
  - Structure tests for cascade compilation
  - Structure tests for madness compilation

### Priority 2: Shared Changes Audit
- [ ] Clean up unused imports in `mtg-policy/src/bot.rs`
- [ ] Verify `mtg-app/src/format.rs` needs no changes (already complete)
- [ ] Audit `mtg-store` for creature type interning strategy

### Priority 3: Rules Coverage Analysis
- [ ] Prioritize next 10 keyword abilities based on impact
- [ ] Plan grammar pattern implementations
- [ ] Identify multi-target noun patterns

### Priority 4: Coordinate with Claude
- [ ] Review morph/megamorph implementation plan
- [ ] Discuss oracle grammar pattern strategy
- [ ] Merge commitment points

---

## File Locations Reference

```
crates/
├── mtg-core/src/characteristics.rs           (Characteristics struct)
├── mtg-ir/src/
│   ├── ability.rs                           (AbilityKind enum)
│   ├── effect.rs                            (Effect types)
│   └── ...
├── mtg-engine/src/
│   ├── resolve.rs                           (Effect resolution)
│   ├── prevention.rs                        (Claude: prevention shields)
│   ├── abilities.rs                         (Claude: granted abilities)
│   └── tests/                               (Engine tests)
├── mtg-oracle/src/
│   ├── compile/
│   │   ├── mod.rs                          (Entry point)
│   │   ├── clauses.rs                      (Grammar clauses)
│   │   ├── conditions.rs                   (Condition parsing)
│   │   ├── nouns.rs                        (Noun parsing)
│   │   └── words.rs                        (Word patterns)
│   └── tests/compiled/                     (Oracle tests)
├── mtg-app/src/
│   └── format.rs                           (Formatting, includes Characteristics usage)
├── mtg-policy/src/
│   └── bot.rs                              (Bot AI, unused imports warning)
└── mtg-store/src/
    └── cards.rs                            (Store, intern_subtype function)
```

