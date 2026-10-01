# Task 2: Performance Analysis and Optimization

## Code Review Analysis

### Known Performance Observations from Phase 5
From PHASE_5_COMPLETION.md:
- **417 clone instances** identified as potential optimization targets
- No specific bottlenecks documented yet
- Engine architecture appears sound (no obvious algorithmic issues)

### Hot Path Analysis

#### 1. Layer System (layers.rs) - MODERATE IMPACT
**Current behavior**:
- `compute()` called for every characteristic read
- Memoized via `CharacteristicsCache` against `GameState::generation`
- Cache invalidated wholesale on any state mutation
- Every `ordered_effects()` scans all continuous effects

**Key functions**:
```
compute()           ← Recomputed per generation
  ordered_effects() ← Scans all effects, filters, sorts
  apply_one()       ← Applied per effect
```

**Performance characteristics**:
- Linear in number of continuous effects (typically 10-50)
- Filter operation: O(n*m) where n=effects, m=objects
- Timestamp sort: O(n log n)
- No dependency_sort() yet (just a no-op)

**Optimization opportunities**:
- [ ] Selective cache invalidation (only invalidate affected objects)
- [ ] Batch-memoize similar queries
- [ ] Early exit in affects() when set is clearly not matching

---

#### 2. Effect Evaluation (eval.rs) - HIGH IMPACT
**Current behavior**:
- `eval::objects()` evaluates selectors (who does this affect?)
- `eval::value()` evaluates values (what quantity?)
- Called during layer computation AND during rule checking

**Key functions**:
```
objects()  ← Selector evaluation: "all creatures", "enchantments you control", etc.
value()    ← Value evaluation: X costs, "+N/+N", "creatures you control"
```

**Performance characteristics**:
- Selector evaluation involves iteration through objects/permanents
- Value evaluation may trigger recursive evaluation
- Called repeatedly for same effect on same object

**Observations**:
- No obvious algorithmic inefficiency (correct use of iterators)
- Potential: evaluate and memoize selector results per effect/layer
- Potential: value caching within a single effect application

---

#### 3. Trigger Detection (detect.rs) - HIGH IMPACT
**Current behavior**:
- Scans all objects for triggers matching an event pattern
- Called after every action (spell cast, combat, etc.)
- Condition evaluation can be expensive

**Key functions**:
```
triggers()        ← Find all active triggers
state_matches()   ← Does object match trigger conditions?
```

**Performance characteristics**:
- O(n) scan through all permanents + stack objects
- For each: check abilities, evaluate conditions
- Conditions may evaluate complex selectors

**Optimization opportunities**:
- [x] Could index triggers by event type
- [ ] Could pre-filter static triggers that won't change
- [ ] Early exit for creatures with no abilities
- [ ] Batch condition evaluation

---

#### 4. Mana Matching (engine.rs) - MEDIUM IMPACT
**Current behavior**:
- `can_cast()` checks if spell is castable
- `can_tap_for()` checks mana source validity
- These run during choice generation

**Performance characteristics**:
- Called once per playable spell (typically 5-20)
- Mana matching involves color set operations
- Checking target legality can be expensive

**Observations**:
- Color set operations are likely efficient (bitset)
- No obvious algorithmic problem

---

#### 5. State Copying - SIGNIFICANT IMPACT
**Current behavior**:
- `GameState` is frequently cloned for:
  - Undo history
  - Choice validation (trying different actions)
  - Concurrent move generation
- Phase 5 identified 417 clone instances

**Performance characteristics**:
- `GameState` contains: objects (large Map), stacks, triggers, continuous effects
- Each clone deep-copies all nested structures
- Undo buffer can hold 100+ states

**Optimization opportunities**:
- [ ] Cow (Copy-on-Write) wrappers for rarely-modified fields
- [ ] Structural sharing (persistent data structures)
- [ ] Reference-counted interior mutability for immutable subcomponents
- [ ] Consider backing up only deltas instead of full states

---

### Clone Instance Audit (417 instances from Phase 5)

**Distribution by module** (estimated):
- `layers.rs`: static effect cloning (clone() on Vec<ContinuousEffect>) - MODERATE
- `engine.rs`: state cloning for choice validation - HIGH
- `resolve.rs`: effect copying - LOW
- `state.rs`: effect/trigger cloning - HIGH
- Other: scattered use - LOW-MEDIUM

---

## Baseline Measurements

### Test Execution Time
```
Full test suite (399 tests):
  - mtg-app: 173 tests in ~0.39s
  - mtg-engine: ~75 tests in ~6-7s (includes Oracle compilation)
  - Other: ~150 tests in <1s
  Total: ~7-8s
```

**Observation**: Engine tests are slowest; Oracle compilation dominates.

### Compilation Time
```
Release build: 22.89s (full)
Debug build: ~10-15s
```

---

## Top 5 Optimization Opportunities (Prioritized by Impact × Effort)

### 1. **Selective Layer Cache Invalidation** - MEDIUM IMPACT, LOW EFFORT
**Current**: Entire cache cleared on any state mutation
**Proposal**: Invalidate only characteristics affected by the mutation
**Impact**: ~20-40% reduction in recomputation per action
**Effort**: 2-3 hours (need careful design to avoid correctness bugs)
**Risk**: Medium (must track dependencies correctly)

**Implementation**:
- Add inverse mapping: which objects are affected by which effects
- On mutation, only clear cache for affected objects
- Validate with existing tests

---

### 2. **Trigger Early Exit for Ability-less Creatures** - LOW IMPACT, TRIVIAL EFFORT
**Current**: Scans all abilities even for creatures with none
**Proposal**: Skip creatures with no abilities during trigger scan
**Impact**: ~5-10% reduction on trigger detection
**Effort**: <30 minutes
**Risk**: Negligible

**Implementation**:
```rust
// In detect.rs, early return if no abilities
if face.abilities.is_empty() { continue; }
```

---

### 3. **Memoize Selector Evaluation Within Layer** - MEDIUM IMPACT, MEDIUM EFFORT
**Current**: Re-evaluate "all creatures you control" for each effect, each object
**Proposal**: Evaluate selector once per effect per layer, cache results
**Impact**: ~10-30% reduction in layer system per turn (many duplicates)
**Effort**: 3-4 hours
**Risk**: Low (purely memoization)

**Implementation**:
- Add HashMap in ordered_effects()
- Key: (effect_id, selector_ptr); Value: Vec<ObjectId>
- Reuse across the layer computation

---

### 4. **Copy-on-Write for GameState** - HIGH IMPACT, HIGH EFFORT
**Current**: Every undo/choice involves full deep clone
**Proposal**: Use Rc<Cell<T>> or similar for rarely-mutated fields
**Impact**: ~30-50% reduction in choice validation overhead
**Effort**: 6-8 hours (major refactor)
**Risk**: High (affects core data structure)

**Implementation**:
- Identify immutable-during-turns fields (printed cards, player IDs)
- Wrap in Rc/Arc
- Minimize clone depth
- Benchmark before/after

---

### 5. **Trigger Indexing by Event Type** - MEDIUM IMPACT, MEDIUM EFFORT
**Current**: Linear scan through all triggers for each event type
**Proposal**: Pre-index triggers: BTreeMap<EventType, Vec<Trigger>>
**Impact**: ~15-25% reduction on trigger detection
**Effort**: 4-5 hours
**Risk**: Medium (must keep index in sync)

**Implementation**:
- Build index when state changes
- On trigger detection, look up by event type
- Benchmark O(n) scan vs index lookup time

---

## Recommended Quick Wins (< 1 hour total)

1. **Trigger Early Exit** (< 30 min)
   - File: `crates/mtg-engine/src/detect.rs`
   - Change: Add `if face.abilities.is_empty() { continue; }`
   - Risk: None
   - Gain: 5-10% on trigger detection

2. **Code Review for Clone Optimization**
   - Identify top 3 most expensive clones
   - Use `cargo flamegraph` or similar to measure
   - Then target those specifically

---

## Measurement Plan

To get accurate baseline:
```bash
# Build release
cargo build --release

# Run engine tests (slowest component)
cd crates/mtg-engine
time cargo test --release

# Identify hottest test
# Profile that test with:
cargo test --release -- --nocapture test_name
```

**Next Phase**: Run profiler (perf/flamegraph) on a realistic game simulation to see which functions actually dominate runtime.

---

## Summary Table

| Opportunity | Impact | Effort | Risk | Payoff |
|---|---|---|---|---|
| Trigger early exit | Low | Trivial | None | Quick win |
| Selector memoization | Medium | Medium | Low | Good ROI |
| Selective cache invalidation | Medium | Low | Medium | Good ROI |
| Trigger indexing | Medium | Medium | Medium | Good |
| Copy-on-Write GameState | High | High | High | Best long-term |

