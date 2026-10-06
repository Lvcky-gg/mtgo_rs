//! Player declarations for pregame discussion; no numerical rating is inferred from card text.
use egui::{RichText, Ui};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Competitiveness {
    #[default]
    Undeclared,
    Casual,
    Focused,
    HighPower,
    Tournament,
}
impl Competitiveness {
    const ALL: [Self; 5] = [
        Self::Undeclared,
        Self::Casual,
        Self::Focused,
        Self::HighPower,
        Self::Tournament,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Undeclared => "Undeclared",
            Self::Casual => "Casual",
            Self::Focused => "Focused",
            Self::HighPower => "High power",
            Self::Tournament => "Tournament",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disclosure {
    #[default]
    Undeclared,
    Included,
    Excluded,
}
impl Disclosure {
    const ALL: [Self; 3] = [Self::Undeclared, Self::Included, Self::Excluded];
    fn label(self) -> &'static str {
        match self {
            Self::Undeclared => "Undeclared",
            Self::Included => "Included",
            Self::Excluded => "Excluded",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeckProfile {
    pub power_level: Option<u8>,
    pub commander_bracket: Option<u8>,
    pub competitiveness: Competitiveness,
    pub typical_win_turn: Option<u8>,
    pub infinite_combos: Disclosure,
    pub extra_turns: Disclosure,
    pub mass_land_denial: Disclosure,
    pub fast_mana: Disclosure,
    pub rule_zero: String,
}

fn bracket_label(bracket: u8) -> &'static str {
    match bracket {
        1 => "1 · Exhibition",
        2 => "2 · Core",
        3 => "3 · Upgraded",
        4 => "4 · Optimized",
        5 => "5 · cEDH",
        _ => "Undeclared",
    }
}

impl DeckProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.power_level.is_some_and(|n| !(1..=10).contains(&n)) {
            return Err("Self-rated power must be 1–10.".into());
        }
        if self
            .commander_bracket
            .is_some_and(|n| !(1..=5).contains(&n))
        {
            return Err("Commander bracket must be 1–5.".into());
        }
        if self
            .typical_win_turn
            .is_some_and(|n| !(1..=20).contains(&n))
        {
            return Err("Typical win turn must be 1–20.".into());
        }
        if self.rule_zero.chars().count() > 4000 {
            return Err("Rule 0 notes are limited to 4000 characters.".into());
        }
        Ok(())
    }

    pub fn from_json(input: &str) -> Result<Self, String> {
        let profile: Self =
            serde_json::from_str(input).map_err(|e| format!("Deck profile: {e}"))?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn summary(&self, commander: bool) -> String {
        let power = self
            .power_level
            .map_or_else(|| "unrated".into(), |n| format!("{n}/10"));
        let mut summary = format!(
            "Self-rated power {power} · {} intent",
            self.competitiveness.label()
        );
        if commander {
            summary.push_str(&format!(
                " · Bracket {}",
                self.commander_bracket.map_or("undeclared", bracket_label)
            ));
        }
        summary
    }

    pub fn pregame_summary(&self, name: &str, commander: bool) -> String {
        format!(
            "{name}\n{}\nTypical winning turn: {} (declared)\nInfinite combos: {}\nExtra turns: {}\nMass land denial: {}\nFast mana: {}\nRule 0: {}\nThese are player declarations; confirm expectations with the table.",
            self.summary(commander),
            self.typical_win_turn
                .map_or_else(|| "undeclared".into(), |n| n.to_string()),
            self.infinite_combos.label(),
            self.extra_turns.label(),
            self.mass_land_denial.label(),
            self.fast_mana.label(),
            if self.rule_zero.trim().is_empty() {
                "No notes provided"
            } else {
                self.rule_zero.trim()
            }
        )
    }

    pub fn edit(&mut self, ui: &mut Ui, commander: bool, name: &str) {
        ui.label(RichText::new("Player declarations for pregame discussion. Power and bracket are not automatic ratings.").small());
        egui::Grid::new("deck-profile-fields").show(ui, |ui| {
            ui.label("Self-rated power");
            optional_number(ui, "deck-power", &mut self.power_level, 10, "Unrated");
            ui.end_row();
            if commander {
                ui.label("Declared bracket");
                egui::ComboBox::from_id_salt("deck-bracket")
                    .selected_text(self.commander_bracket.map_or("Undeclared", bracket_label))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.commander_bracket, None, "Undeclared");
                        for n in 1..=5 {
                            ui.selectable_value(
                                &mut self.commander_bracket,
                                Some(n),
                                bracket_label(n),
                            );
                        }
                    });
                ui.end_row();
            }
            ui.label("Competitiveness");
            egui::ComboBox::from_id_salt("deck-competitiveness")
                .selected_text(self.competitiveness.label())
                .show_ui(ui, |ui| {
                    for value in Competitiveness::ALL {
                        ui.selectable_value(&mut self.competitiveness, value, value.label());
                    }
                });
            ui.end_row();
            ui.label("Typical winning turn");
            optional_number(
                ui,
                "deck-win-turn",
                &mut self.typical_win_turn,
                20,
                "Undeclared",
            );
            ui.end_row();
            for (label, value) in [
                ("Infinite combos", &mut self.infinite_combos),
                ("Extra turns", &mut self.extra_turns),
                ("Mass land denial", &mut self.mass_land_denial),
                ("Fast mana", &mut self.fast_mana),
            ] {
                ui.label(label);
                egui::ComboBox::from_id_salt(("deck-disclosure", label))
                    .selected_text(value.label())
                    .show_ui(ui, |ui| {
                        for choice in Disclosure::ALL {
                            ui.selectable_value(value, choice, choice.label());
                        }
                    });
                ui.end_row();
            }
        });
        ui.label("Rule 0 notes: proxies, combos, house rules, game length, and table expectations");
        ui.add(
            egui::TextEdit::multiline(&mut self.rule_zero)
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .char_limit(4000),
        );
        if commander {
            ui.hyperlink_to(
                "Official Commander bracket guidance",
                "https://magic.wizards.com/en/formats/commander",
            );
            ui.label(RichText::new("Bracket names checked October 5, 2026. Brackets describe intended play experience; confirm current guidance and discuss exceptions with your pod.").small());
        }
        if ui.button("Copy pregame summary").clicked() {
            ui.ctx().copy_text(self.pregame_summary(name, commander));
        }
    }
}

fn optional_number(ui: &mut Ui, id: &str, value: &mut Option<u8>, max: u8, unset: &str) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.map_or_else(|| unset.into(), |n| n.to_string()))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, unset);
            for n in 1..=max {
                ui.selectable_value(value, Some(n), n.to_string());
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_defaults_and_declarations_are_validated() {
        assert_eq!(
            DeckProfile::from_json("{}").unwrap(),
            DeckProfile::default()
        );
        for input in [
            r#"{"power_level":0}"#,
            r#"{"power_level":11}"#,
            r#"{"commander_bracket":6}"#,
            r#"{"typical_win_turn":0}"#,
            r#"{"competitiveness":"automatic"}"#,
        ] {
            assert!(DeckProfile::from_json(input).is_err());
        }
        let profile = DeckProfile {
            power_level: Some(7),
            commander_bracket: Some(3),
            competitiveness: Competitiveness::Focused,
            infinite_combos: Disclosure::Included,
            rule_zero: "Proxies welcome; discuss combos.".into(),
            ..Default::default()
        };
        assert_eq!(
            DeckProfile::from_json(&serde_json::to_string(&profile).unwrap()).unwrap(),
            profile
        );
        assert!(profile.summary(true).contains("Upgraded"));
        assert!(!profile.summary(false).contains("Bracket"));
        assert!(
            profile
                .pregame_summary("My deck", true)
                .contains("Infinite combos: Included")
        );
    }
}
