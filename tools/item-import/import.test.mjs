import { describe, it, expect } from 'vitest';
import { parseItem, cleanDescription } from './parse-item.mjs';
import { parseRecipes } from './parse-recipes.mjs';
import { readFileSync, readdirSync } from 'node:fs';
import { join, basename, extname } from 'node:path';

// ---------------------------------------------------------------------------
// parseItem
// ---------------------------------------------------------------------------

describe('parseItem', () => {
  it('parses a food item with energy', () => {
    const content = `
var label = "Apple";
var description = "A boldly red and brazenly juicy apple.";
var is_hidden = false;
var stackmax = 100;
var base_cost = 5;
var parent_classes = ["apple", "food", "takeable"];
var classProps = {
  "energy_factor": "1"
};
`;
    const item = parseItem('apple', content);
    expect(item).not.toBeNull();
    expect(item.id).toBe('apple');
    expect(item.name).toBe('Apple');
    expect(item.description).toBe('A boldly red and brazenly juicy apple.');
    expect(item.category).toBe('food');
    expect(item.stackLimit).toBe(100);
    expect(item.baseCost).toBe(5);
    expect(item.energyValue).toBe(5); // 5 * 1
    expect(item.icon).toBe('apple');
  });

  it('returns null for hidden items', () => {
    const content = `
var label = "Secret Thing";
var is_hidden = true;
var stackmax = 1;
var base_cost = 0;
var parent_classes = ["takeable"];
`;
    expect(parseItem('secret', content)).toBeNull();
  });

  it('returns null for files without label (system files)', () => {
    const content = `function doSomething() { return 42; }`;
    expect(parseItem('system', content)).toBeNull();
  });

  it('categorizes tools correctly', () => {
    const content = `
var label = "Knife & Board";
var is_hidden = false;
var stackmax = 1;
var base_cost = 75;
var parent_classes = ["knife_and_board", "tool_base", "takeable"];
var classProps = {
  "points_capacity": "100"
};
`;
    const item = parseItem('knife_and_board', content);
    expect(item.category).toBe('tool');
    expect(item.energyValue).toBeNull();
    expect(item.baseCost).toBe(75);
  });

  it('categorizes materials correctly', () => {
    const content = `
var label = "Sparkly";
var is_hidden = false;
var stackmax = 250;
var base_cost = 3;
var parent_classes = ["sparkly", "takeable"];
`;
    const item = parseItem('sparkly', content);
    expect(item.category).toBe('material');
  });

  it('handles zero base_cost as null', () => {
    const content = `
var label = "Free Thing";
var is_hidden = false;
var stackmax = 1;
var base_cost = 0;
var parent_classes = ["takeable"];
`;
    const item = parseItem('free_thing', content);
    expect(item.baseCost).toBeNull();
  });

  it('calculates energy with non-1 factor', () => {
    const content = `
var label = "Rich Food";
var is_hidden = false;
var stackmax = 50;
var base_cost = 20;
var parent_classes = ["food", "takeable"];
var classProps = {
  "energy_factor": "5"
};
`;
    const item = parseItem('rich_food', content);
    expect(item.energyValue).toBe(100); // 20 * 5
  });

  it('handles fractional energy factor', () => {
    const content = `
var label = "Light Snack";
var is_hidden = false;
var stackmax = 50;
var base_cost = 3;
var parent_classes = ["food", "takeable"];
var classProps = {
  "energy_factor": "0.4"
};
`;
    const item = parseItem('light_snack', content);
    expect(item.energyValue).toBe(1); // round(3 * 0.4) = round(1.2) = 1
  });

  it('handles food with zero base_cost (no energy)', () => {
    const content = `
var label = "Freebie Food";
var is_hidden = false;
var stackmax = 50;
var base_cost = 0;
var parent_classes = ["food", "takeable"];
var classProps = {
  "energy_factor": "1"
};
`;
    const item = parseItem('freebie', content);
    expect(item.energyValue).toBeNull();
    expect(item.baseCost).toBeNull();
  });

  it('defaults stackLimit to 1 when stackmax is 0 or missing', () => {
    const content = `
var label = "No Stack";
var is_hidden = false;
var base_cost = 10;
var parent_classes = ["takeable"];
`;
    const item = parseItem('no_stack', content);
    expect(item.stackLimit).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// cleanDescription
// ---------------------------------------------------------------------------

describe('cleanDescription', () => {
  it('strips HTML tags keeping inner text', () => {
    expect(cleanDescription('A <a href="/items/265/">Fruit Machine</a> makes this.'))
      .toBe('A Fruit Machine makes this.');
  });

  it('handles escaped slashes', () => {
    expect(cleanDescription('See <a href="\\/items\\/1\\/">here<\\/a>.'))
      .toBe('See here.');
  });

  it('normalizes escaped newlines', () => {
    expect(cleanDescription('Line one.\\r\\nLine two.\\nLine three.'))
      .toBe('Line one. Line two. Line three.');
  });

  it('collapses whitespace', () => {
    expect(cleanDescription('  too   many    spaces  '))
      .toBe('too many spaces');
  });

  it('returns empty string for empty input', () => {
    expect(cleanDescription('')).toBe('');
  });
});

// ---------------------------------------------------------------------------
// parseRecipes
// ---------------------------------------------------------------------------

describe('parseRecipes', () => {
  const sampleItems = new Map([
    ['cabbage', { category: 'food' }],
    ['corn', { category: 'food' }],
    ['bean_plain', { category: 'food' }],
    ['simple_slaw', { category: 'food' }],
    ['knife_and_board', { category: 'tool' }],
    ['meat', { category: 'food' }],
    ['potato', { category: 'food' }],
    ['hash', { category: 'food' }],
    ['frying_pan', { category: 'tool' }],
  ]);

  it('parses a single recipe block', () => {
    const content = `
this.recipes["1"] = {
  name : "Simple Slaw",
  skill : "ezcooking_2",
  skills : ["ezcooking_2"],
  achievements : [],
  tool : "knife_and_board",
  tool_wear : 1,
  learnt : 1,
  energy_cost : 3,
  xp_reward : 2,
  wait_ms : 2000,
  task_limit : 40,
  inputs : [
    ["cabbage", 1],
    ["corn", 1],
    ["bean_plain", 5],
  ],
  outputs : [
    ["simple_slaw", 1],
  ],
};
`;
    const { recipes, skipped } = parseRecipes(content, sampleItems);
    expect(skipped).toHaveLength(0);
    expect(Object.keys(recipes)).toHaveLength(1);

    const slaw = recipes['simple_slaw'];
    expect(slaw.name).toBe('Simple Slaw');
    expect(slaw.inputs).toEqual([
      { item: 'cabbage', count: 1 },
      { item: 'corn', count: 1 },
      { item: 'bean_plain', count: 5 },
    ]);
    expect(slaw.tools).toEqual([{ item: 'knife_and_board', count: 1 }]);
    expect(slaw.outputs).toEqual([{ item: 'simple_slaw', count: 1 }]);
    expect(slaw.durationSecs).toBe(2);
    expect(slaw.energyCost).toBe(3);
    expect(slaw.category).toBe('food');
    expect(slaw.description).toBe('');
  });

  it('skips recipes with missing item references', () => {
    const content = `
this.recipes["99"] = {
  name : "Missing Stuff",
  tool : "knife_and_board",
  energy_cost : 5,
  wait_ms : 1000,
  inputs : [
    ["nonexistent_item", 1],
  ],
  outputs : [
    ["also_missing", 1],
  ],
};
`;
    const { recipes, skipped } = parseRecipes(content, sampleItems);
    expect(Object.keys(recipes)).toHaveLength(0);
    expect(skipped).toHaveLength(1);
    expect(skipped[0]).toContain('nonexistent_item');
  });

  it('parses multiple recipes', () => {
    const content = `
this.recipes["1"] = {
  name : "Simple Slaw",
  tool : "knife_and_board",
  energy_cost : 3,
  wait_ms : 2000,
  inputs : [
    ["cabbage", 1],
    ["corn", 1],
    ["bean_plain", 5],
  ],
  outputs : [
    ["simple_slaw", 1],
  ],
};

this.recipes["5"] = {
  name : "Hash",
  tool : "frying_pan",
  energy_cost : 6,
  wait_ms : 2000,
  inputs : [
    ["meat", 1],
    ["potato", 2],
    ["corn", 2],
  ],
  outputs : [
    ["hash", 1],
  ],
};
`;
    const { recipes } = parseRecipes(content, sampleItems);
    expect(Object.keys(recipes)).toHaveLength(2);
    expect(recipes['simple_slaw']).toBeDefined();
    expect(recipes['hash']).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// Integration: parse real Glitch data (requires source files)
// ---------------------------------------------------------------------------

const GLITCH_ITEMS_DIR = process.env.GLITCH_ITEMS_DIR
  || `${process.env.HOME}/work/tinyspeck/glitch-GameServerJS/items`;

describe('integration: real Glitch data', () => {
  let hasSourceFiles = false;
  try {
    readdirSync(GLITCH_ITEMS_DIR);
    hasSourceFiles = true;
  } catch { /* skip */ }

  it.skipIf(!hasSourceFiles)('parses all item files', () => {
    const EXCLUDE = new Set(['bag.js', 'item.js']);
    const files = readdirSync(GLITCH_ITEMS_DIR)
      .filter(f => extname(f) === '.js' && !EXCLUDE.has(f) && !f.startsWith('catalog_'));

    const items = new Map();
    let errors = 0;
    for (const file of files) {
      const id = basename(file, '.js');
      const content = readFileSync(join(GLITCH_ITEMS_DIR, file), 'utf-8');
      const item = parseItem(id, content);
      if (item) items.set(id, item);
      else if (content.includes('var label')) errors++; // hidden items counted as "errors" for stats
    }

    // Should have 1200+ items (1288 total - 26 hidden - system files)
    expect(items.size).toBeGreaterThan(1200);

    // Spot-check known items
    expect(items.has('apple')).toBe(true);
    expect(items.get('apple').name).toBe('Apple');
    expect(items.get('apple').category).toBe('food');
    expect(items.get('apple').baseCost).toBe(5);

    expect(items.has('knife_and_board')).toBe(true);
    expect(items.get('knife_and_board').category).toBe('tool');
  });

  it.skipIf(!hasSourceFiles)('parses all recipes', () => {
    // First parse all items
    const EXCLUDE = new Set(['bag.js', 'item.js']);
    const files = readdirSync(GLITCH_ITEMS_DIR)
      .filter(f => extname(f) === '.js' && !EXCLUDE.has(f) && !f.startsWith('catalog_'));

    const items = new Map();
    for (const file of files) {
      const id = basename(file, '.js');
      const content = readFileSync(join(GLITCH_ITEMS_DIR, file), 'utf-8');
      const item = parseItem(id, content);
      if (item) items.set(id, item);
    }

    const recipeCatalog = readFileSync(
      join(GLITCH_ITEMS_DIR, 'catalog_recipes.js'), 'utf-8'
    );
    const { recipes, skipped } = parseRecipes(recipeCatalog, items);

    // Should have 300+ recipes
    expect(Object.keys(recipes).length).toBeGreaterThan(300);

    // Very few should be skipped (only if items are hidden)
    expect(skipped.length).toBeLessThan(50);
  });
});
