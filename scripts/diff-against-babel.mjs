#!/usr/bin/env bun
/**
 * Compare OXC SSR output to Babel SSR output for a corpus of .tsx files.
 *
 * Usage:
 *   bun scripts/diff-against-babel.mjs <file-or-dir> [<file-or-dir>...]
 *
 * For each input file:
 *   1. Parse + transform with @babel/core using @babel/preset-typescript +
 *      babel-plugin-jsx-dom-expressions (the reference implementation).
 *   2. Transform with the OXC native binding.
 *   3. Diff the two outputs after a small normalization pass.
 *
 * The script prints:
 *   - per-file: OK / DIFF / ERROR with a unified diff for the first N differing files
 *   - aggregate stats at the end
 *
 * Exit code: 0 if all files match (after normalization), 1 if any file diffs
 * or errors.
 */

import { promises as fs } from "node:fs";
import { existsSync } from "node:fs";
import { dirname, join, relative, resolve, extname, basename } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");

// ---------- imports ----------
const babel = await import(join(ROOT, "node_modules/@babel/core/lib/index.js"))
  .catch(() => import("@babel/core"));
const babelPresetTs = await import(join(ROOT, "packages/babel-plugin-jsx-dom-expressions/node_modules/@babel/preset-typescript/lib/index.js"))
  .catch(async () => {
    // fall back: resolve from babel plugin folder
    const p = join(ROOT, "packages/babel-plugin-jsx-dom-expressions/node_modules/@babel/preset-typescript");
    return import(p);
  });
const jsxDomExpressionsModule = await import(
  join(ROOT, "packages/babel-plugin-jsx-dom-expressions/index.js")
);
const jsxDomExpressions = jsxDomExpressionsModule.default ?? jsxDomExpressionsModule;
const oxcModule = await import(
  join(ROOT, "packages/solid-jsx-oxc/index.js")
);

// ---------- options ----------
// These mirror Gothab's vite.config.ts SSR plugin options.
const SHARED_OPTIONS = {
  moduleName: "solid-js/web",
  generate: "ssr",
  hydratable: true,
  contextToCustomElements: true,
  builtIns: [
    "For",
    "Show",
    "Switch",
    "Match",
    "Suspense",
    "SuspenseList",
    "Portal",
    "Index",
    "Dynamic",
    "ErrorBoundary",
  ],
  wrapConditionals: true,
  delegateEvents: true,
};

// ---------- helpers ----------

async function collectFiles(input) {
  const out = [];
  const stat = await fs.stat(input);
  if (stat.isFile()) {
    if (/\.(tsx|jsx)$/.test(input)) out.push(input);
    return out;
  }
  if (stat.isDirectory()) {
    const entries = await fs.readdir(input, { withFileTypes: true });
    for (const e of entries) {
      const full = join(input, e.name);
      if (e.isDirectory()) {
        if (e.name === "node_modules" || e.name === "dist" || e.name.startsWith(".")) continue;
        out.push(...(await collectFiles(full)));
      } else if (/\.(tsx|jsx)$/.test(e.name)) {
        out.push(full);
      }
    }
  }
  return out;
}

function transformBabel(source, filename) {
  const result = babel.transformSync(source, {
    filename,
    babelrc: false,
    configFile: false,
    presets: [
      [
        babelPresetTs.default ?? babelPresetTs,
        { isTSX: true, allExtensions: true, allowDeclareFields: true },
      ],
    ],
    plugins: [[jsxDomExpressions, SHARED_OPTIONS]],
    sourceMaps: false,
  });
  return result?.code ?? "";
}

function transformOxc(source, filename) {
  const result = oxcModule.transform(source, {
    ...SHARED_OPTIONS,
    filename,
  });
  return result?.code ?? "";
}

/**
 * Strip purely-cosmetic differences from compiled output before diffing.
 *
 * - Renames `_$tmpl` and `_tmpl$` style identifiers to a canonical form.
 * - Collapses leading whitespace to single spaces (the two codegens format
 *   differently — Babel uses tab-ish indentation, OXC uses 2-space).
 * - Removes trailing whitespace and collapses blank-line runs.
 * - Normalizes leading `_$` vs no-`_$` on imported helpers (both refer to the
 *   same imported symbol; the codegens disagree on whether to rename on
 *   import).
 *
 * The goal is for normalized output to match byte-for-byte when the runtime
 * behavior is the same, so any remaining diff is *substantive*.
 */
function normalize(code) {
  let s = code;

  // Babel's `import { ssr as _$ssr, ... }` vs OXC's `import { ssr, ... }`:
  // strip leading `_$` from helper references so both shapes line up.
  s = s.replace(/_\$([a-zA-Z_][a-zA-Z0-9_]*)/g, "$1");

  // Babel: `var _tmpl$1 = ["..."]`; OXC: `const _tmpl1 = [...]`.
  // Canonicalize template-variable names: any identifier starting with
  // `_tmpl` followed by `$N` or `N` becomes `_tmpl_N`.
  s = s.replace(/_tmpl\$(\d+)/g, "_tmpl_$1");
  s = s.replace(/_tmpl(\d+)/g, "_tmpl_$1");

  // Babel uses `var`, OXC uses `const` / `let`. Treat all top-level decls
  // identically by replacing `var |let |const ` with `__decl__ `.
  s = s.replace(/(^|\n)\s*(var|let|const)\s+/g, "$1__decl__ ");

  // Collapse runs of whitespace within a line (incl. tabs) to a single space.
  s = s.split("\n").map((line) => line.replace(/[ \t]+/g, " ").replace(/\s+$/, "")).join("\n");

  // Drop blank lines.
  s = s.replace(/\n{2,}/g, "\n");
  return s.trim();
}

function unifiedDiff(a, b, contextLines = 3) {
  const aLines = a.split("\n");
  const bLines = b.split("\n");
  // Simple LCS-based diff
  const n = aLines.length, m = bLines.length;
  // For perf cap at 4000 lines per side
  if (n > 4000 || m > 4000) {
    return `(diff suppressed: too large; ${n} vs ${m} lines)`;
  }
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      if (aLines[i] === bLines[j]) dp[i][j] = dp[i + 1][j + 1] + 1;
      else dp[i][j] = Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const ops = []; // { type: 'eq' | 'del' | 'add', text }
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (aLines[i] === bLines[j]) { ops.push({ type: "eq", text: aLines[i] }); i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { ops.push({ type: "del", text: aLines[i] }); i++; }
    else { ops.push({ type: "add", text: bLines[j] }); j++; }
  }
  while (i < n) ops.push({ type: "del", text: aLines[i++] });
  while (j < m) ops.push({ type: "add", text: bLines[j++] });

  // Render hunks with context
  const out = [];
  let k = 0;
  while (k < ops.length) {
    if (ops[k].type === "eq") { k++; continue; }
    // Hunk start: include up to contextLines of preceding eqs
    let start = k;
    while (start > 0 && k - start < contextLines && ops[start - 1].type === "eq") start--;
    let end = k;
    while (end < ops.length && (ops[end].type !== "eq" || (() => {
      // peek ahead — keep extending while there's a non-eq within contextLines * 2
      let look = end + 1;
      let eqRun = ops[end].type === "eq" ? 1 : 0;
      while (look < ops.length && eqRun < contextLines * 2) {
        if (ops[look].type === "eq") eqRun++;
        else { eqRun = 0; }
        if (ops[look].type !== "eq") return true;
        look++;
      }
      return false;
    })())) end++;
    // Trim trailing eqs in hunk to contextLines
    let hunkEnd = end;
    let trailingEq = 0;
    for (let p = end - 1; p >= start && ops[p].type === "eq"; p--) trailingEq++;
    hunkEnd = end - Math.max(0, trailingEq - contextLines);
    out.push(`@@ hunk @@`);
    for (let p = start; p < hunkEnd; p++) {
      const op = ops[p];
      const sigil = op.type === "eq" ? " " : op.type === "del" ? "-" : "+";
      out.push(`${sigil}${op.text}`);
    }
    k = hunkEnd;
  }
  return out.join("\n");
}

function summarizeDiff(diff, maxLines = 60) {
  const lines = diff.split("\n");
  if (lines.length <= maxLines) return diff;
  return [
    ...lines.slice(0, maxLines),
    `... (${lines.length - maxLines} more diff lines suppressed)`,
  ].join("\n");
}

// ---------- main ----------

const args = process.argv.slice(2);
const flags = new Set(args.filter((a) => a.startsWith("--")));
const showDiffs = !flags.has("--no-diff");
const maxShown = (() => {
  const f = args.find((a) => a.startsWith("--show="));
  return f ? Number.parseInt(f.split("=")[1], 10) : 5;
})();
const writeArtifacts = flags.has("--write");
const writeDir = (() => {
  const f = args.find((a) => a.startsWith("--write-dir="));
  return f ? f.split("=")[1] : ".diff-artifacts";
})();
const inputs = args.filter((a) => !a.startsWith("--"));
if (inputs.length === 0) {
  console.error("Usage: bun scripts/diff-against-babel.mjs <file-or-dir> [...]");
  console.error("Flags: --no-diff, --show=N (default 5), --write [--write-dir=DIR]");
  process.exit(2);
}

const files = [];
for (const input of inputs) {
  files.push(...(await collectFiles(resolve(input))));
}
files.sort();

let okCount = 0;
let diffCount = 0;
let errorCount = 0;
let shownDiffs = 0;
const diffEntries = [];

for (const file of files) {
  const source = await fs.readFile(file, "utf8");
  let babelOut, oxcOut;
  try {
    babelOut = transformBabel(source, file);
  } catch (e) {
    errorCount++;
    console.log(`ERROR (babel) ${file}: ${e.message}`);
    continue;
  }
  try {
    oxcOut = transformOxc(source, file);
  } catch (e) {
    errorCount++;
    console.log(`ERROR (oxc)   ${file}: ${e.message}`);
    continue;
  }

  const nb = normalize(babelOut);
  const no = normalize(oxcOut);

  if (writeArtifacts) {
    const rel = relative(process.cwd(), file).replace(/[/\\]/g, "__");
    const baseOut = join(writeDir, rel);
    await fs.mkdir(dirname(baseOut), { recursive: true });
    await fs.writeFile(`${baseOut}.babel.js`, babelOut);
    await fs.writeFile(`${baseOut}.oxc.js`, oxcOut);
  }

  if (nb === no) {
    okCount++;
  } else {
    diffCount++;
    const entry = { file, babelOut, oxcOut, normalizedBabel: nb, normalizedOxc: no };
    diffEntries.push(entry);
  }
}

// Sort diffs by smallest size first (easiest to fix)
diffEntries.sort((a, b) => {
  const an = a.normalizedBabel.length + a.normalizedOxc.length;
  const bn = b.normalizedBabel.length + b.normalizedOxc.length;
  return an - bn;
});

if (showDiffs) {
  for (const entry of diffEntries.slice(0, maxShown)) {
    console.log("\n=========================================================");
    console.log(`DIFF ${entry.file}`);
    console.log(`(babel: ${entry.normalizedBabel.length} ch, oxc: ${entry.normalizedOxc.length} ch normalized)`);
    console.log("=========================================================");
    const d = unifiedDiff(entry.normalizedBabel, entry.normalizedOxc);
    console.log(summarizeDiff(d));
    shownDiffs++;
  }
  if (diffEntries.length > maxShown) {
    console.log(`\n(${diffEntries.length - maxShown} additional diffing files not shown; pass --show=N to widen)`);
  }
}

console.log("");
console.log(`Total:  ${files.length}`);
console.log(`OK:     ${okCount}`);
console.log(`DIFF:   ${diffCount}`);
console.log(`ERROR:  ${errorCount}`);
if (diffEntries.length > 0 && !showDiffs) {
  console.log("\nDiffing files:");
  for (const e of diffEntries) console.log(`  ${e.file}`);
}

process.exit(diffCount === 0 && errorCount === 0 ? 0 : 1);
