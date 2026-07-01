import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const marketCssPath = join(process.cwd(), "frontend", "src", "styles", "features", "market.css");
const tokensCssPath = join(process.cwd(), "frontend", "src", "styles", "tokens.css");

const marketCss = readFileSync(marketCssPath, "utf8");
const themeCss = `${readFileSync(tokensCssPath, "utf8")}\n${marketCss}`;

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function selectorBlocks(source, selector) {
  const pattern = new RegExp(`${escapeRegExp(selector)}\\s*\\{([\\s\\S]*?)\\}`, "g");
  return [...source.matchAll(pattern)].map((match) => match[1]);
}

function declarationsFor(source, selector) {
  const blocks = selectorBlocks(source, selector);
  assert.ok(blocks.length > 0, `${selector} block should exist`);
  return blocks.join("\n");
}

const rail = declarationsFor(marketCss, ".market-filter-rail");
assert.match(
  rail,
  /background:\s*var\(--market-filter-rail-bg\)\s*;/,
  "market category rail background must use theme-aware variable"
);

const hover = declarationsFor(marketCss, ".market-filter-chip:hover");
assert.match(
  hover,
  /background:\s*var\(--market-filter-chip-hover-bg\)\s*;/,
  "market category hover background must use theme-aware variable"
);
assert.match(
  hover,
  /border-color:\s*var\(--market-filter-chip-hover-border\)\s*;/,
  "market category hover border must use theme-aware variable"
);

const active = declarationsFor(marketCss, ".market-filter-chip.active");
assert.match(
  active,
  /box-shadow:\s*var\(--market-filter-chip-active-shadow\)\s*;/,
  "market category active state must use theme-aware shadow"
);

const darkTheme = declarationsFor(marketCss, '.app-shell[data-theme="dark"] .market-grid');
for (const token of [
  "--market-filter-rail-bg",
  "--market-filter-chip-hover-bg",
  "--market-filter-chip-hover-border",
  "--market-filter-chip-active-shadow"
]) {
  assert.match(darkTheme, new RegExp(`${token}\\s*:`), `dark theme must define ${token}`);
}
