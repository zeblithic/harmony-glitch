use crate::skill::types::SkillDefs;

/// Parse skill definitions from JSON string.
/// The JSON is a map of id -> SkillDef. We set each SkillDef.id from its map key.
pub fn parse_skill_defs(json: &str) -> Result<SkillDefs, String> {
    let mut raw: SkillDefs =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse skills.json: {e}"))?;
    for (key, def) in raw.iter_mut() {
        def.id = key.clone();
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_defs_from_json() {
        let json = r#"{
            "cooking_1": {
                "name": "Cooking I",
                "description": "Basic cooking.",
                "prerequisites": [],
                "imaginationCost": 50,
                "learnTimeSecs": 120,
                "unlocksRecipes": ["bread", "butter"]
            }
        }"#;
        let defs = parse_skill_defs(json).unwrap();
        assert_eq!(defs.len(), 1);
        let skill = &defs["cooking_1"];
        assert_eq!(skill.id, "cooking_1");
        assert_eq!(skill.name, "Cooking I");
        assert_eq!(skill.imagination_cost, 50);
        assert_eq!(skill.learn_time_secs, 120);
        assert_eq!(skill.prerequisites.len(), 0);
        assert_eq!(skill.unlocks_recipes, vec!["bread", "butter"]);
    }

    #[test]
    fn parse_skill_with_prerequisites() {
        let json = r#"{
            "cooking_1": {
                "name": "Cooking I",
                "description": "Basic.",
                "imaginationCost": 50,
                "learnTimeSecs": 120
            },
            "cooking_2": {
                "name": "Cooking II",
                "description": "Advanced.",
                "prerequisites": ["cooking_1"],
                "imaginationCost": 150,
                "learnTimeSecs": 600,
                "unlocksRecipes": ["cherry_pie"]
            }
        }"#;
        let defs = parse_skill_defs(json).unwrap();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs["cooking_2"].prerequisites, vec!["cooking_1"]);
    }

    #[test]
    fn parse_bundled_skills_json() {
        let json = include_str!("../../../assets/skills.json");
        let defs = parse_skill_defs(json).unwrap();
        assert_eq!(defs.len(), 4);
        assert!(defs.contains_key("cooking_1"));
        assert!(defs.contains_key("cooking_2"));
        assert!(defs.contains_key("woodwork_1"));
        assert!(defs.contains_key("toymaking_1"));
        // Verify prerequisite chains
        assert!(defs["cooking_1"].prerequisites.is_empty());
        assert_eq!(defs["cooking_2"].prerequisites, vec!["cooking_1"]);
        assert!(defs["woodwork_1"].prerequisites.is_empty());
        assert_eq!(defs["toymaking_1"].prerequisites, vec!["woodwork_1"]);
    }
}
