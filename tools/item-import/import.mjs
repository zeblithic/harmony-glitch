#!/usr/bin/env node
/**
 * Import Glitch item and recipe definitions into harmony-glitch.
 *
 * Usage:
 *   node tools/item-import/import.mjs \
 *     --source ~/work/tinyspeck/glitch-GameServerJS/items \
 *     --output assets
 */

import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join, basename, extname } from 'node:path';
import { parseArgs } from 'node:util';
import { parseItem } from './parse-item.mjs';
import { parseRecipes } from './parse-recipes.mjs';

// ── CLI args ────────────────────────────────────────────────────────────

const { values: args } = parseArgs({
  options: {
    source: { type: 'string', short: 's' },
    output: { type: 'string', short: 'o' },
  },
});

if (!args.source || !args.output) {
  console.error('Usage: import.mjs --source <glitch-items-dir> --output <assets-dir>');
  process.exit(1);
}

const sourceDir = args.source;
const outputDir = args.output;

// ── Exclusion list ──────────────────────────────────────────────────────

const EXCLUDE_FILES = new Set(['bag.js', 'item.js']);
const EXCLUDE_PREFIXES = ['catalog_'];

function shouldExclude(filename) {
  if (EXCLUDE_FILES.has(filename)) return true;
  for (const prefix of EXCLUDE_PREFIXES) {
    if (filename.startsWith(prefix)) return true;
  }
  return false;
}

// ── Step 1: Parse all item files ────────────────────────────────────────

console.log(`Scanning ${sourceDir} for item definitions...`);

const files = readdirSync(sourceDir)
  .filter(f => extname(f) === '.js' && !shouldExclude(f));

const glitchItems = new Map();
let parseErrors = 0;

for (const file of files) {
  const id = basename(file, '.js');
  try {
    const content = readFileSync(join(sourceDir, file), 'utf-8');
    const item = parseItem(id, content);
    if (item) {
      glitchItems.set(id, item);
    }
  } catch (e) {
    parseErrors++;
    console.warn(`  Warning: failed to parse ${file}: ${e.message}`);
  }
}

console.log(`Parsed ${glitchItems.size} Glitch items (${parseErrors} errors, ${files.length - glitchItems.size - parseErrors} hidden/skipped)`);

// ── Step 2: Parse recipes ───────────────────────────────────────────────

const recipeCatalogPath = join(sourceDir, 'catalog_recipes.js');
console.log(`\nParsing recipes from ${recipeCatalogPath}...`);

// Build items map for validation (Glitch items + demo items we'll merge later)
const demoItemsPath = join(outputDir, 'items.json');
const demoItems = JSON.parse(readFileSync(demoItemsPath, 'utf-8'));

// Combined map for recipe validation
const allItemsMap = new Map(glitchItems);
for (const [id, def] of Object.entries(demoItems)) {
  if (!allItemsMap.has(id)) {
    allItemsMap.set(id, { category: def.category });
  }
}

const recipeCatalog = readFileSync(recipeCatalogPath, 'utf-8');
const { recipes: glitchRecipes, skipped: recipeSkipped } = parseRecipes(recipeCatalog, allItemsMap);

console.log(`Parsed ${Object.keys(glitchRecipes).length} Glitch recipes`);
if (recipeSkipped.length > 0) {
  console.log(`Skipped ${recipeSkipped.length} recipes (missing item refs):`);
  for (const msg of recipeSkipped) {
    console.log(`  ${msg}`);
  }
}

// ── Step 3: Merge items ─────────────────────────────────────────────────

console.log('\nMerging items...');

const mergedItems = {};
const replacedDemo = [];
const preservedDemo = [];

// Add all Glitch items
for (const [id, item] of glitchItems) {
  const { id: _id, ...rest } = item;
  mergedItems[id] = rest;
}

// Add demo-only items (IDs not in Glitch set)
for (const [id, def] of Object.entries(demoItems)) {
  if (glitchItems.has(id)) {
    replacedDemo.push(id);
  } else {
    preservedDemo.push(id);
    mergedItems[id] = def;
  }
}

// ── Step 4: Merge recipes ───────────────────────────────────────────────

console.log('Merging recipes...');

const demoRecipesPath = join(outputDir, 'recipes.json');
const demoRecipes = JSON.parse(readFileSync(demoRecipesPath, 'utf-8'));

const mergedRecipes = {};
const replacedDemoRecipes = [];
const preservedDemoRecipes = [];

// Add all Glitch recipes
for (const [id, recipe] of Object.entries(glitchRecipes)) {
  const { id: _id, ...rest } = recipe;
  mergedRecipes[id] = rest;
}

// Add demo recipes that don't collide
for (const [id, def] of Object.entries(demoRecipes)) {
  if (id in mergedRecipes) {
    replacedDemoRecipes.push(id);
  } else {
    preservedDemoRecipes.push(id);
    mergedRecipes[id] = def;
  }
}

// ── Step 5: Final validation ────────────────────────────────────────────

console.log('\nValidating referential integrity...');

let validationErrors = 0;
for (const [recipeId, recipe] of Object.entries(mergedRecipes)) {
  const refs = [
    ...recipe.inputs.map(i => i.item),
    ...(recipe.tools || []).map(t => t.item),
    ...recipe.outputs.map(o => o.item),
  ];
  for (const ref of refs) {
    if (!(ref in mergedItems)) {
      console.error(`  ERROR: recipe '${recipeId}' references missing item '${ref}'`);
      validationErrors++;
    }
  }
}

if (validationErrors > 0) {
  console.error(`\n${validationErrors} validation errors — aborting!`);
  process.exit(1);
}

console.log('Validation passed.');

// ── Step 6: Write output ────────────────────────────────────────────────

// Sort by key for stable output
const sortedItems = Object.fromEntries(
  Object.entries(mergedItems).sort(([a], [b]) => a.localeCompare(b))
);
const sortedRecipes = Object.fromEntries(
  Object.entries(mergedRecipes).sort(([a], [b]) => a.localeCompare(b))
);

const itemsOut = join(outputDir, 'items.json');
const recipesOut = join(outputDir, 'recipes.json');

writeFileSync(itemsOut, JSON.stringify(sortedItems, null, 2) + '\n');
writeFileSync(recipesOut, JSON.stringify(sortedRecipes, null, 2) + '\n');

// ── Report ──────────────────────────────────────────────────────────────

const categories = { food: 0, tool: 0, material: 0 };
for (const item of Object.values(mergedItems)) {
  categories[item.category] = (categories[item.category] || 0) + 1;
}

console.log(`
═══════════════════════════════════════════
  Import Complete
═══════════════════════════════════════════
  Items: ${Object.keys(mergedItems).length} total
    food: ${categories.food}, tool: ${categories.tool}, material: ${categories.material}
    Glitch: ${glitchItems.size}, demo preserved: ${preservedDemo.length}
    Demo replaced by Glitch: ${replacedDemo.join(', ') || 'none'}
    Demo-only preserved: ${preservedDemo.join(', ') || 'none'}

  Recipes: ${Object.keys(mergedRecipes).length} total
    Glitch: ${Object.keys(glitchRecipes).length}, demo preserved: ${preservedDemoRecipes.length}
    Demo replaced: ${replacedDemoRecipes.join(', ') || 'none'}
    Demo preserved: ${preservedDemoRecipes.join(', ') || 'none'}

  Written:
    ${itemsOut}
    ${recipesOut}
═══════════════════════════════════════════`);
