#!/usr/bin/env bun
/**
 * Compile a JSX/TSX snippet (or whole file) with both Babel and OXC, render
 * the resulting component to HTML via solid-js/web's renderToStringAsync, and
 * diff the two HTML strings.
 *
 * This is the *real* parity test: the JS shape can differ between
 * implementations as long as the rendered HTML is byte-identical, since the
 * client hydrator only ever sees the HTML.
 *
 * Usage:
 *   bun scripts/diff-html.mjs <fixture-name>
 *   bun scripts/diff-html.mjs --all
 *   bun scripts/diff-html.mjs --fixture-dir scripts/fixtures
 *
 * A fixture is a .tsx file under scripts/fixtures/ that has a `default export`
 * which is a SolidJS component (zero-arg or accepting no props).
 */

import { promises as fs } from "node:fs";
import { existsSync } from "node:fs";
import { dirname, join, basename, resolve, extname, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const GOTHAB_FRONTEND = resolve(ROOT, "../../frontend");
const FIXTURE_DIR = join(__dirname, "fixtures");

// ---------- imports ----------
const babel = await import("@babel/core");
const babelPresetTs = (await import("@babel/preset-typescript")).default;
const jsxDomExpressionsModule = await import(
  join(ROOT, "packages/babel-plugin-jsx-dom-expressions/index.js")
);
const jsxDomExpressions = jsxDomExpressionsModule.default ?? jsxDomExpressionsModule;
const oxcModule = await import(join(ROOT, "packages/solid-jsx-oxc/index.js"));

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

function transformBabel(source, filename) {
  const result = babel.transformSync(source, {
    filename,
    babelrc: false,
    configFile: false,
    presets: [[babelPresetTs, { isTSX: true, allExtensions: true, allowDeclareFields: true }]],
    plugins: [[jsxDomExpressions, SHARED_OPTIONS]],
    sourceMaps: false,
  });
  return result?.code ?? "";
}

function stripTypes(source, filename) {
  // Mirrors what Vite's esbuild loader does on .tsx files before the OXC
  // plugin sees them: strip TypeScript type annotations, leave JSX intact.
  const result = babel.transformSync(source, {
    filename,
    babelrc: false,
    configFile: false,
    presets: [[babelPresetTs, { isTSX: true, allExtensions: true, allowDeclareFields: true, onlyRemoveTypeImports: false }]],
    sourceMaps: false,
  });
  return result?.code ?? "";
}

function transformOxc(source, filename) {
  // OXC's transformJsx doesn't strip TS types itself; in Vite, esbuild
  // strips them upstream. Replicate that here so the compiled module is
  // valid JavaScript.
  const stripped = stripTypes(source, filename);
  const result = oxcModule.transform(stripped, { ...SHARED_OPTIONS, filename });
  return result?.code ?? "";
}

/**
 * Compile a snippet, write the compiled JS into a temp file inside Gothab's
 * frontend tree (so that `solid-js/web` resolves correctly), and dynamically
 * import it. Returns the module's default export.
 */
async function compileAndImport(source, filename, transformer, label) {
  const compiled = transformer(source, filename);
  const tmpRoot = join(GOTHAB_FRONTEND, ".diff-html-tmp");
  await fs.mkdir(tmpRoot, { recursive: true });
  const outPath = join(
    tmpRoot,
    `${basename(filename, extname(filename))}.${label}.${Date.now()}-${Math.random()
      .toString(36)
      .slice(2)}.mjs`,
  );
  await fs.writeFile(outPath, compiled);
  try {
    // Bun caches modules by URL; appending a query string forces a fresh load.
    const mod = await import(`${outPath}?cb=${Date.now()}`);
    return { mod, compiled, outPath };
  } catch (e) {
    return { error: e, compiled, outPath };
  }
}

async function renderViaSolid(component) {
  // Resolve solid-js/web from gothab frontend
  const solidWeb = await import(
    join(GOTHAB_FRONTEND, "node_modules/solid-js/web/dist/server.js")
  );
  const html = await solidWeb.renderToStringAsync(() => component(), { renderId: "" });
  return html;
}

function unifiedDiff(a, b, contextLines = 2) {
  const aLines = a.split("\n");
  const bLines = b.split("\n");
  const n = aLines.length, m = bLines.length;
  if (n > 4000 || m > 4000) return `(diff suppressed: ${n} vs ${m} lines)`;
  const dp = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--)
    for (let j = m - 1; j >= 0; j--)
      dp[i][j] = aLines[i] === bLines[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
  const ops = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (aLines[i] === bLines[j]) ops.push({ t: "eq", v: aLines[i++] }), j++;
    else if (dp[i + 1][j] >= dp[i][j + 1]) ops.push({ t: "del", v: aLines[i++] });
    else ops.push({ t: "add", v: bLines[j++] });
  }
  while (i < n) ops.push({ t: "del", v: aLines[i++] });
  while (j < m) ops.push({ t: "add", v: bLines[j++] });
  const out = [];
  for (let k = 0; k < ops.length; k++) {
    if (ops[k].t === "eq") continue;
    let s = Math.max(0, k - contextLines);
    let e = k;
    while (e < ops.length && (ops[e].t !== "eq" || ops.slice(e, e + contextLines + 1).some((o) => o.t !== "eq"))) e++;
    e = Math.min(ops.length, e + contextLines);
    out.push("@@");
    for (let p = s; p < e; p++) {
      const sigil = ops[p].t === "eq" ? " " : ops[p].t === "del" ? "-" : "+";
      out.push(sigil + ops[p].v);
    }
    k = e;
  }
  return out.join("\n");
}

/**
 * Normalize cosmetic-but-irrelevant differences in SSR HTML so the diff
 * surfaces only substantive divergences:
 *
 *   1. Sort the attributes inside every opening tag alphabetically. The DOM
 *      hydrator does not care about attribute order; Babel and OXC merge
 *      spread/static attributes in different orders.
 *
 *   2. Collapse internal runs of whitespace inside `class="..."` values and
 *      trim leading/trailing whitespace. Babel and OXC's `classList` /
 *      `class` mergers leave different trailing-space artifacts; the
 *      browser tokenizer treats `"a b "` and `"a b"` identically.
 *
 *   3. Collapse internal runs of whitespace inside `style="..."` values
 *      (same rationale — browsers tokenize on `;` and ignore extra spaces).
 *
 * Everything else (text content, marker comments, void-element forms,
 * data-* attribute values, the *presence* of attributes) is left untouched
 * so genuine bugs surface.
 */
function normalizeHtml(html) {
  // Match an opening tag, including self-closing: <tag ...> or <tag .../>.
  // Captures the tag name and the attribute string (which may be empty).
  // Skips comments (`<!-- ... -->`) and processing instructions, which start
  // with `<!` or `<?` — we filter those by requiring a name char after `<`.
  return html.replace(/<([a-zA-Z][a-zA-Z0-9-]*)((?:\s+[^>]*?)?)(\s*\/?)>/g, (_match, name, attrs, trail) => {
    const pairs = [];
    // Tokenize attributes: name, name=value, name="value", name='value'.
    // The codegens never emit attribute names containing whitespace or `=`,
    // and values are always quoted in this output. Bare-value form (no
    // quotes) doesn't appear. Boolean attrs render as bare `name`.
    const attrRe = /\s+([a-zA-Z_:][\w:.\-]*)(?:=("[^"]*"|'[^']*'))?/g;
    let m;
    while ((m = attrRe.exec(attrs)) !== null) {
      let [_, key, value] = m;
      if (value !== undefined && key === "class") {
        // Inside the surrounding quotes, collapse whitespace runs and trim.
        const quote = value[0];
        const inner = value.slice(1, -1).replace(/\s+/g, " ").trim();
        value = `${quote}${inner}${quote}`;
      }
      if (value !== undefined && key === "style") {
        const quote = value[0];
        const inner = value
          .slice(1, -1)
          .replace(/\s*;\s*/g, ";")
          .replace(/^\s+|\s+$/g, "")
          .replace(/;+$/, "");
        value = `${quote}${inner}${quote}`;
      }
      pairs.push([key, value]);
    }
    pairs.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
    const rendered = pairs.map(([k, v]) => (v === undefined ? ` ${k}` : ` ${k}=${v}`)).join("");
    // Drop any trailing whitespace before `>` / `/>` (codegens differ on
    // whether the spread renderer leaves a stray space after the last attr).
    const trailNorm = trail.replace(/^\s+/, "").length === 0 ? "" : trail.trim();
    return `<${name}${rendered}${trailNorm}>`;
  });
}

/**
 * Format HTML across multiple lines so a one-character diff doesn't show as
 * one giant line. Pure formatting, no normalization.
 */
function formatHtml(html) {
  return html
    .replace(/></g, ">\n<")
    .replace(/(<!--\$-->|<!--\/-->)/g, "$1\n")
    .replace(/\n+/g, "\n")
    .trim();
}

/**
 * Pretty-print + normalize for equality-style diff display.
 */
function prettyHtml(html) {
  return formatHtml(noNormalize ? html : normalizeHtml(html));
}

// ---------- runners ----------

async function runFixture(fixturePath) {
  const source = await fs.readFile(fixturePath, "utf8");
  const filename = fixturePath;

  const babelImport = await compileAndImport(source, filename, transformBabel, "babel");
  if (babelImport.error) {
    return {
      file: fixturePath,
      ok: false,
      reason: `babel-import failed: ${babelImport.error.message}`,
      babelCompiled: babelImport.compiled,
    };
  }
  const oxcImport = await compileAndImport(source, filename, transformOxc, "oxc");
  if (oxcImport.error) {
    return {
      file: fixturePath,
      ok: false,
      reason: `oxc-import failed: ${oxcImport.error.message}`,
      oxcCompiled: oxcImport.compiled,
    };
  }

  const babelComp = babelImport.mod.default;
  const oxcComp = oxcImport.mod.default;
  if (typeof babelComp !== "function" || typeof oxcComp !== "function") {
    return {
      file: fixturePath,
      ok: false,
      reason: "fixture must export a default function component",
    };
  }

  let babelHtml, oxcHtml;
  try {
    babelHtml = await renderViaSolid(babelComp);
  } catch (e) {
    return {
      file: fixturePath,
      ok: false,
      reason: `babel-render failed: ${e.message}`,
      babelCompiled: babelImport.compiled,
    };
  }
  try {
    oxcHtml = await renderViaSolid(oxcComp);
  } catch (e) {
    return {
      file: fixturePath,
      ok: false,
      reason: `oxc-render failed: ${e.message}`,
      oxcCompiled: oxcImport.compiled,
    };
  }

  // Cleanup tmp files
  await fs.unlink(babelImport.outPath).catch(() => {});
  await fs.unlink(oxcImport.outPath).catch(() => {});

  // Compare on normalized HTML so attribute order / class whitespace /
  // style whitespace don't masquerade as bugs. The --no-normalize flag
  // disables this so we can audit whether normalization is hiding real
  // differences.
  const normalizedBabel = noNormalize ? babelHtml : normalizeHtml(babelHtml);
  const normalizedOxc = noNormalize ? oxcHtml : normalizeHtml(oxcHtml);

  return {
    file: fixturePath,
    ok: normalizedBabel === normalizedOxc,
    babelHtml,
    oxcHtml,
    normalizedBabel,
    normalizedOxc,
    babelCompiled: babelImport.compiled,
    oxcCompiled: oxcImport.compiled,
  };
}

async function gatherFixtures(dir) {
  const out = [];
  const entries = await fs.readdir(dir, { withFileTypes: true });
  for (const e of entries) {
    if (e.isDirectory()) out.push(...(await gatherFixtures(join(dir, e.name))));
    else if (/\.(tsx|jsx)$/.test(e.name)) out.push(join(dir, e.name));
  }
  return out.sort();
}

// ---------- main ----------

const args = process.argv.slice(2);
const flags = new Set(args.filter((a) => a.startsWith("--")));
const positional = args.filter((a) => !a.startsWith("--"));
const showCompiled = flags.has("--show-compiled");
const showHtml = flags.has("--show-html");
const dumpHtml = flags.has("--dump-html");
const noNormalize = flags.has("--no-normalize");
const fixtureDirArg = (() => {
  const f = args.find((a) => a.startsWith("--fixture-dir="));
  return f ? resolve(f.split("=")[1]) : FIXTURE_DIR;
})();

let fixtures = [];
if (flags.has("--all") || positional.length === 0) {
  if (!existsSync(fixtureDirArg)) {
    console.error(`No fixture directory: ${fixtureDirArg}`);
    process.exit(2);
  }
  fixtures = await gatherFixtures(fixtureDirArg);
} else {
  for (const p of positional) {
    const candidate = existsSync(p) ? resolve(p) : join(fixtureDirArg, p.endsWith(".tsx") ? p : `${p}.tsx`);
    if (!existsSync(candidate)) {
      console.error(`Fixture not found: ${p}`);
      process.exit(2);
    }
    fixtures.push(candidate);
  }
}

console.log(`Running ${fixtures.length} fixture(s) from ${fixtureDirArg}\n`);

let okCount = 0;
let diffCount = 0;
let errorCount = 0;
const results = [];

for (const fx of fixtures) {
  const result = await runFixture(fx);
  results.push(result);
  const rel = relative(process.cwd(), fx);
  if (result.ok === true) {
    console.log(`OK    ${rel}`);
    okCount++;
  } else if (result.reason && !result.babelHtml) {
    console.log(`ERROR ${rel}: ${result.reason}`);
    errorCount++;
  } else {
    console.log(`DIFF  ${rel}`);
    diffCount++;
  }
}

// --dump-html: print full formatted HTML for both compilers and write to
// scripts/.dump/<basename>.{babel,oxc}.html so Bart can open them
// side-by-side in an editor / vscode diff view.
if (dumpHtml) {
  const dumpDir = join(__dirname, ".dump");
  await fs.mkdir(dumpDir, { recursive: true });
  for (const r of results) {
    if (!r.babelHtml || !r.oxcHtml) continue;
    const base = basename(r.file, extname(r.file));
    const babelPath = join(dumpDir, `${base}.babel.html`);
    const oxcPath = join(dumpDir, `${base}.oxc.html`);
    await fs.writeFile(babelPath, formatHtml(r.babelHtml) + "\n");
    await fs.writeFile(oxcPath, formatHtml(r.oxcHtml) + "\n");
    console.log("");
    console.log("─".repeat(60));
    console.log(`DUMP: ${relative(process.cwd(), r.file)}`);
    console.log(`  Babel HTML → ${relative(process.cwd(), babelPath)} (${r.babelHtml.length} ch)`);
    console.log(`  OXC   HTML → ${relative(process.cwd(), oxcPath)} (${r.oxcHtml.length} ch)`);
    console.log("─".repeat(60));
    console.log("\n--- BABEL (formatted, raw, no normalization) ---");
    console.log(formatHtml(r.babelHtml));
    console.log("\n--- OXC (formatted, raw, no normalization) ---");
    console.log(formatHtml(r.oxcHtml));
    console.log("\n--- DIFF (formatted, raw, no normalization) ---");
    console.log(unifiedDiff(formatHtml(r.babelHtml), formatHtml(r.oxcHtml), 3));
  }
}

// Detail diffs (smallest first)
const diffs = results.filter((r) => r.ok === false && r.babelHtml && r.oxcHtml);
diffs.sort((a, b) => a.babelHtml.length + a.oxcHtml.length - (b.babelHtml.length + b.oxcHtml.length));

for (const r of diffs) {
  console.log("\n=========================================================");
  console.log(`HTML DIFF: ${relative(process.cwd(), r.file)}`);
  console.log(`(babel: ${r.babelHtml.length} ch, oxc: ${r.oxcHtml.length} ch)`);
  console.log("=========================================================");
  const pb = prettyHtml(r.babelHtml);
  const po = prettyHtml(r.oxcHtml);
  console.log(unifiedDiff(pb, po));
  if (showHtml) {
    console.log("\n--- BABEL HTML (raw) ---");
    console.log(r.babelHtml);
    console.log("\n--- OXC HTML (raw) ---");
    console.log(r.oxcHtml);
  }
  if (showCompiled) {
    console.log("\n--- BABEL compiled ---");
    console.log(r.babelCompiled);
    console.log("\n--- OXC compiled ---");
    console.log(r.oxcCompiled);
  }
}

// Surface ERRORs with their compiled output (helps debug)
for (const r of results.filter((r) => r.reason && !r.babelHtml)) {
  console.log("\n---------------------------------------------------------");
  console.log(`ERROR DETAIL: ${relative(process.cwd(), r.file)}`);
  console.log(r.reason);
  if (showCompiled) {
    if (r.babelCompiled) {
      console.log("\n--- BABEL compiled ---");
      console.log(r.babelCompiled);
    }
    if (r.oxcCompiled) {
      console.log("\n--- OXC compiled ---");
      console.log(r.oxcCompiled);
    }
  }
}

console.log("");
console.log(`OK:    ${okCount}`);
console.log(`DIFF:  ${diffCount}`);
console.log(`ERROR: ${errorCount}`);

process.exit(diffCount === 0 && errorCount === 0 ? 0 : 1);
