#!/usr/bin/env python3
"""Run real production mutations in a disposable source copy.

This is a curated milestone smoke campaign, not a repository mutation score.
Compilation failures and missing tests are errors, never killed mutants.
"""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MUTATIONS = [
    {
        "id": "multiplayer_first_mulligan_costs_a_card",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": ".saturating_sub(free);",
        "after": ".saturating_sub(0); let _ = free;",
        "checks": [["--test", "opening_hands"]],
    },
    {
        "id": "mulligan_redraw_before_other_declarations",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "pg.mulliganing.push(who);",
        "after": "pg.mulliganing.push(who); self.redeal(who);",
        "checks": [["--test", "opening_hands"]],
    },
    {
        "id": "mulligan_bottom_choice_skipped",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "while pg.to_bottom.is_none() && !pg.bottom_pending.is_empty() {",
        "after": "while false && pg.to_bottom.is_none() && !pg.bottom_pending.is_empty() {",
        "checks": [["--test", "opening_hands"]],
    },
    {
        "id": "pod_attack_destinations_only_first_opponent",
        "source": "crates/mtg-engine/src/combat.rs",
        "before": "let mut destinations: Vec<_> = defenders.iter().copied().map(Target::Player).collect();",
        "after": "let mut destinations: Vec<_> = defenders.iter().take(1).copied().map(Target::Player).collect();",
        "checks": [["--test", "multiplayer_combat"]],
    },
    {
        "id": "pod_blocks_allow_foreign_attackers",
        "source": "crates/mtg-engine/src/combat.rs",
        "before": "if state.combat.attackers.contains_key(&attacker)",
        "after": "if false && state.combat.attackers.contains_key(&attacker)",
        "checks": [["--test", "multiplayer_combat"]],
    },
    {
        "id": "pod_only_first_defender_declares_blocks",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "return self.next_pod_blocker(cards, remaining);",
        "after": "return self.next_pod_blocker(cards, { let mut selected: Vec<PlayerId> = remaining; selected.truncate(1); selected });",
        "checks": [["--test", "multiplayer_combat"]],
    },
    {
        "id": "mana_floating_restriction_ignored",
        "source": "crates/mtg-engine/src/mana.rs",
        "before": "pool[slot] = pool[slot].saturating_sub(*amount);",
        "after": "pool[slot] = pool[slot].saturating_sub(0);",
        "checks": [["--test", "mana_restrictions_review"]],
    },
    {
        "id": "mana_restricted_provenance_not_recorded",
        "source": "crates/mtg-engine/src/resolve.rs",
        "before": "if !rc.mana_restrictions.is_empty() {",
        "after": "if false {",
        "checks": [["--test", "mana_restrictions_review"]],
    },
    {
        "id": "mana_restriction_survives_step_end",
        "source": "crates/mtg-engine/src/apply.rs",
        "before": "state.restricted_mana.clear();",
        "after": "// Mutation: omit restricted provenance cleanup.",
        "checks": [["--test", "mana_restrictions_review"]],
    },
    {
        "id": "mana_restriction_missing_from_digest",
        "source": "crates/mtg-engine/src/state.rs",
        "before": "if !self.0.restricted_mana.is_empty() {",
        "after": "if false {",
        "checks": [["--test", "mana_restrictions_review"]],
    },
    {
        "id": "mana_phyrexian_starves_generic_payment",
        "source": "crates/mtg-engine/src/mana.rs",
        "before": "if release > 0 && assigned.take().is_some() {",
        "after": "if false && assigned.take().is_some() {",
        "checks": [["--test", "mana_alternative_cost_review"]],
    },
    {
        "id": "mana_mono_hybrid_cost_truncated_at_eight",
        "source": "crates/mtg-engine/src/mana.rs",
        "before": "        &req.mono_hybrid,\n        &mut trial,",
        "after": "        &req.mono_hybrid[..req.mono_hybrid.len().min(8)],\n        &mut trial,",
        "checks": [["--test", "mana_alternative_cost_review"]],
    },
    {
        "id": "mana_multi_output_alternative_hidden",
        "source": "crates/mtg-engine/src/mana.rs",
        "before": "let multi = group.iter().any(|j| source_units(&sources[*j]) > 1);",
        "after": "let multi = false;",
        "checks": [["--test", "mana_multiple_output_review"]],
    },
    {
        "id": "mana_repeated_choice_splits_colors",
        "source": "crates/mtg-engine/src/mana.rs",
        "before": "if !(correlated || (multi && group.len() > 1)) {",
        "after": "if !(multi && group.len() > 1) {",
        "checks": [["--test", "mana_multiple_output_review"]],
    },
    {
        "id": "mana_shared_tap_source_loses_colorless_alternative",
        "source": "crates/mtg-engine/src/mana.rs",
        "before": "ColorRequirement::Colorless => self.colorless,",
        "after": "ColorRequirement::Colorless => self.colors.is_empty(),",
        "checks": [["--test", "mana_shared_source_review"]],
    },
    {
        "id": "mana_shared_tap_source_double_spent",
        "source": "crates/mtg-engine/src/mana.rs",
        "before": "if group.len() < 2 {",
        "after": "if !group.is_empty() {",
        "checks": [["--test", "mana_shared_source_review"]],
    },
    {
        "id": "mana_shared_tap_source_wrong_ability",
        "source": "crates/mtg-engine/src/mana.rs",
        "before": "if sources[si].taps && !makes(&sources[si]) {",
        "after": "if sources[si].taps && false {",
        "checks": [["--test", "mana_shared_source_review"]],
    },
    {
        "id": "advance_exhaustion_loses_structured_evidence",
        "source": "crates/mtg-verify/src/scenario.rs",
        "before": "Err(error) => return initial_failure(&engine, artifact, &error),",
        "after": "Err(error) => return Err(error),",
        "checks": [["--test", "advance_failure_review", "initial_exhaustion_is_structured_replayable_evidence_without_action_index"]],
    },
    {
        "id": "replay_ignores_scenario_advance_budget",
        "source": "crates/mtg-verify/src/scenario.rs",
        "before": "let mut choice = match next_choice_with_budget(&mut engine, &cards, scenario.advance_budget) {",
        "after": "let mut choice = match next_choice_with_budget(&mut engine, &cards, MAX_ADVANCE_BUDGET) {",
        "checks": [["--test", "advance_failure_review", "initial_exhaustion_is_structured_replayable_evidence_without_action_index"]],
    },
    {
        "id": "campaign_initial_failure_has_phantom_action",
        "source": "crates/mtg-verify/src/campaign.rs",
        "before": "report.first_divergent_action = artifact.actions.len().checked_sub(1);",
        "after": "report.first_divergent_action = Some(0);",
        "checks": [["--test", "advance_failure_review", "semantic_failure_artifact_and_observed_report_match_replay"]],
    },
    {
        "id": "zero_action_campaign_skips_normalization",
        "source": "crates/mtg-verify/src/campaign.rs",
        "before": "let mut pending = match next_choice_with_budget(&mut engine, &cards, artifact.advance_budget) {",
        "after": "let mut pending = match if steps == 0 { Ok(None) } else { next_choice_with_budget(&mut engine, &cards, artifact.advance_budget) } {",
        "checks": [["--test", "campaign_boundaries", "zero_action_campaign_emits_a_replayable_choice_boundary"]],
    },
    {
        "id": "terminal_game_keeps_priority",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "self.state.priority = None;\n            self.phase = Phase::Over;",
        "after": "self.phase = Phase::Over;",
        "checks": [["--test", "campaign_boundaries", "legal_concession_terminal_replay_is_structurally_valid"]],
    },
    {
        "id": "replay_report_trusts_captured_observation",
        "source": "crates/mtg-verify/src/bug_report.rs",
        "before": "if actual != self.observed {",
        "after": "if false {",
        "checks": [["--test", "bug_report_review", "tampering_with_any_material_observation_is_rejected"]],
    },
    {
        "id": "minimizer_substitutes_stale_checkpoint",
        "source": "crates/mtg-verify/src/campaign.rs",
        "before": "signature(candidate, &r) == expected",
        "after": "r.message == expected.message",
        "checks": [["--test", "minimization_review", "last_checkpoint_failure_cannot_be_replaced_by_an_earlier_stale_checkpoint"]],
    },
    {
        "id": "sba_zero_toughness_boundary",
        "source": "crates/mtg-engine/src/sba.rs",
        "before": "if toughness <= 0 {",
        "after": "if toughness < 0 {",
        "checks": [
            ["--lib", "production_sba_matches_independent_table_across_small_state_space"],
            ["--test", "rules", "cr_704_5f_zero_toughness_is_an_owner_graveyard_move_even_if_indestructible"],
            ["--test", "properties", "stable_priority_never_retains_nonpositive_toughness"],
        ],
    },
    {
        "id": "opponent_hand_identity_leak",
        "source": "crates/mtg-engine/src/view.rs",
        "before": "Zone::Hand => obj.owner == viewer,",
        "after": "Zone::Hand => true,",
        "checks": [["--test", "properties", "p0_hidden_hand_and_library_identity_noninterference"]],
    },
    {
        "id": "shared_command_zone_ignores_owner",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "o.owner == who && self.state.commander.is_commander(o.owner, o.card)",
        "after": "self.state.commander.is_commander(o.owner, o.card)",
        "checks": [["--test", "commander", "cr_903_8_shared_command_zone_casting_is_owner_only"]],
    },
    {
        "id": "commander_return_confirmation_inverted",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "if accept {\n                    actions.push(Sba::ReturnCommander { object });",
        "after": "if !accept {\n                    actions.push(Sba::ReturnCommander { object });",
        "checks": [["--test", "commander", "cr_903_9a_owner_can_accept_or_decline_graveyard_and_exile_returns"]],
    },
    {
        "id": "departed_player_keeps_owned_objects",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": ".filter(|o| o.owner == player && !is_ability(o))",
        "after": ".filter(|o| o.owner == player && !is_ability(o) && o.zone.zone == Zone::Hand)",
        "checks": [["--test", "leave_game", "cr_800_4a_owned_objects_in_every_zone_leave_the_game"]],
    },
    {
        "id": "departed_player_control_effects_persist",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "                    .or_else(|| self.state.objects.get(&e.source).map(|s| s.controller))\n                    == Some(player)",
        "after": "                    .or_else(|| self.state.objects.get(&e.source).map(|s| s.controller))\n                    == None",
        "checks": [["--test", "leave_game", "cr_800_4a_control_effects_for_the_departed_player_end"]],
    },
    {
        "id": "departed_player_abilities_exiled_as_cards",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "                } else {\n                    gone.push(*id);",
        "after": "                } else {\n                    exiled.push(*id);",
        "checks": [["--test", "leave_game", "cr_800_4a_departed_players_stack_objects"]],
    },
    {
        "id": "departed_player_remains_in_apnap_order",
        "source": "crates/mtg-engine/src/state.rs",
        "before": ".filter(|p| self.players.get(p).is_some_and(|s| !s.has_lost))",
        "after": ".filter(|p| self.players.contains_key(p))",
        "checks": [
            ["--test", "leave_game", "cr_800_4d_departed_players_triggers_are_not_put_on_the_stack"],
            ["--test", "leave_game", "cr_800_4_each_player_means_players_still_in_the_game"],
            ["--test", "leave_game", "cr_800_4_departed_player_is_no_longer_an_opponent"],
        ],
    },
    {
        "id": "departed_owner_takes_others_stack_abilities",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": ".filter(|o| o.owner == player && !is_ability(o))",
        "after": ".filter(|o| o.owner == player && (is_ability(o) || !is_ability(o)))",
        "checks": [["--test", "leave_game", "cr_800_4a_remaining_players_ability_outlives_its_sources_owner"]],
    },
    {
        "id": "combat_damage_assigned_to_departed_player",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "!matches!(to, Target::Player(p) if self.state.player(*p).has_lost)",
        "after": "!matches!(to, Target::Player(p) if self.state.player(*p).has_lost && false)",
        "checks": [["--test", "leave_game", "cr_800_4e_no_combat_damage_is_assigned_to_a_departed_player"]],
    },
    {
        "id": "mana_exact_generic_boundary",
        "source": "crates/mtg-engine/src/mana.rs",
        "before": "if leftover < req.generic as usize {",
        "after": "if leftover <= req.generic as usize {",
        "checks": [
            ["--test", "mana_properties", "exact_generic_boundary_and_flexible_source_no_double_spend"],
            ["--test", "mana_properties", "production_mana_plan_matches_independent_exhaustive_color_assignments"],
        ],
    },
    {
        "id": "lost_abilities_keep_static_effects",
        "source": "crates/mtg-engine/src/layers.rs",
        "before": "if !silencers.is_empty() {",
        "after": "if silencers.is_empty() {",
        "checks": [
            ["--test", "layers", "cr_613_1_ability_removal_stops_the_lords_anthem_not_a_resolved_pump"],
            ["--test", "layers", "cr_613_cda_removed_in_layer_6_never_applies_in_7a"],
        ],
    },
    {
        "id": "ability_loss_removes_its_own_effect",
        "source": "crates/mtg-engine/src/layers.rs",
        "before": "!(s.source == e.source && s.ability == e.ability)",
        "after": "true",
        "checks": [["--test", "layers", "cr_613_6_global_ability_loss_from_a_creature_still_applies"]],
    },
    {
        "id": "targeting_shroud_ignored",
        "source": "crates/mtg-engine/src/eval.rs",
        "before": "!ctx.has_keyword(id, Keyword::Shroud)?",
        "after": "true",
        "checks": [
            ["--test", "target_properties", "shroud_blocks_both_sides_but_hexproof_allows_its_controller"],
            ["--test", "target_properties", "production_creature_targets_match_independent_scoped_decision_table"],
        ],
    },
    {
        "id": "targeting_phased_out_creature_accepted",
        "source": "crates/mtg-engine/src/eval.rs",
        "before": "!obj.phased_out",
        "after": "true",
        "checks": [
            ["--test", "target_properties", "phased_out_creatures_do_not_exist_for_target_selection"],
            ["--test", "target_properties", "canonical_phased_out_target_regression_preserves_human_asserted_priority"],
            ["--test", "target_properties", "production_creature_targets_match_independent_scoped_decision_table"],
        ],
    },
    {
        "id": "dynamic_effects_read_printed_characteristics",
        "source": "crates/mtg-engine/src/layers.rs",
        "before": "Some(self.ch.clone())",
        "after": "self.printed.characteristics(state, id)",
        "checks": [
            ["--test", "layers", "cr_613_1e_color_change_decides_a_color_anthem"],
            ["--test", "layers", "cr_613_1e_color_change_decides_an_ability_grant"],
        ],
    },
    {
        "id": "spell_copy_keeps_original_owner",
        "source": "crates/mtg-engine/src/apply.rs",
        "before": "obj.owner = *controller;",
        "after": "let _ = controller;",
        "checks": [["--test", "copy_targeting_rules", "cr_707_10_copy_retains_x_mode_targets_but_is_owned_and_controlled_by_copying_player"]],
    },
    {
        "id": "copy_retargeting_accepts_new_illegal_target",
        "source": "crates/mtg-engine/src/engine.rs",
        "before": "&& !check.accepts(&choice.kind, &answer)",
        "after": "&& false && !check.accepts(&choice.kind, &answer)",
        "checks": [["--test", "copy_targeting_rules", "cr_707_10c_new_illegal_target_is_rejected_but_copy_choice_remains_pending"]],
    },
    {
        "id": "spell_copy_inherits_mana_spent",
        "source": "crates/mtg-engine/src/apply.rs",
        "before": "cc.mana_spent.clear();",
        "after": "let _ = &cc.mana_spent;",
        "checks": [["--test", "copy_targeting_rules", "cr_707_10_copy_retains_x_but_does_not_copy_mana_spent"]],
    },
    {
        "id": "protection_prevents_unpreventable_damage",
        "source": "crates/mtg-engine/src/prevention.rs",
        "before": "if state.damage_unpreventable {",
        "after": "if state.damage_unpreventable && !matches!(to, Target::Object(object) if crate::eval::protected_from(state, cards, object, source)) {",
        "checks": [
            ["--test", "protection_rules", "unpreventable_red_damage_bypasses_protection_and_gains_actual_lifelink"],
            ["--test", "protection_rules", "actual_engine_combat_respects_protection_and_unpreventable_damage"],
            ["--test", "prevention_properties"],
            ["--test", "corpus", "every_replay_and_regression_fixture_matches_all_checkpoints"],
        ],
    },
    {
        "id": "unpreventable_damage_skips_shield_counter_removal",
        "source": "crates/mtg-engine/src/prevention.rs",
        "before": "remove_counter(state, object, CounterKind::Shield, events);",
        "after": "let _ = object;",
        "checks": [
            ["--test", "protection_rules", "unpreventable_damage_still_removes_one_shield_counter"],
            ["--test", "prevention_properties"],
            ["--test", "corpus", "every_replay_and_regression_fixture_matches_all_checkpoints"],
        ],
    },
    {
        "id": "unpreventable_damage_skips_phantom_counter_removal",
        "source": "crates/mtg-engine/src/prevention.rs",
        "before": "remove_counter(state, object, CounterKind::PlusOnePlusOne, events);",
        "after": "let _ = object;",
        "checks": [
            ["--test", "protection_rules", "unpreventable_damage_still_removes_phantom_plus_one_counter"],
            ["--test", "prevention_properties"],
            ["--test", "corpus", "every_replay_and_regression_fixture_matches_all_checkpoints"],
        ],
    },
]


def run(copy, env, args, timeout):
    process = subprocess.run(
        ["cargo", "test", "-p", "mtg-verify", "--offline", *args],
        cwd=copy, env=env, text=True, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, timeout=timeout, check=False,
    )
    return {"command": args, "exit_code": process.returncode, "output": process.stdout}


def test_outcome(result):
    match = re.search(r"test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;", result["output"])
    if not match or sum(map(int, match.groups())) == 0:
        return "ERROR"
    if result["exit_code"] == 0 and int(match[2]) == 0:
        return "PASS"
    if result["exit_code"] != 0 and int(match[2]) > 0:
        return "FAIL"
    return "ERROR"



@contextmanager
def isolated_target(base, environment):
    """A fresh namespace prevents baseline reuse of prior or concurrent mutants."""
    base.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="run-", dir=base) as directory:
        yield dict(environment, CARGO_TARGET_DIR=str(Path(directory).resolve()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target" / "verification-mutations", help="Parent directory for a fresh disposable target namespace per campaign")
    parser.add_argument("--timeout", type=int, default=300)
    parser.add_argument("--cases", type=int, default=1000)
    parser.add_argument("--only", action="append", choices=[mutation["id"] for mutation in MUTATIONS], help="Run only named curated mutants; repeat to select several")
    args = parser.parse_args()
    mutations = [mutation for mutation in MUTATIONS if args.only is None or mutation["id"] in args.only]
    if args.cases < 1:
        parser.error("--cases must be positive")
    report = {"format_version": 1, "scope": f"{len(mutations)} selected production mutants; not repository mutation score", "property_cases_per_check": args.cases, "mutations": []}
    env = dict(os.environ, CARGO_TARGET_DIR=str(args.target_dir.resolve()), PROPTEST_CASES=str(args.cases), PROPTEST_RNG_SEED="481", PROPTEST_DISABLE_FAILURE_PERSISTENCE="1")
    try:
        with isolated_target(args.target_dir, env) as env, tempfile.TemporaryDirectory(prefix="mtgo-production-mutations-") as temporary:
            report["target_isolation"] = "unique-per-run"
            copy = Path(temporary)
            for filename in ["Cargo.toml", "Cargo.lock"]:
                shutil.copy2(ROOT / filename, copy / filename)
            shutil.copytree(ROOT / "crates", copy / "crates", ignore=shutil.ignore_patterns("target", "proptest-regressions"))
            digest = hashlib.sha256()
            for source in sorted(copy.rglob("*.rs")):
                digest.update(str(source.relative_to(copy)).encode())
                digest.update(b"\0")
                digest.update(source.read_bytes())
            report["rust_source_sha256"] = digest.hexdigest()
            # copytree keeps source mtimes, and Cargo reuses artifacts in the shared
            # target directory that look newer, which can be the previous run's last
            # mutant. Fresh mtimes force the baseline to be rebuilt from this copy.
            for source in copy.rglob("*"):
                if source.is_file():
                    os.utime(source)
            # Test fixtures referenced outside crate roots must accompany the copy.
            if (ROOT / "tests").exists():
                shutil.copytree(ROOT / "tests", copy / "tests")
            if (ROOT / "fuzz" / "corpus").exists():
                shutil.copytree(ROOT / "fuzz" / "corpus", copy / "fuzz" / "corpus")
            if (ROOT / "verification").exists():
                shutil.copytree(ROOT / "verification", copy / "verification")
            # Validate every check against the unchanged baseline first.
            for mutation in mutations:
                for check in mutation["checks"]:
                    baseline = run(copy, env, check, args.timeout)
                    if test_outcome(baseline) != "PASS":
                        raise RuntimeError("baseline check failed: " + json.dumps(baseline))
            for mutation in mutations:
                source = copy / mutation["source"]
                original = source.read_text()
                if original.count(mutation["before"]) != 1:
                    raise RuntimeError("mutation anchor must appear exactly once: " + mutation["id"])
                result = {"id": mutation["id"], "source": mutation["source"], "before": mutation["before"], "after": mutation["after"], "checks": []}
                try:
                    source.write_text(original.replace(mutation["before"], mutation["after"]))
                    compilation = run(copy, env, ["--lib", "--tests", "--no-run"], args.timeout)
                    if compilation["exit_code"] != 0:
                        result.update(status="ERROR", compilation=compilation)
                    else:
                        for check in mutation["checks"]:
                            observed = run(copy, env, check, args.timeout)
                            observed["status"] = test_outcome(observed)
                            result["checks"].append(observed)
                        statuses = [check["status"] for check in result["checks"]]
                        result["status"] = "ERROR" if "ERROR" in statuses else "KILLED" if "FAIL" in statuses else "SURVIVED"
                finally:
                    source.write_text(original)
                report["mutations"].append(result)
    except (OSError, subprocess.TimeoutExpired, RuntimeError) as error:
        report["error"] = str(error)
    report["killed"] = sum(m["status"] == "KILLED" for m in report["mutations"])
    report["completed"] = len(report["mutations"])
    report["passed"] = "error" not in report and report["completed"] == len(mutations) and all(m["status"] == "KILLED" for m in report["mutations"])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Production mutation smoke: {report['killed']}/{len(mutations)} killed; report: {args.output}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
