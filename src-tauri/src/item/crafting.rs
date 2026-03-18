use crate::item::inventory::Inventory;
use crate::item::types::{CraftError, CraftOutput, ItemDefs, RecipeDef, RecipeItem};

/// Execute a crafting recipe against the player's inventory.
///
/// Pure function: validates tools, inputs, and output room, then atomically
/// consumes inputs and produces outputs. Tools are required but not consumed.
pub fn craft(
    recipe: &RecipeDef,
    inventory: &mut Inventory,
    item_defs: &ItemDefs,
) -> Result<Vec<CraftOutput>, CraftError> {
    // 1. Validate tools
    for tool in &recipe.tools {
        let have = inventory.count_item(&tool.item);
        if have < tool.count {
            return Err(CraftError::MissingTool {
                item: tool.item.clone(),
            });
        }
    }

    // 2. Validate inputs
    for input in &recipe.inputs {
        let have = inventory.count_item(&input.item);
        if have < input.count {
            return Err(CraftError::MissingInput {
                item: input.item.clone(),
                need: input.count,
                have,
            });
        }
    }

    // 3. Validate output room (count-aware to prevent item loss)
    for output in &recipe.outputs {
        if !inventory.has_room_for_count(&output.item, output.count, item_defs) {
            return Err(CraftError::NoRoom);
        }
    }

    // 4. Consume inputs (tools NOT consumed)
    for input in &recipe.inputs {
        inventory.remove_item(&input.item, input.count);
    }

    // 5. Add outputs and build result
    let mut outputs = Vec::new();
    for output in &recipe.outputs {
        inventory.add(&output.item, output.count, item_defs);
        let name = item_defs
            .get(&output.item)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| output.item.clone());
        outputs.push(CraftOutput {
            item_id: output.item.clone(),
            name,
            count: output.count,
        });
    }

    Ok(outputs)
}

/// Check whether a recipe can be crafted with the current inventory.
/// Used as a reference implementation — the frontend mirrors this in TypeScript.
pub fn check_recipe_availability(
    recipe: &RecipeDef,
    inventory: &Inventory,
) -> crate::item::types::RecipeAvailability {
    use crate::item::types::{IngredientStatus, RecipeAvailability};

    let inputs: Vec<IngredientStatus> = recipe
        .inputs
        .iter()
        .map(|input| IngredientStatus {
            item: input.item.clone(),
            need: input.count,
            have: inventory.count_item(&input.item),
        })
        .collect();

    let tools: Vec<IngredientStatus> = recipe
        .tools
        .iter()
        .map(|tool| IngredientStatus {
            item: tool.item.clone(),
            need: tool.count,
            have: inventory.count_item(&tool.item),
        })
        .collect();

    let craftable = inputs.iter().all(|i| i.have >= i.need)
        && tools.iter().all(|t| t.have >= t.need);

    RecipeAvailability {
        craftable,
        inputs,
        tools,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::ItemDef;

    fn test_defs() -> ItemDefs {
        let mut defs = ItemDefs::new();
        for (id, name, stack_limit) in [
            ("cherry", "Cherry", 50),
            ("grain", "Grain", 50),
            ("cherry_pie", "Cherry Pie", 10),
            ("pot", "Pot", 1),
            ("wood", "Wood", 50),
            ("plank", "Plank", 50),
        ] {
            defs.insert(
                id.into(),
                ItemDef {
                    id: id.into(),
                    name: name.into(),
                    description: "".into(),
                    category: "".into(),
                    stack_limit,
                    icon: id.into(),
                },
            );
        }
        defs
    }

    fn cherry_pie_recipe() -> RecipeDef {
        RecipeDef {
            id: "cherry_pie".into(),
            name: "Cherry Pie".into(),
            description: "".into(),
            inputs: vec![
                RecipeItem { item: "cherry".into(), count: 5 },
                RecipeItem { item: "grain".into(), count: 2 },
            ],
            tools: vec![RecipeItem { item: "pot".into(), count: 1 }],
            outputs: vec![RecipeItem { item: "cherry_pie".into(), count: 1 }],
            duration_secs: 10.0,
            category: "food".into(),
        }
    }

    fn plank_recipe() -> RecipeDef {
        RecipeDef {
            id: "plank".into(),
            name: "Plank".into(),
            description: "".into(),
            inputs: vec![RecipeItem { item: "wood".into(), count: 3 }],
            tools: vec![],
            outputs: vec![RecipeItem { item: "plank".into(), count: 2 }],
            duration_secs: 4.0,
            category: "material".into(),
        }
    }

    #[test]
    fn craft_success_consumes_inputs_keeps_tools() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 10, &defs);
        inv.add("grain", 5, &defs);
        inv.add("pot", 1, &defs);

        let result = craft(&cherry_pie_recipe(), &mut inv, &defs).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].item_id, "cherry_pie");
        assert_eq!(result[0].count, 1);
        assert_eq!(inv.count_item("cherry"), 5);
        assert_eq!(inv.count_item("grain"), 3);
        assert_eq!(inv.count_item("pot"), 1);
        assert_eq!(inv.count_item("cherry_pie"), 1);
    }

    #[test]
    fn craft_missing_input_returns_error() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 2, &defs);
        inv.add("grain", 5, &defs);
        inv.add("pot", 1, &defs);

        let err = craft(&cherry_pie_recipe(), &mut inv, &defs).unwrap_err();
        match err {
            CraftError::MissingInput { item, need, have } => {
                assert_eq!(item, "cherry");
                assert_eq!(need, 5);
                assert_eq!(have, 2);
            }
            _ => panic!("Expected MissingInput, got {:?}", err),
        }
        assert_eq!(inv.count_item("cherry"), 2);
        assert_eq!(inv.count_item("grain"), 5);
    }

    #[test]
    fn craft_missing_tool_returns_error() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 10, &defs);
        inv.add("grain", 5, &defs);

        let err = craft(&cherry_pie_recipe(), &mut inv, &defs).unwrap_err();
        match err {
            CraftError::MissingTool { item } => assert_eq!(item, "pot"),
            _ => panic!("Expected MissingTool, got {:?}", err),
        }
    }

    #[test]
    fn craft_no_room_returns_error_nothing_consumed() {
        let defs = test_defs();
        let mut inv = Inventory::new(3);
        inv.add("cherry", 5, &defs);
        inv.add("grain", 2, &defs);
        inv.add("pot", 1, &defs);

        let err = craft(&cherry_pie_recipe(), &mut inv, &defs).unwrap_err();
        match err {
            CraftError::NoRoom => {}
            _ => panic!("Expected NoRoom, got {:?}", err),
        }
        assert_eq!(inv.count_item("cherry"), 5);
        assert_eq!(inv.count_item("grain"), 2);
    }

    #[test]
    fn craft_multiple_outputs() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("wood", 3, &defs);

        let result = craft(&plank_recipe(), &mut inv, &defs).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].item_id, "plank");
        assert_eq!(result[0].count, 2);
        assert_eq!(inv.count_item("wood"), 0);
        assert_eq!(inv.count_item("plank"), 2);
    }

    #[test]
    fn craft_no_tool_recipe_succeeds_without_tools() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("wood", 3, &defs);

        let result = craft(&plank_recipe(), &mut inv, &defs);
        assert!(result.is_ok());
    }

    #[test]
    fn check_availability_craftable() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 10, &defs);
        inv.add("grain", 5, &defs);
        inv.add("pot", 1, &defs);

        let avail = check_recipe_availability(&cherry_pie_recipe(), &inv);
        assert!(avail.craftable);
        assert_eq!(avail.inputs.len(), 2);
        assert!(avail.inputs.iter().all(|i| i.have >= i.need));
        assert_eq!(avail.tools.len(), 1);
        assert!(avail.tools[0].have >= avail.tools[0].need);
    }

    #[test]
    fn check_availability_not_craftable() {
        let defs = test_defs();
        let mut inv = Inventory::new(16);
        inv.add("cherry", 2, &defs);

        let avail = check_recipe_availability(&cherry_pie_recipe(), &inv);
        assert!(!avail.craftable);
        let cherry_status = avail.inputs.iter().find(|i| i.item == "cherry").unwrap();
        assert_eq!(cherry_status.have, 2);
        assert_eq!(cherry_status.need, 5);
    }
}
