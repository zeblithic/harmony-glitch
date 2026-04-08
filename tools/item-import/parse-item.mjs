/**
 * Parse a Glitch item JS file into a harmony-glitch ItemDef.
 *
 * Each .js file in glitch-GameServerJS/items/ defines an item using
 * `var` declarations. We extract the fields we need with regex.
 */

/**
 * @param {string} filename - Item ID (filename without .js)
 * @param {string} content - Full file contents
 * @returns {{ id: string, name: string, description: string, category: string, stackLimit: number, icon: string, baseCost: number|null, energyValue: number|null } | null}
 */
export function parseItem(filename, content) {
  // Must have a label to be a real item definition
  const label = extractString(content, 'label');
  if (label === null) return null;

  const isHidden = extractBool(content, 'is_hidden');
  if (isHidden) return null;

  const description = cleanDescription(extractString(content, 'description') ?? '');
  const stackmax = extractInt(content, 'stackmax');
  const baseCost = extractInt(content, 'base_cost');
  const parentClasses = extractStringArray(content, 'parent_classes');

  // Category mapping
  let category;
  if (parentClasses.includes('food') || parentClasses.includes('drink')) {
    category = 'food';
  } else if (parentClasses.includes('tool_base')) {
    category = 'tool';
  } else {
    category = 'material';
  }

  // Energy calculation (food items only)
  let energyValue = null;
  if (parentClasses.includes('food')) {
    const energyFactor = extractClassProp(content, 'energy_factor');
    if (energyFactor !== null && baseCost > 0) {
      energyValue = Math.round(baseCost * parseFloat(energyFactor));
      if (energyValue <= 0) energyValue = null;
    }
  } else if (parentClasses.includes('drink')) {
    const drinkEnergy = extractClassProp(content, 'drink_energy');
    if (drinkEnergy !== null) {
      const val = parseInt(drinkEnergy, 10);
      if (val > 0) energyValue = val;
    }
  }

  return {
    id: filename,
    name: label,
    description,
    category,
    stackLimit: stackmax || 1,
    icon: filename,
    baseCost: baseCost > 0 ? baseCost : null,
    energyValue,
  };
}

// ── Regex extractors ────────────────────────────────────────────────────

function extractString(content, varName) {
  const m = content.match(new RegExp(`var\\s+${varName}\\s*=\\s*"((?:[^"\\\\]|\\\\.)*)"`));
  return m ? m[1].replace(/\\"/g, '"').replace(/\\\\/g, '\\') : null;
}

function extractBool(content, varName) {
  const m = content.match(new RegExp(`var\\s+${varName}\\s*=\\s*(true|false)`));
  return m ? m[1] === 'true' : false;
}

function extractInt(content, varName) {
  const m = content.match(new RegExp(`var\\s+${varName}\\s*=\\s*(-?\\d+)`));
  return m ? parseInt(m[1], 10) : 0;
}

function extractStringArray(content, varName) {
  const m = content.match(new RegExp(`var\\s+${varName}\\s*=\\s*\\[([^\\]]*)]`));
  if (!m) return [];
  const items = [];
  const re = /"([^"]*)"/g;
  let match;
  while ((match = re.exec(m[1])) !== null) {
    items.push(match[1]);
  }
  return items;
}

function extractClassProp(content, propName) {
  const m = content.match(new RegExp(`"${propName}"\\s*:\\s*"([^"]*)"`));
  return m ? m[1] : null;
}

// ── Description cleanup ─────────────────────────────────────────────────

export function cleanDescription(desc) {
  let s = desc;
  // Unescape JS escapes
  s = s.replace(/\\\//g, '/');
  s = s.replace(/\\r\\n|\\n/g, ' ');
  // Strip HTML tags, keeping inner text
  s = s.replace(/<[^>]+>/g, '');
  // Collapse whitespace
  s = s.replace(/\s+/g, ' ').trim();
  return s;
}
