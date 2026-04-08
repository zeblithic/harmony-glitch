/**
 * Parse Glitch catalog_recipes.js into harmony-glitch RecipeDef objects.
 *
 * The file contains recipe blocks like:
 *   this.recipes["1"] = { name: "Simple Slaw", ... };
 *
 * We extract them using regex on each block.
 */

/**
 * @typedef {{ item: string, count: number }} RecipeItem
 * @typedef {{ id: string, name: string, description: string, inputs: RecipeItem[], tools: RecipeItem[], outputs: RecipeItem[], durationSecs: number, energyCost: number, category: string }} RecipeDef
 */

/**
 * @param {string} content - Full contents of catalog_recipes.js
 * @param {Map<string, { category: string }>} itemsMap - Parsed items for category lookup
 * @returns {{ recipes: Record<string, RecipeDef>, skipped: string[] }}
 */
export function parseRecipes(content, itemsMap) {
  const recipes = {};
  const skipped = [];

  // Split on recipe block boundaries
  const blockRe = /this\.recipes\["(\d+)"\]\s*=\s*\{([\s\S]*?)\};/g;
  let match;

  while ((match = blockRe.exec(content)) !== null) {
    const numId = match[1];
    const body = match[2];

    const name = extractField(body, 'name');
    const tool = extractField(body, 'tool');
    const energyCost = extractNumField(body, 'energy_cost');
    const waitMs = extractNumField(body, 'wait_ms');
    const inputs = extractItemArray(body, 'inputs');
    const outputs = extractItemArray(body, 'outputs');

    if (!name || outputs.length === 0) {
      skipped.push(`recipe ${numId}: missing name or outputs`);
      continue;
    }

    // Validate all item references exist
    const allRefs = [
      ...inputs.map(i => i.item),
      ...outputs.map(o => o.item),
      ...(tool ? [tool] : []),
    ];
    const missing = allRefs.filter(id => !itemsMap.has(id));
    if (missing.length > 0) {
      skipped.push(`recipe ${numId} (${name}): missing items: ${missing.join(', ')}`);
      continue;
    }

    // Recipe ID = primary output item ID
    const recipeId = outputs[0].item;

    // Derive category from output item
    const outputItem = itemsMap.get(recipeId);
    const category = outputItem ? outputItem.category : 'material';

    recipes[recipeId] = {
      id: recipeId,
      name,
      description: '',
      inputs,
      tools: tool ? [{ item: tool, count: 1 }] : [],
      outputs,
      durationSecs: waitMs / 1000,
      energyCost,
      category,
    };
  }

  return { recipes, skipped };
}

// ── Field extractors ────────────────────────────────────────────────────

function extractField(body, fieldName) {
  const m = body.match(new RegExp(`${fieldName}\\s*:\\s*"([^"]*)"`));
  return m ? m[1] : null;
}

function extractNumField(body, fieldName) {
  const m = body.match(new RegExp(`${fieldName}\\s*:\\s*(-?\\d+)`));
  return m ? parseInt(m[1], 10) : 0;
}

function extractItemArray(body, fieldName) {
  // Find the field, then use bracket-depth counting to extract the full array
  const start = body.indexOf(`${fieldName} :`);
  if (start === -1) return [];

  const openBracket = body.indexOf('[', start);
  if (openBracket === -1) return [];

  let depth = 1;
  let i = openBracket + 1;
  while (i < body.length && depth > 0) {
    if (body[i] === '[') depth++;
    else if (body[i] === ']') depth--;
    i++;
  }

  const section = body.substring(openBracket + 1, i - 1);
  const items = [];
  const re = /\["([^"]+)",\s*(\d+)\]/g;
  let im;
  while ((im = re.exec(section)) !== null) {
    items.push({ item: im[1], count: parseInt(im[2], 10) });
  }
  return items;
}
