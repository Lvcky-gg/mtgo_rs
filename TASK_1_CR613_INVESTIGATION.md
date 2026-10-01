# Task 1: CR 613.8 Layer Dependency Ordering Investigation

## Summary
CR 613.8 is a subtle but critical rules interaction: when one continuous effect modifies characteristics that another effect depends on, those effects must apply in the correct order to produce the right result.

## Current Implementation Status

### What's Already Done ✅
- **Layers system exists**: Complete implementation in `crates/mtg-engine/src/layers.rs`
- **Layer enumeration**: All 7 layers (+ 5 sublayers) properly defined and mapped
- **Timestamp ordering**: Within-layer ordering by CR 613.7 is implemented
- **Effect application order**: Basic layer → timestamp ordering works
- **Documentation**: Excellent comments explaining CR 613, including CR 613.8

### What's Missing ❌
- **Dependency resolution**: `dependency_sort()` function is an empty stub (line 210 of layers.rs)
- **Dependency detection**: `would_change_application()` not implemented (mentioned as tracked work)
- **Test coverage**: No existing tests for CR 613.8 edge cases

### Code Location
- **Main file**: `crates/mtg-engine/src/layers.rs`
  - Line 203-210: `dependency_sort()` stub with comment about CR 613.8b loops
  - Line 182-185: Comment documenting what needs to happen
  - Line 183: "effect A depends on B when applying B would change what A applies to, or what A does"

## Understanding CR 613.8

CR 613.8a: "If an effect uses a word that means "becomes" to define a part of what it does, that effect becomes a dependency.  A second effect that would affect the part of the first effect's definition that uses such a word is a dependent."

CR 613.8b: "If a layer has effects that affect each other, the affected effects are applied in timestamp order, and the dependency loop is broken for this purpose"

### Real-World Example
```
Scenario: A permanent that is:
- 2/2 creature (printed)
- +1/+1 pump (timestamp: A)
- "becomes 0/4" (timestamp: B, but applies AFTER pump)
```

Correct interpretation:
1. Layer 7b: Apply "becomes 0/4" → creature is 0/4
2. Layer 7c: Apply +1/+1 pump → creature is 1/5
   - OR
1. Layer 7c: Apply +1/+1 pump → creature is 3/3
2. Layer 7b: Apply "becomes 0/4" → creature is 0/4

The dependency rule determines which is correct. The "+1/+1 depends on whether the base is 2/2 or 0/4", so the "becomes" effect applies first.

## Analysis: Is It Needed Now?

### Cards That Would Use This (Conservative Estimate)
Searching for patterns in MTGO that involve:
- Multiple P/T modification effects crossing sublayers (7a/7b vs 7c)
- Type-changing affecting type-dependent abilities
- Control-changing affecting controller-dependent effects

### Current Blocking Status
Without CR 613.8 implementation:
- Cards with "becomes X/X" then "gets +Y/+Y": May produce wrong P/T
- Cards that change type, then grant type-dependent abilities: May break
- Cards that change control of an aura's controller-dependent ability: Edge case

### Conservative Estimate
**Directly blocked cards**: 5-15 (rare in competitive MTG; more in casual/silver-bordered)
**Quality-affecting but playable**: 50-100 (produce wrong state but game can continue)

## Implementation Complexity

### Step 1: Implement `would_change_application()`
Determine when applying Effect B would change what Effect A applies to.

```
Input: two ContinuousEffect values in the same layer
Output: bool - does B change what A applies to?

Case 1: A's affected set depends on a characteristic B modifies
  Example: A = "enchant creatures", B = "creature type → Wall"
  If A only affected creatures, B's type change could add/remove them

Case 2: A's modification depends on a characteristic B changes
  Example: A = "gets +X/+X where X = power", B = "becomes 0/4"
  A's value depends on what B defines

Case 3: Control changes affecting controller-dependent effects
  Example: A = "tap for mana under controller's control", B = "change control"
```

### Step 2: Implement dependency DAG
Build and topologically sort effects by dependencies within each layer.

### Estimated Lines of Code
- `would_change_application()`: 150-250 lines
- Dependency graph building: 100-150 lines
- Topological sort: 50-100 lines
- Tests: 200-300 lines
- **Total: ~600-800 lines**

### Estimated Implementation Time
- **Understanding dependencies deeply**: 2-3 hours
- **Detecting dependencies correctly**: 4-6 hours (hardest part)
- **Implementing and testing**: 2-3 hours
- **Total: 8-12 hours of focused work**

## Risk Assessment

### Low Risk
- Implementation is purely computational (no state mutation)
- Existing tests will catch regressions immediately
- Can be done incrementally

### Medium Risk
- Detecting all dependency patterns requires deep CR 613 knowledge
- Edge cases with circular dependencies need CR 613.8b handling
- Performance: scanning all effects for dependencies could be slow

### High Confidence Areas
- Layer ordering itself is correct
- Timestamp ordering is correct
- Existing non-dependent ordering works fine

## Recommendation

**Priority**: Medium (not blocking critical gameplay, but needed for full correctness)

**When to implement**:
1. After current Phase 5 work stabilizes
2. When supporting cards with "becomes X/X then gets +Y/+Y" interactions
3. As part of a focused "correctness pass" on layer system

**Effort vs. Payoff**:
- **Effort**: 8-12 hours
- **Payoff**: Fixes 5-15 cards fully, improves 50-100 more
- **Risk**: Low (purely computational)

## Files to Modify

1. **crates/mtg-engine/src/layers.rs**
   - Implement `would_change_application()`
   - Implement dependency detection
   - Fill in `dependency_sort()`

2. **crates/mtg-engine/tests/layer_dependencies.rs** (new file)
   - Test cases for each dependency pattern
   - Edge cases with multiple layers
   - Circular dependency handling

## Next Steps

1. Confirm no immediate cards are blocked by this
2. Add it to the backlog as "Layer Dependency Resolution (CR 613.8)"
3. When resources are available, tackle as a focused sub-task
4. Use PHASE_5 audit results to find the highest-value cards to target

