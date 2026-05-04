#!/usr/bin/env bun

import { $ } from "bun";
import { parseArgs } from "util";
import { dirname, join, basename } from "node:path";
import { stat, copyFile } from "node:fs/promises";
import { stdin as input, stdout as output } from "node:process";
import { createInterface } from "node:readline/promises";

type PackageJson = {
  name: string;
  version: string;
  private?: boolean;
  scripts?: Record<string, string>;
  files?: string[];
  dependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
};

type WorkspacePackage = {
  name: string;
  version: string;
  dir: string;
  manifest: PackageJson;
  internalDeps: string[];
  /**
   * If this is a NAPI-RS per-platform sub-package living under
   * <parentDir>/npm/<target>/, this holds the parent package's name (e.g.
   * "solid-jsx-oxc"). Sub-packages have no source of their own — their .node
   * file is copied in from the parent package directory before publish.
   */
  subPackageOf?: string;
  /** For sub-packages: the NAPI target id, e.g. "linux-arm64-gnu". */
  napiTarget?: string;
};

type RescopeConfig = {
  scope: string;            // e.g. "@aeolun"
  packageNames: Set<string>; // unscoped names that should be rewritten
};

type Publisher = "bun" | "npm";

type Options = {
  tag: string;
  publish: boolean;
  yes: boolean;
  dryRun: boolean;
  list: boolean;
  skipBuild: boolean;
  tolerateRepublish: boolean;
  allowDirty: boolean;
  runScripts: boolean;
  registry?: string;
  access?: string;
  otp?: string;
  publisher: Publisher;
  provenance: boolean;
  only: Set<string>;
  exclude: Set<string>;
  rescope?: string;
  help: boolean;
};

class UserError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "UserError";
  }
}

function printHelp() {
  console.log(
    `
solid-jsx-oxc publish (monorepo)

Usage:
  bun publish-alpha.ts [options]

Behavior:
  - Default mode is dry-run (prints + runs build, but does not publish)
  - Use --publish to actually publish (asks for confirmation)
  - Use --yes to publish without confirmation

Options:
  --list                     Print publish order and exit
  --tag <name>               Dist-tag to publish under (default: alpha)
  --only <pkg>               Only publish a package (repeatable, includes internal deps)
  --exclude <pkg>            Exclude a package (repeatable)

Publish:
  --publish                  Actually publish to the registry
  --yes                      Skip confirmation prompt (implies --publish)
  --dry-run                  Force dry run (overrides --publish/--yes)
  --tolerate-republish        Do not fail if version already exists
  --allow-dirty              Allow uncommitted git changes (publish only)

Build:
  --skip-build               Skip running each package's build script

Registry:
  --registry <url>           Override registry URL
  --access <public|...>      Publish access level (mostly for scoped packages)
  --otp <code>               One-time password for 2FA
  --run-scripts              Allow lifecycle scripts during publish
  --publisher <bun|npm>      Which client to use for the publish call (default: bun).
                             Use "npm" for npm trusted-publishing (OIDC) in CI —
                             bun publish does not yet support OIDC.
  --provenance               Emit a provenance attestation. Implies --publisher npm
                             (npm only). Trusted publishing already implies
                             provenance, so this flag is mostly redundant in CI.

Rescope (publish under your own npm scope without committing the rename):
  --rescope <scope>          Rewrite name + inter-package dep refs to <scope>/<name>
                             just before publish, restore after. <scope> must
                             appear as a key under "rescope" in root package.json,
                             e.g.:
                               "rescope": { "@aeolun": ["solid-jsx-oxc", ...] }
                             Implies --only-the-listed-packages: only those names
                             are considered as publish candidates.

Other:
  -h, --help                 Show help

Examples:
  bun publish-alpha.ts --list
  bun publish-alpha.ts
  bun publish-alpha.ts --only solid-jsx-oxc
  bun publish-alpha.ts --exclude babel-plugin-jsx-dom-expressions
  bun publish-alpha.ts --tag alpha --publish
  bun publish-alpha.ts --tag next --yes --otp 123456
  bun publish-alpha.ts --rescope @aeolun --publish --access public
`.trimStart(),
  );
}

function stringArray(value: unknown): string[] {
  if (!value) return [];
  return Array.isArray(value) ? value : [String(value)];
}

function getInternalDeps(manifest: PackageJson, internalNames: Set<string>): string[] {
  const deps = {
    ...(manifest.dependencies ?? {}),
    ...(manifest.optionalDependencies ?? {}),
    ...(manifest.peerDependencies ?? {}),
  };

  return Object.keys(deps).filter((name) => internalNames.has(name));
}

function topoSort(packages: WorkspacePackage[]): WorkspacePackage[] {
  const byName = new Map<string, WorkspacePackage>();
  for (const pkg of packages) byName.set(pkg.name, pkg);

  const names = Array.from(byName.keys());

  const outgoing = new Map<string, Set<string>>();
  const indegree = new Map<string, number>();

  for (const name of names) {
    outgoing.set(name, new Set());
    indegree.set(name, 0);
  }

  for (const pkg of packages) {
    for (const dep of pkg.internalDeps) {
      const outs = outgoing.get(dep);
      if (!outs) continue;
      if (outs.has(pkg.name)) continue;
      outs.add(pkg.name);
      indegree.set(pkg.name, (indegree.get(pkg.name) ?? 0) + 1);
    }
  }

  const priority = new Map<string, number>([["solid-jsx-oxc", 0]]);
  const sortNames = (a: string, b: string) => {
    const pa = priority.get(a) ?? 1_000_000;
    const pb = priority.get(b) ?? 1_000_000;
    if (pa !== pb) return pa - pb;
    return a.localeCompare(b);
  };

  const queue = names.filter((n) => (indegree.get(n) ?? 0) === 0).sort(sortNames);
  const ordered: WorkspacePackage[] = [];

  while (queue.length > 0) {
    const name = queue.shift()!;
    ordered.push(byName.get(name)!);

    for (const next of outgoing.get(name) ?? []) {
      const nextDeg = (indegree.get(next) ?? 0) - 1;
      indegree.set(next, nextDeg);
      if (nextDeg === 0) {
        queue.push(next);
        queue.sort(sortNames);
      }
    }
  }

  if (ordered.length !== packages.length) {
    return packages.slice().sort((a, b) => sortNames(a.name, b.name));
  }

  return ordered;
}

async function ensureCleanGit(repoRoot: string) {
  const status = (await $`git status --porcelain`.cwd(repoRoot).text()).trim();
  if (status.length > 0) {
    throw new UserError("Working tree is not clean. Commit/stash first or pass --allow-dirty.");
  }
}

type WorkspacePatterns = {
  include: string[];
  exclude: string[];
};

function normalizeWorkspacePattern(pattern: string): string {
  return pattern.replace(/\\/g, "/").replace(/\/$/, "");
}

function toPackageJsonGlob(pattern: string): string {
  return pattern.endsWith("package.json") ? pattern : `${pattern}/package.json`;
}

async function getWorkspacePackageJsonPatterns(repoRoot: string): Promise<WorkspacePatterns> {
  const rootManifestPath = join(repoRoot, "package.json");
  const rootManifest = (await Bun.file(rootManifestPath).json()) as { workspaces?: unknown };

  const rawWorkspaces = rootManifest?.workspaces;

  let patterns: string[];

  if (Array.isArray(rawWorkspaces)) {
    patterns = rawWorkspaces.map(String);
  } else if (
    rawWorkspaces &&
    typeof rawWorkspaces === "object" &&
    Array.isArray((rawWorkspaces as { packages?: unknown }).packages)
  ) {
    patterns = (rawWorkspaces as { packages: unknown[] }).packages.map(String);
  } else {
    patterns = ["packages/*"];
  }

  const include: string[] = [];
  const exclude: string[] = [];

  for (const raw of patterns) {
    const normalized = normalizeWorkspacePattern(String(raw));
    if (!normalized) continue;

    if (normalized.startsWith("!")) {
      const withoutBang = normalized.slice(1);
      if (withoutBang) exclude.push(toPackageJsonGlob(withoutBang));
      continue;
    }

    include.push(toPackageJsonGlob(normalized));
  }

  return {
    include: include.length > 0 ? include : ["packages/*/package.json"],
    exclude,
  };
}

async function readWorkspacePackages(repoRoot: string): Promise<WorkspacePackage[]> {
  const patterns = await getWorkspacePackageJsonPatterns(repoRoot);

  const relPaths = new Set<string>();
  for (const pattern of patterns.include) {
    const glob = new Bun.Glob(pattern);
    for await (const relPath of glob.scan({ cwd: repoRoot, onlyFiles: true })) {
      relPaths.add(relPath);
    }
  }

  if (patterns.exclude.length > 0) {
    for (const pattern of patterns.exclude) {
      const glob = new Bun.Glob(pattern);
      for await (const relPath of glob.scan({ cwd: repoRoot, onlyFiles: true })) {
        relPaths.delete(relPath);
      }
    }
  }

  const found: WorkspacePackage[] = [];

  for (const relPath of Array.from(relPaths).sort()) {
    const absPath = join(repoRoot, relPath);
    const manifest = (await Bun.file(absPath).json()) as PackageJson;

    if (!manifest || typeof manifest !== "object") continue;
    if (manifest.private) continue;

    if (!manifest.name || !manifest.version) {
      throw new UserError(`Invalid package.json: ${relPath}`);
    }

    found.push({
      name: manifest.name,
      version: manifest.version,
      dir: dirname(absPath),
      manifest,
      internalDeps: [],
    });
  }

  // Discover NAPI-RS per-platform sub-packages: `<workspace-pkg>/npm/<target>/package.json`.
  // These are generated by `napi create-npm-dirs` and aren't covered by the
  // workspace globs (we don't want them treated as workspace members for
  // bun/pnpm purposes — just for publish).
  const subPackages: WorkspacePackage[] = [];
  for (const parent of found) {
    const npmDir = join(parent.dir, "npm");
    const npmStat = await stat(npmDir).catch(() => null);
    if (!npmStat?.isDirectory()) continue;

    const subGlob = new Bun.Glob("*/package.json");
    for await (const subRel of subGlob.scan({ cwd: npmDir, onlyFiles: true })) {
      const subAbs = join(npmDir, subRel);
      const subManifest = (await Bun.file(subAbs).json()) as PackageJson;
      if (!subManifest || typeof subManifest !== "object") continue;
      if (subManifest.private) continue;
      if (!subManifest.name || !subManifest.version) {
        throw new UserError(`Invalid sub-package package.json: ${subAbs}`);
      }

      const target = basename(dirname(subAbs)); // e.g. "linux-arm64-gnu"
      subPackages.push({
        name: subManifest.name,
        version: subManifest.version,
        dir: dirname(subAbs),
        manifest: subManifest,
        internalDeps: [],
        subPackageOf: parent.name,
        napiTarget: target,
      });
    }
  }

  const all = [...found, ...subPackages];

  const internalNames = new Set(all.map((p) => p.name));
  for (const pkg of all) {
    pkg.internalDeps = getInternalDeps(pkg.manifest, internalNames);
  }

  return topoSort(all);
}

function validateTag(tag: string): void {
  if (!tag || tag.includes(" ") || tag.includes("\n")) {
    throw new UserError(`Invalid tag: "${tag}". Tag must be non-empty and contain no whitespace.`);
  }
}

async function loadRescopeConfig(repoRoot: string, scope: string): Promise<RescopeConfig> {
  if (!scope.startsWith("@") || !scope.slice(1).length || scope.includes("/")) {
    throw new UserError(`Invalid --rescope value: "${scope}". Expected an npm scope like "@aeolun".`);
  }

  const rootManifest = (await Bun.file(join(repoRoot, "package.json")).json()) as {
    rescope?: Record<string, unknown>;
  };

  const config = rootManifest?.rescope?.[scope];
  if (!config || !Array.isArray(config) || config.length === 0) {
    throw new UserError(
      `No rescope config found for "${scope}". Add a top-level "rescope" object to package.json:\n` +
        `  "rescope": { "${scope}": ["solid-jsx-oxc", "vite-plugin-solid-oxc", ...] }`,
    );
  }

  for (const entry of config) {
    if (typeof entry !== "string" || !entry) {
      throw new UserError(`Invalid rescope entry under "${scope}": ${JSON.stringify(entry)}. Expected unscoped package names.`);
    }
  }

  return {
    scope,
    packageNames: new Set(config as string[]),
  };
}

/**
 * Augment a rescope config with the names of any NAPI-RS sub-packages whose
 * parent workspace package is being rescoped. This means the user only has to
 * list the top-level packages in their root `rescope` config — the per-platform
 * sub-packages auto-rename to match (e.g. listing `solid-jsx-oxc` causes
 * `solid-jsx-oxc-linux-arm64-gnu` to also be rewritten to
 * `@aeolun/solid-jsx-oxc-linux-arm64-gnu`).
 */
function extendRescopeWithSubPackages(config: RescopeConfig, packages: WorkspacePackage[]): void {
  for (const pkg of packages) {
    if (!pkg.subPackageOf) continue;
    if (!config.packageNames.has(pkg.subPackageOf)) continue;
    config.packageNames.add(pkg.name);
  }
}

async function collectPublishedSourceFiles(pkg: WorkspacePackage): Promise<string[]> {
  // Files we may need to rewrite are those that ship in the npm tarball AND contain
  // textual import specifiers (JS/TS/declaration files). The `files` field in
  // package.json is the source of truth; if it's missing, we conservatively scan
  // the package root.
  const result: string[] = [];
  const entries = pkg.manifest.files ?? ["."];
  const textExtRe = /\.(?:m?jsx?|cjs|tsx?|d\.ts)$/;

  for (const rawEntry of entries) {
    const entry = rawEntry.replace(/\\/g, "/");
    const fullPath = join(pkg.dir, entry);

    // Resolve the entry. node:fs/promises#stat handles both files and
    // directories (Bun.file().exists() returns false for directories, which
    // is what tripped this up before — only package.json got rewritten,
    // dist/ and src/ contents were quietly skipped).
    const stats = await stat(fullPath).catch(() => null);

    if (stats?.isDirectory()) {
      const glob = new Bun.Glob("**/*.{js,mjs,cjs,jsx,ts,tsx,d.ts}");
      for await (const rel of glob.scan({ cwd: fullPath, onlyFiles: true })) {
        result.push(join(fullPath, rel));
      }
      continue;
    }

    if (stats?.isFile()) {
      if (textExtRe.test(entry)) result.push(fullPath);
      continue;
    }

    // Entry didn't resolve to a concrete path — treat as glob (e.g. "*.node",
    // though that won't match textual extensions and will be filtered out).
    const glob = new Bun.Glob(entry);
    for await (const rel of glob.scan({ cwd: pkg.dir, onlyFiles: true })) {
      if (textExtRe.test(rel)) result.push(join(pkg.dir, rel));
    }
  }

  // Deduplicate
  return Array.from(new Set(result));
}

function buildSpecifierRegexes(packageNames: Iterable<string>): { name: string; re: RegExp }[] {
  return Array.from(packageNames).map((name) => {
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    // Match a quoted module specifier: "name" or 'name' or "name/subpath" or 'name/subpath'.
    // Leaves the closing quote and any subpath intact via capture groups.
    return { name, re: new RegExp(`(['"\`])${escaped}((?:/[^'"\`]*)?)\\1`, "g") };
  });
}

type RescopeSnapshot = Map<string, string>; // absolute file path → original content

/**
 * Translate a `workspace:` version specifier into a concrete one.
 * Once we rescope, bun publish can no longer follow `workspace:*` because the
 * workspace package's name has changed but the lockfile still records the old
 * name. We resolve the protocol here so the published manifest carries a real
 * version range — the same thing pnpm publish does automatically.
 */
function resolveWorkspaceVersion(spec: string, targetVersion: string): string {
  if (!spec.startsWith("workspace:")) return spec;
  const range = spec.slice("workspace:".length);
  if (range === "" || range === "*") return targetVersion;
  if (range === "^") return `^${targetVersion}`;
  if (range === "~") return `~${targetVersion}`;
  // workspace:<explicit-spec> — use the explicit spec verbatim
  return range;
}

async function applyRescopeForPackage(
  pkg: WorkspacePackage,
  config: RescopeConfig,
  workspaceVersions: Map<string, string>,
): Promise<RescopeSnapshot> {
  const snapshot: RescopeSnapshot = new Map();

  // 1) Rewrite package.json: name + inter-package dep refs.
  const pkgJsonPath = join(pkg.dir, "package.json");
  const originalPkgJson = await Bun.file(pkgJsonPath).text();
  snapshot.set(pkgJsonPath, originalPkgJson);

  const manifest = JSON.parse(originalPkgJson) as PackageJson;
  let manifestChanged = false;

  if (config.packageNames.has(manifest.name)) {
    manifest.name = `${config.scope}/${manifest.name}`;
    manifestChanged = true;
  }

  for (const depKey of ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] as const) {
    const deps = (manifest as unknown as Record<string, Record<string, string> | undefined>)[depKey];
    if (!deps) continue;

    for (const name of Object.keys(deps)) {
      if (!config.packageNames.has(name)) continue;
      const originalSpec = deps[name]!;
      const targetVersion = workspaceVersions.get(name);
      if (!targetVersion) {
        throw new UserError(
          `Cannot resolve workspace version for "${name}" while rescoping "${pkg.name}". ` +
            `Is it actually in the workspace?`,
        );
      }
      const resolvedSpec = resolveWorkspaceVersion(originalSpec, targetVersion);
      delete deps[name];
      deps[`${config.scope}/${name}`] = resolvedSpec;
      manifestChanged = true;
    }
  }

  if (manifestChanged) {
    // Preserve trailing newline if the original had one.
    const trailing = originalPkgJson.endsWith("\n") ? "\n" : "";
    await Bun.write(pkgJsonPath, JSON.stringify(manifest, null, 2) + trailing);
  } else {
    snapshot.delete(pkgJsonPath);
  }

  // 2) Rewrite published source files: replace bare imports of rescoped packages.
  const regexes = buildSpecifierRegexes(config.packageNames);
  const filesToScan = await collectPublishedSourceFiles(pkg);

  for (const filepath of filesToScan) {
    const originalText = await Bun.file(filepath).text();
    let rewritten = originalText;

    for (const { name, re } of regexes) {
      rewritten = rewritten.replace(re, (_match, quote: string, subpath: string) => {
        return `${quote}${config.scope}/${name}${subpath}${quote}`;
      });
    }

    if (rewritten !== originalText) {
      snapshot.set(filepath, originalText);
      await Bun.write(filepath, rewritten);
    }
  }

  return snapshot;
}

/**
 * Check whether a specific name@version already exists in the registry.
 *
 * npm publish has no `--tolerate-republish` flag (that's a bun extension), so
 * to honor `--tolerate-republish` when --publisher=npm we pre-check with
 * `npm view`. Returns true if the version is already published.
 */
async function isVersionPublished(
  name: string,
  version: string,
  registry: string | undefined,
): Promise<boolean> {
  const args = ["npm", "view", `${name}@${version}`, "version"];
  if (registry) args.push(`--registry=${registry}`);

  const proc = Bun.spawn(args, {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, exitCode] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
  if (exitCode !== 0) return false; // 404 / E404 / network — treat as "not published"
  return stdout.trim() === version;
}

/**
 * For a per-platform sub-package, look up the matching `.node` binary in the
 * parent package directory and copy it into the sub-package directory so it
 * ships in the published tarball.
 *
 * Returns `true` if a binary was placed (publish should proceed), `false` if
 * none was found (publish should be skipped — common locally where you've only
 * built one target).
 */
async function placeSubPackageBinary(
  pkg: WorkspacePackage,
  packagesByName: Map<string, WorkspacePackage>,
): Promise<boolean> {
  if (!pkg.subPackageOf || !pkg.napiTarget) return false;

  const parent = packagesByName.get(pkg.subPackageOf);
  if (!parent) return false;

  // The binary name is dictated by NAPI-RS: <binaryName>.<target>.node.
  // We use the parent package's manifest binaryName if available, falling back
  // to the parent's package name.
  const napiCfg = (parent.manifest as { napi?: { binaryName?: string } }).napi;
  const binaryName = napiCfg?.binaryName ?? parent.name;
  const fileName = `${binaryName}.${pkg.napiTarget}.node`;

  const src = join(parent.dir, fileName);
  const dest = join(pkg.dir, fileName);

  const srcStat = await stat(src).catch(() => null);
  if (!srcStat?.isFile()) return false;

  // Skip if it's already placed (idempotent for repeat runs).
  const destStat = await stat(dest).catch(() => null);
  if (!destStat?.isFile() || destStat.size !== srcStat.size) {
    await copyFile(src, dest);
  }
  return true;
}

async function restoreRescopeSnapshots(snapshots: Iterable<RescopeSnapshot>): Promise<void> {
  // Restore in reverse insertion order so later overlays unwind first (defensive; sets are flat in practice).
  const all: [string, string][] = [];
  for (const snap of snapshots) {
    for (const entry of snap) all.push(entry);
  }

  // Best-effort: don't let a single failed restore stop the others.
  for (const [filepath, content] of all.reverse()) {
    try {
      await Bun.write(filepath, content);
    } catch (err) {
      console.error(`⚠️  Failed to restore ${filepath}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
}

function parseCli(): Options {
  const { values } = parseArgs({
    args: Bun.argv.slice(2),
    options: {
      tag: { type: "string", default: "alpha" },
      list: { type: "boolean", default: false },

      publish: { type: "boolean", default: false },
      yes: { type: "boolean", default: false },
      "dry-run": { type: "boolean", default: false },

      "skip-build": { type: "boolean", default: false },
      "tolerate-republish": { type: "boolean", default: false },
      "allow-dirty": { type: "boolean", default: false },
      "run-scripts": { type: "boolean", default: false },
      registry: { type: "string" },
      access: { type: "string" },
      otp: { type: "string" },
      publisher: { type: "string" },
      provenance: { type: "boolean", default: false },

      only: { type: "string", multiple: true },
      exclude: { type: "string", multiple: true },
      rescope: { type: "string" },
      help: { type: "boolean", short: "h", default: false },
    },
    allowPositionals: false,
  });

  const tag = String(values.tag ?? "alpha");
  validateTag(tag);

  const yes = Boolean(values.yes);
  const publish = Boolean(values.publish) || yes;
  const dryRun = Boolean(values["dry-run"]) || !publish;

  const provenance = Boolean(values.provenance);
  const publisherRaw = values.publisher ? String(values.publisher) : (provenance ? "npm" : "bun");
  if (publisherRaw !== "bun" && publisherRaw !== "npm") {
    throw new UserError(`--publisher must be "bun" or "npm", got "${publisherRaw}".`);
  }
  if (provenance && publisherRaw !== "npm") {
    throw new UserError(`--provenance requires --publisher npm (bun publish has no provenance support).`);
  }

  return {
    tag,
    publish,
    yes,
    dryRun,
    list: Boolean(values.list),
    skipBuild: Boolean(values["skip-build"]),
    tolerateRepublish: Boolean(values["tolerate-republish"]),
    allowDirty: Boolean(values["allow-dirty"]),
    runScripts: Boolean(values["run-scripts"]),
    registry: values.registry ? String(values.registry) : undefined,
    access: values.access ? String(values.access) : undefined,
    otp: values.otp ? String(values.otp) : undefined,
    publisher: publisherRaw as Publisher,
    provenance,
    only: new Set(stringArray(values.only)),
    exclude: new Set(stringArray(values.exclude)),
    rescope: values.rescope ? String(values.rescope) : undefined,
    help: Boolean(values.help),
  };
}

async function promptToProceed(options: Options, targets: WorkspacePackage[]): Promise<boolean> {
  if (options.dryRun) return true;
  if (options.yes) return true;

  const rl = createInterface({ input, output });
  try {
    const answer = await rl.question(
      `\n💬 About to publish ${targets.length} package(s) with tag "${options.tag}". Continue? (y/N) `,
    );
    return /^y(es)?$/i.test(answer.trim());
  } finally {
    rl.close();
  }
}

function printTargets(targets: WorkspacePackage[]) {
  for (const pkg of targets) {
    console.log(`  • ${pkg.name}@${pkg.version}`);
  }
}

function resolveTargets(
  packages: WorkspacePackage[],
  only: Set<string>,
  exclude: Set<string>,
): WorkspacePackage[] {
  let candidates = packages;

  // If --only is specified, only include those packages and their dependencies
  if (only.size > 0) {
    const byName = new Map(packages.map((p) => [p.name, p] as const));
    const resolved = new Set<string>();
    const queue = Array.from(only);

    while (queue.length > 0) {
      const name = queue.pop()!;
      if (resolved.has(name)) continue;
      resolved.add(name);

      const pkg = byName.get(name);
      if (!pkg) continue;

      for (const dep of pkg.internalDeps) {
        if (!resolved.has(dep)) queue.push(dep);
      }
    }

    candidates = packages.filter((p) => resolved.has(p.name));
  }

  // Filter out excluded packages
  return candidates.filter((p) => !exclude.has(p.name));
}

function generateHtmlReport(targets: WorkspacePackage[], options: Options, rescopeConfig: RescopeConfig | null): string {
  const registry = options.registry || "https://www.npmjs.com";
  const baseUrl = registry.replace(/\/$/, "");

  const packages = targets
    .map((pkg) => {
      const publishedName = rescopeConfig ? `${rescopeConfig.scope}/${pkg.name}` : pkg.name;
      return `
    <li class="package-item">
      <a href="${baseUrl}/package/${publishedName}" target="_blank" class="package-link">
        <span class="package-name">${publishedName}</span>
        <span class="package-version">@${pkg.version}</span>
      </a>
      <span class="tag-badge">${options.tag}</span>
    </li>
  `;
    })
    .join("");

  return `
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>solid-jsx-oxc Publish Results</title>
  <style>
    * {
      margin: 0;
      padding: 0;
      box-sizing: border-box;
    }

    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
      background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 20px;
    }

    .container {
      background: white;
      border-radius: 12px;
      box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
      max-width: 600px;
      width: 100%;
      padding: 40px;
    }

    h1 {
      color: #333;
      margin-bottom: 10px;
      font-size: 28px;
    }

    .subtitle {
      color: #666;
      margin-bottom: 30px;
      font-size: 14px;
    }

    .status {
      display: inline-block;
      padding: 8px 16px;
      border-radius: 20px;
      font-size: 12px;
      font-weight: 600;
      margin-bottom: 20px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
    }

    .status.dry-run {
      background: #fff3cd;
      color: #856404;
    }

    .status.publish {
      background: #d4edda;
      color: #155724;
    }

    .packages-list {
      list-style: none;
      margin-bottom: 20px;
    }

    .package-item {
      display: flex;
      align-items: center;
      padding: 12px 0;
      border-bottom: 1px solid #eee;
      transition: background 0.2s;
    }

    .package-item:last-child {
      border-bottom: none;
    }

    .package-item:hover {
      background: #f8f9fa;
      padding: 12px;
      margin: 0 -12px;
      border-radius: 6px;
    }

    .package-link {
      flex: 1;
      display: flex;
      align-items: center;
      gap: 12px;
      text-decoration: none;
      color: inherit;
      cursor: pointer;
    }

    .package-name {
      font-weight: 600;
      color: #667eea;
      font-family: 'Monaco', 'Courier New', monospace;
      font-size: 14px;
    }

    .package-version {
      font-size: 12px;
      color: #999;
      font-family: 'Monaco', 'Courier New', monospace;
    }

    .tag-badge {
      display: inline-block;
      background: #e9ecef;
      color: #495057;
      padding: 4px 10px;
      border-radius: 4px;
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
    }

    .footer {
      text-align: center;
      color: #999;
      font-size: 12px;
      margin-top: 20px;
      padding-top: 20px;
      border-top: 1px solid #eee;
    }

    .footer a {
      color: #667eea;
      text-decoration: none;
    }

    .footer a:hover {
      text-decoration: underline;
    }
  </style>
</head>
<body>
  <div class="container">
    <h1>📦 solid-jsx-oxc</h1>
    <p class="subtitle">Publish Results</p>

    <div class="status ${options.dryRun ? "dry-run" : "publish"}">
      ${options.dryRun ? "🔄 Dry Run" : "✅ Published"}
    </div>

    <ul class="packages-list">
      ${packages}
    </ul>

    <div class="footer">
      <p>Click any package to view on npm registry</p>
      <p style="margin-top: 8px;">Registry: <a href="${baseUrl}" target="_blank">${baseUrl}</a></p>
    </div>
  </div>
</body>
</html>
`;
}

async function runWithPty(
  cmd: string[],
  cwd: string,
): Promise<number> {
  const proc = Bun.spawn(cmd, {
    cwd,
    terminal: {
      cols: process.stdout.columns ?? 80,
      rows: process.stdout.rows ?? 24,
      data(term, data) {
        process.stdout.write(data);
      },
    },
  });

  // Handle terminal resize
  const resizeHandler = () => {
    if (proc.terminal) {
      proc.terminal.resize(process.stdout.columns ?? 80, process.stdout.rows ?? 24);
    }
  };

  process.stdout.on("resize", resizeHandler);

  try {
    const code = await proc.exited;
    process.stdout.off("resize", resizeHandler);
    return code;
  } catch (err) {
    process.stdout.off("resize", resizeHandler);
    throw err;
  }
}

async function main() {
  const options = parseCli();
  const repoRoot = import.meta.dir;

  if (options.help) {
    printHelp();
    return;
  }

  const packages = await readWorkspacePackages(repoRoot);

  if (packages.length === 0) {
    throw new UserError("No publishable workspace packages found.");
  }

  const rescopeConfig = options.rescope ? await loadRescopeConfig(repoRoot, options.rescope) : null;
  if (rescopeConfig) {
    // Auto-include any NAPI-RS per-platform sub-packages whose parent is
    // being rescoped, so callers don't have to list every `<pkg>-<target>`
    // by hand.
    extendRescopeWithSubPackages(rescopeConfig, packages);
  }

  // When rescoping, restrict the candidate set to the rescope list. Don't try
  // to publish packages that aren't in scope (e.g. upstream forks).
  let resolveBase = packages;
  if (rescopeConfig) {
    resolveBase = packages.filter((p) => rescopeConfig.packageNames.has(p.name));
    if (resolveBase.length === 0) {
      throw new UserError(
        `No workspace packages match rescope config for "${rescopeConfig.scope}". ` +
          `Listed names: ${Array.from(rescopeConfig.packageNames).join(", ")}`,
      );
    }
  }

  const targets = resolveTargets(resolveBase, options.only, options.exclude);

  if (targets.length === 0) {
    throw new UserError("No matching packages to publish.");
  }

  if (options.only.size > 0) {
    const missing = Array.from(options.only).filter((name) => !packages.some((p) => p.name === name));
    if (missing.length > 0) {
      throw new UserError(`Unknown package(s): ${missing.join(", ")}`);
    }
  }

  if (options.list) {
    console.log("\n📦 Publish order:");
    if (rescopeConfig) {
      for (const pkg of targets) {
        const scoped = `${rescopeConfig.scope}/${pkg.name}`;
        console.log(`  • ${pkg.name} → ${scoped}@${pkg.version}`);
      }
    } else {
      printTargets(targets);
    }
    return;
  }

  if (!options.dryRun && !options.allowDirty) {
    await ensureCleanGit(repoRoot);
  }

  console.log(`\n${"═".repeat(60)}`);
  console.log(`  solid-jsx-oxc v${targets[0].version}`);
  console.log(`${"═".repeat(60)}\n`);

  console.log(`📍 Tag: ${options.tag}`);
  console.log(`📝 Mode: ${options.dryRun ? "🔄 dry-run" : "🚀 publish"}`);
  if (rescopeConfig) console.log(`🏷️  Rescope: → ${rescopeConfig.scope}/<name>`);
  if (options.registry) console.log(`🌐 Registry: ${options.registry}`);
  if (options.access) console.log(`🔒 Access: ${options.access}`);
  console.log(`📦 Packages (${targets.length}):`);
  if (rescopeConfig) {
    for (const pkg of targets) {
      console.log(`  • ${pkg.name} → ${rescopeConfig.scope}/${pkg.name}@${pkg.version}`);
    }
  } else {
    printTargets(targets);
  }
  console.log("");

  const proceed = await promptToProceed(options, targets);
  if (!proceed) {
    console.log("⏹️  Cancelled.\n");
    return;
  }

  // Track every snapshot so we can restore on success, error, or cancellation.
  const snapshots: RescopeSnapshot[] = [];

  // Map of unscoped workspace name → version — used to resolve `workspace:*`
  // dep specifiers when we rescope (since the workspace name itself changes).
  const workspaceVersions = new Map(packages.map((p) => [p.name, p.version] as const));
  const packagesByName = new Map(packages.map((p) => [p.name, p] as const));

  // Track sub-packages we end up skipping (no binary on disk for that target).
  const skippedTargets = new Set<string>();

  // Best-effort SIGINT restore — Bun.spawn'd children get the signal too, but if a publish
  // is mid-flight we still want the working tree clean afterward.
  let interrupted = false;
  const sigintHandler = () => {
    interrupted = true;
  };
  process.on("SIGINT", sigintHandler);

  try {
    for (const pkg of targets) {
      const displayName = rescopeConfig ? `${rescopeConfig.scope}/${pkg.name}` : pkg.name;
      console.log(`\n${"─".repeat(60)}`);
      console.log(`📦 ${displayName}@${pkg.version}`);
      console.log(`${"─".repeat(60)}`);

      // Sub-packages: place the matching .node binary from the parent
      // package directory before publish. If no binary is available locally,
      // skip publishing this sub-package (it just means installs on that
      // platform fall back to the loader's "not found" warning, instead of
      // crashing).
      if (pkg.subPackageOf) {
        const placed = await placeSubPackageBinary(pkg, packagesByName);
        if (!placed) {
          console.log(
            `\n⚠️  No binary found for ${pkg.napiTarget} — skipping ${displayName}.`,
          );
          console.log(
            `   (Build it locally with \`napi build --target ${pkg.napiTarget}\`, ` +
              `or rely on the CI matrix.)`,
          );
          skippedTargets.add(pkg.name);
          continue;
        }
      } else if (!options.skipBuild && pkg.manifest.scripts?.build) {
        console.log("\n🔨 Building...");
        const buildCode = await runWithPty(["bun", "run", "build"], pkg.dir);
        if (buildCode !== 0) {
          throw new UserError(`Build failed for ${pkg.name} (exit code ${buildCode})`);
        }
      }

      // Apply rescope rewrites *after* build (build would otherwise overwrite dist files)
      // and *before* publish (bun publish reads package.json + ships files from disk).
      if (rescopeConfig) {
        console.log(`\n🏷️  Rewriting to ${rescopeConfig.scope}/${pkg.name}...`);
        const snapshot = await applyRescopeForPackage(pkg, rescopeConfig, workspaceVersions);
        snapshots.push(snapshot);
        if (snapshot.size > 0) {
          console.log(`   Modified ${snapshot.size} file(s) (will be restored after publish).`);
        }
      }

      if (interrupted) {
        throw new UserError("Interrupted before publish.");
      }

      // npm publish has no --tolerate-republish flag (that's bun-specific);
      // emulate it by pre-checking the registry. Skipped for dry-run because
      // the version may not exist yet anywhere.
      const publishedName = rescopeConfig ? `${rescopeConfig.scope}/${pkg.name}` : pkg.name;
      if (
        options.publisher === "npm" &&
        options.tolerateRepublish &&
        !options.dryRun &&
        (await isVersionPublished(publishedName, pkg.version, options.registry))
      ) {
        console.log(`\n⏭️  ${publishedName}@${pkg.version} already published — skipping.`);
        continue;
      }

      const publishArgs: string[] =
        options.publisher === "npm"
          ? ["npm", "publish", "--tag", options.tag]
          : ["bun", "publish", "--tag", options.tag];

      if (options.dryRun) publishArgs.push("--dry-run");
      if (options.publisher === "bun" && options.tolerateRepublish) {
        publishArgs.push("--tolerate-republish");
      }
      if (!options.runScripts) publishArgs.push("--ignore-scripts");

      if (options.registry) publishArgs.push(`--registry=${options.registry}`);
      if (options.access) publishArgs.push(`--access=${options.access}`);
      if (options.otp) publishArgs.push(`--otp=${options.otp}`);
      if (options.provenance) publishArgs.push("--provenance");

      // Direct stdio inheritance so any 2FA URLs / OIDC prompts land on the
      // user's terminal. The earlier pty wrapper swallowed those.
      console.log(`\n🚢 Publishing via ${options.publisher}...\n`);
      const publishProc = Bun.spawn(publishArgs, {
        cwd: pkg.dir,
        stdin: "inherit",
        stdout: "inherit",
        stderr: "inherit",
      });
      const publishCode = await publishProc.exited;
      if (publishCode !== 0) {
        throw new UserError(`Publishing failed for ${pkg.name} (exit code ${publishCode})`);
      }
    }
  } finally {
    process.off("SIGINT", sigintHandler);
    if (snapshots.length > 0) {
      console.log("\n♻️  Restoring rewritten files...");
      await restoreRescopeSnapshots(snapshots);
    }
  }

  console.log(`\n${"═".repeat(60)}`);
  console.log(`  ${options.dryRun ? "✅ Dry run complete" : "🎉 Publish complete"}!`);
  console.log(`${"═".repeat(60)}\n`);

  if (skippedTargets.size > 0) {
    console.log(
      `\n⚠️  Skipped ${skippedTargets.size} sub-package(s) (no local binary): ` +
        Array.from(skippedTargets).join(", "),
    );
  }

  // Generate HTML report. Don't auto-open — on headless or non-default-GUI
  // machines `xdg-open` / `open` / `start` just spew errors and fail. Print
  // the path; the user can click it (most terminals make file:// paths
  // clickable) or open it manually.
  const reportTargets = targets.filter((p) => !skippedTargets.has(p.name));
  const htmlReport = generateHtmlReport(reportTargets, options, rescopeConfig);
  const reportPath = join(repoRoot, "publish-report.html");
  await Bun.write(reportPath, htmlReport);

  console.log(`📄 Report: file://${reportPath}`);
}

await main().catch((err) => {
  const message = err instanceof Error ? err.message : String(err);
  console.error(`\n❌ ${message}`);
  process.exit(1);
});
