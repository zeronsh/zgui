// Builds website/content/docs from the repository's docs, as
// docs.manifest.mjs places them. Runs before every `dev` and `build`; its
// output is not committed.
//
// Each page is the source's Markdown with its H1 lifted into the
// frontmatter, links rewritten (a doc's link to another doc goes to that
// doc's page and heading on the site, anything else in the repository to
// GitHub), images copied, and code fences given a language Shiki knows.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkGfm from 'remark-gfm';
import remarkStringify from 'remark-stringify';
import { toString } from 'mdast-util-to-string';
import { visit } from 'unist-util-visit';
import GithubSlugger from 'github-slugger';
import { sets, internal, repo, branch, rewrites } from '../docs.manifest.mjs';

const website = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const root = path.resolve(website, '..');
const outDir = path.join(website, 'content/docs');
const assetsDir = path.join(website, 'public/_docs');
const handWritten = path.join(website, 'content-src');

/** Rustdoc compiles these files' bare fences as Rust. */
const doctested = new Set(['README.md', 'docs/composition.md', 'docs/styling.md', 'docs/window-controls.md']);

const errors = [];
const fail = (message) => errors.push(message);

// --- Coverage: every public doc is placed or declared internal. ----------
const placed = new Set(sets.flatMap((set) => set.pages.filter((p) => p.src).map((p) => p.src)));
for (const dir of ['docs', 'docs/agents', 'docs/reference']) {
  for (const name of fs.readdirSync(path.join(root, dir))) {
    const file = `${dir}/${name}`;
    if (!name.endsWith('.md')) continue;
    if (!placed.has(file) && !internal.includes(file))
      fail(`${file} is neither placed in docs.manifest.mjs nor listed as internal`);
  }
}

// --- Parse each source once. ---------------------------------------------
const processor = unified().use(remarkParse).use(remarkGfm);
const sources = new Map();
function source(src) {
  if (sources.has(src)) return sources.get(src);
  const file = path.join(root, src);
  if (!fs.existsSync(file)) {
    fail(`${src}: no such file`);
    return null;
  }
  // The changelog's unreleased entries are files of their own
  // (changelog.d/), shown in its Unreleased section as a release puts them.
  const text = src === 'CHANGELOG.md'
    ? execFileSync('python3', [path.join(root, 'scripts/changelog.py'), 'render'], { encoding: 'utf8' })
    : fs.readFileSync(file, 'utf8');
  const tree = processor.parse(text);
  const h1 = tree.children.find((n) => n.type === 'heading' && n.depth === 1);
  // Anchors as GitHub computes them, over the whole file.
  const slugger = new GithubSlugger();
  const headings = [];
  visit(tree, 'heading', (node) => {
    node.data = { ...node.data, sourceSlug: slugger.slug(toString(node)) };
    headings.push(node);
  });
  const parsed = { tree, title: h1 ? toString(h1) : path.basename(src, '.md'), headings };
  sources.set(src, parsed);
  return parsed;
}

/** The nodes of a page, cloned, and the source's sections none took. */
function select(parsed, page) {
  const nodes = parsed.tree.children.filter((n) => !(n.type === 'heading' && n.depth === 1));
  const firstH2 = nodes.findIndex((n) => n.type === 'heading' && n.depth === 2);
  const intro = firstH2 === -1 ? nodes : nodes.slice(0, firstH2);
  const sections = [];
  for (let i = firstH2; i !== -1 && i < nodes.length; ) {
    let end = nodes.findIndex((n, j) => j > i && n.type === 'heading' && n.depth === 2);
    if (end === -1) end = nodes.length;
    sections.push({ title: toString(nodes[i]), nodes: nodes.slice(i, end) });
    i = end;
  }
  const wantIntro = page.intro ?? !page.sections;
  let picked = wantIntro ? [...intro] : [];
  if (page.sections) {
    page.sections.forEach((want, index) => {
      const found = sections.find((s) => s.title.startsWith(want));
      if (!found) return fail(`${page.src}: no section "${want}" for ${page.slug}`);
      found.taken = true;
      // `unwrap`: the first section's heading is the page's title.
      picked.push(...(page.unwrap && index === 0 ? found.nodes.slice(1).map(promote) : found.nodes));
    });
  } else {
    picked = nodes;
    sections.forEach((s) => (s.taken = true));
  }
  return { nodes: structuredClone(picked), sections, unwrappedSlug: page.unwrap ? found(sections, page)?.nodes[0].data.sourceSlug : null };
}
const found = (sections, page) => sections.find((s) => s.title.startsWith(page.sections[0]));
function promote(node) {
  if (node.type === 'heading') return { ...node, depth: Math.max(2, node.depth - 1) };
  return node;
}

// --- Plan every page: its URL and the anchors it holds. --------------------
const pages = [];
const anchors = new Map(); // `${src}#${sourceSlug}` → url
const fileUrls = new Map(); // src → url of its first page
const taken = new Map(); // src → Set of section titles taken
// A page with `when` exists only while that file does (a platform's results).
for (const set of sets) set.pages = set.pages.filter((p) => typeof p === 'string' || !p.when || fs.existsSync(path.join(root, p.when)));
for (const set of sets) {
  for (const page of set.pages) {
    if (typeof page === 'string') continue;
    const url = `/${set.dir}${page.slug === 'index' ? '' : `/${page.slug}`}`;
    if (page.mdx) {
      pages.push({ set, page, url });
      continue;
    }
    const parsed = source(page.src);
    if (!parsed) continue;
    const { nodes, sections, unwrappedSlug } = select(parsed, page);
    for (const s of sections) if (s.taken) (taken.get(page.src) ?? taken.set(page.src, new Set()).get(page.src)).add(s.title);
    // Anchors as Fumadocs computes them, over the page.
    const slugger = new GithubSlugger();
    for (const node of nodes) {
      if (node.type !== 'heading') continue;
      const slug = slugger.slug(toString(node));
      anchors.set(`${page.src}#${node.data.sourceSlug}`, `${url}#${slug}`);
    }
    if (unwrappedSlug) anchors.set(`${page.src}#${unwrappedSlug}`, url);
    if (!fileUrls.has(page.src) || (page.intro ?? !page.sections)) {
      if (!fileUrls.has(page.src) || !fileUrls.get(page.src).intro)
        fileUrls.set(page.src, { url, intro: page.intro ?? !page.sections });
    }
    pages.push({ set, page, url, parsed, nodes });
  }
}
// A split file's intro is on some page too (README's lede is the home page's).
for (const src of taken.keys()) {
  if (src === 'README.md' || !src.startsWith('docs/')) continue;
  const pagesOf = sets.flatMap((set) => set.pages).filter((p) => typeof p !== 'string' && p.src === src);
  const nodes = sources.get(src).tree.children;
  const firstH2 = nodes.findIndex((n) => n.type === 'heading' && n.depth === 2);
  const hasIntro = nodes.slice(0, firstH2).some((n) => n.type !== 'heading');
  if (hasIntro && !pagesOf.some((p) => p.intro ?? !p.sections)) fail(`${src}: its text before the first section is on no page (give a page intro: true)`);
}
// A split file's every section is on some page.
for (const [src, titles] of taken) {
  for (const heading of sources.get(src).headings.filter((h) => h.depth === 2)) {
    if (!titles.has(toString(heading)) && src.startsWith('docs/'))
      fail(`${src}: section "${toString(heading)}" is on no page (add it to a page's sections)`);
  }
}

// --- Links. ---------------------------------------------------------------
const githubUrl = (repoPath, hash = '') => {
  const full = path.join(root, repoPath);
  const kind = fs.existsSync(full) && fs.statSync(full).isDirectory() ? 'tree' : 'blob';
  return `${repo}/${kind}/${branch}/${repoPath}${hash ? `#${hash}` : ''}`;
};

function resolveLink(src, href) {
  if (rewrites[href]) return rewrites[href][0];
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('//')) return href;
  const [target, hash = ''] = href.split('#');
  let repoPath = src;
  if (target) {
    if (target.startsWith('/')) repoPath = target.slice(1);
    else repoPath = path.posix.normalize(path.posix.join(path.posix.dirname(src), target));
  }
  repoPath = repoPath.replace(/\/$/, '');
  if (repoPath.startsWith('..')) return href;
  if (hash && anchors.has(`${repoPath}#${hash}`)) return anchors.get(`${repoPath}#${hash}`);
  if (fileUrls.has(repoPath)) {
    if (hash) console.warn(`  ${src}: ${href}: no heading #${hash} on the site, linking the page`);
    return fileUrls.get(repoPath).url;
  }
  if (!fs.existsSync(path.join(root, repoPath))) console.warn(`  ${src}: ${href}: no such file in the repository`);
  return githubUrl(repoPath, hash);
}

function copyImage(src, url) {
  if (/^[a-z]+:/i.test(url) || url.startsWith('/')) return url;
  const repoPath = path.posix.normalize(path.posix.join(path.posix.dirname(src), url));
  const from = path.join(root, repoPath);
  if (!fs.existsSync(from)) {
    fail(`${src}: image ${url} does not exist`);
    return url;
  }
  const to = path.join(assetsDir, repoPath);
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.copyFileSync(from, to);
  return `/_docs/${repoPath}`;
}

// --- Code fences. ------------------------------------------------------------
const known = new Set(['rust', 'luau', 'lua', 'sh', 'bash', 'shell', 'console', 'powershell', 'json', 'jsonc', 'toml', 'text', 'wgsl', 'swift', 'diff', 'yaml', 'python', 'ts', 'js', 'html', 'css', 'go']);
function fence(src, node) {
  let [lang, ...attrs] = (node.lang ?? '').split(',');
  if (lang === 'no_run' || lang === 'ignore' || lang === 'should_panic') lang = 'rust';
  if (!lang) {
    if (doctested.has(src)) lang = 'rust';
    else if (/\bfn |\blet |::|\.child\(|^use /m.test(node.value)) lang = 'rust';
    else if (/\bui\.|^local /m.test(node.value)) lang = 'luau';
    else if (/^\s*(cargo|python3?|zgx|npx|git|cd|export|sudo) /m.test(node.value)) lang = 'sh';
    else lang = 'text';
  }
  if (lang === 'lua') lang = 'luau';
  if (!known.has(lang)) lang = 'text';
  // Rustdoc's hidden lines: `# ` or a lone `#` start a line the reader doesn't see.
  if (lang === 'rust' && !attrs.includes('text')) {
    node.value = node.value
      .split('\n')
      .filter((line) => !/^\s*#( |$)/.test(line))
      .map((line) => line.replace(/^(\s*)##/, '$1#'))
      .join('\n');
  }
  // A project's file (`-- file: plugins/x.luau`, or ```json file=.luaurc,
  // the convention the docs' tests read) is shown under its name.
  let title = (node.meta ?? '').match(/(?:^|\s)file=(\S+)/)?.[1];
  const marker = node.value.match(/^-- file: (.+)\n/);
  if (marker) {
    title = marker[1].trim();
    node.value = node.value.slice(marker[0].length);
  }
  node.value = node.value.replace(/^(\s*\n)+/, '').replace(/(\n\s*)+$/, '');
  node.lang = lang;
  node.meta = title ? `title="${title}"` : null;
}

// --- Facts read from the source, for hand-written Markdown pages. -----------
/** A Rust `const NAME: &str = "…";`, unescaped. */
function rustString(file, name) {
  const match = fs.readFileSync(path.join(root, file), 'utf8').match(new RegExp(`const ${name}: &str = "((?:[^"\\\\]|\\\\.)*)";`));
  if (!match) return fail(`${file}: no const ${name}: &str`), '';
  return match[1].replace(/\\(["\\n])/g, (_, c) => (c === 'n' ? '\n' : c));
}
/** `zgx lint`'s rules: `zgx_script::lint::RULES`. */
function lintRules() {
  const source = fs.readFileSync(path.join(root, 'crates/zgx-script/src/lint.rs'), 'utf8');
  const start = source.indexOf('pub const RULES');
  const table = source.slice(start, source.indexOf('];', start));
  const rules = [...table.matchAll(/\(\s*"([^"]+)",\s*"((?:[^"\\\\]|\\\\.)*)",?\s*\)/g)].map((m) => [m[1], m[2].replace(/\\"/g, '"')]);
  if (!rules.length) fail('crates/zgx-script/src/lint.rs: no RULES');
  return rules;
}
function expandFromSource(text, page) {
  return text
    .replace(/<!-- usage (\S+) (\S+) -->/g, (_, file, name) => `\`\`\`text title="${file.split('/').pop()} · ${name}"\n${rustString(file, name)}\n\`\`\``)
    .replace(/<!-- lint-rules -->/g, () =>
      ['| Rule | Flags |', '| --- | --- |', ...lintRules().map(([rule, what]) => `| \`${rule}\` | ${what.replace(/\|/g, '\\|')} |`)].join('\n'))
    .replace(/<!-- (usage|lint-rules)[^>]*-->/g, (m) => (fail(`${page}: ${m} is not a directive the sync knows`), m));
}

// --- Write. ------------------------------------------------------------------
fs.rmSync(outDir, { recursive: true, force: true });
// Astro keeps rendered pages by their files' contents, so a change to the
// Markdown pipeline (lib/code-frame.mjs, the themes) would leave old HTML.
for (const store of ['.astro/data-store.json', 'node_modules/.astro/data-store.json'])
  fs.rmSync(path.join(website, store), { force: true });
fs.rmSync(assetsDir, { recursive: true, force: true });
fs.mkdirSync(outDir, { recursive: true });

const stringify = unified().use(remarkGfm, { tablePipeAlign: false }).use(remarkStringify, { bullet: '-', fences: true, rule: '-', emphasis: '_' });
const yaml = (fields) =>
  `---\n${Object.entries(fields)
    .filter(([, v]) => v !== undefined)
    .map(([k, v]) => `${k}: ${JSON.stringify(v)}`)
    .join('\n')}\n---\n\n`;

function firstSentence(nodes) {
  const para = nodes.find((n) => n.type === 'paragraph');
  if (!para) return undefined;
  const text = toString(para).replace(/\s+/g, ' ').trim();
  const match = text.match(/^(.+?[.!?])(\s|$)/);
  const sentence = match ? match[1] : text;
  return sentence.length > 220 ? `${sentence.slice(0, 217).replace(/\s+\S*$/, '')}…` : sentence;
}

for (const { set, page, url, parsed, nodes } of pages) {
  const file = path.join(outDir, set.dir, `${page.slug}.${page.mdx?.endsWith('.mdx') ? 'mdx' : 'md'}`);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  if (page.mdx) {
    // Its source, for "Edit on GitHub", as the synced pages carry theirs.
    const text = expandFromSource(fs.readFileSync(path.join(handWritten, page.mdx), 'utf8'), page.mdx);
    fs.writeFileSync(file, text.replace(/^---\n/, `---\nsource: "website/content-src/${page.mdx}"\n`));
    continue;
  }
  const tree = { type: 'root', children: nodes };
  visit(tree, (node) => {
    if (node.type === 'link' || node.type === 'definition') {
      const before = node.url;
      node.url = resolveLink(page.src, node.url);
      // A link written as its old address says what it opens now.
      if (node.type === 'link' && rewrites[before] && before.includes(toString(node).replace(/\/$/, '')))
        node.children = [{ type: 'text', value: rewrites[before][1] }];
    }
    else if (node.type === 'image') node.url = copyImage(page.src, node.url);
    else if (node.type === 'code') fence(page.src, node);
  });
  const description = page.description ?? firstSentence(nodes);
  const body = stringify.stringify(tree);
  fs.writeFileSync(
    file,
    yaml({ title: page.title ?? parsed.title, description, lede: Boolean(page.description), source: page.src, heading: parsed.title }) + body,
  );
  void url;
}

// meta.json: the root, each set (a root folder: a tab of the switcher), and
// folders of nested pages flattened into their set's list.
fs.writeFileSync(path.join(outDir, 'meta.json'), JSON.stringify({ pages: sets.map((s) => s.dir) }, null, 2));
for (const set of sets) {
  const list = [];
  const folders = new Map();
  for (const page of set.pages) {
    if (typeof page === 'string') {
      list.push(page);
      continue;
    }
    const [head, ...rest] = page.slug.split('/');
    if (rest.length) {
      if (!folders.has(head)) folders.set(head, []);
      folders.get(head).push(rest.join('/'));
    } else if (set.pages.some((p) => typeof p !== 'string' && p.slug.startsWith(`${head}/`))) {
      // `reference.md` and `reference/…`: the folder's index, its pages flattened here.
      fs.renameSync(path.join(outDir, set.dir, `${head}.md`), path.join(outDir, set.dir, head, 'index.md'));
      folders.set(head, ['index', ...(folders.get(head) ?? [])]);
      list.push(`...${head}`);
    } else list.push(page.slug);
  }
  fs.writeFileSync(
    path.join(outDir, set.dir, 'meta.json'),
    JSON.stringify({ title: set.title, description: set.description, icon: set.icon, root: true, pages: list }, null, 2),
  );
  for (const [folder, items] of folders)
    fs.writeFileSync(path.join(outDir, set.dir, folder, 'meta.json'), JSON.stringify({ pages: items }, null, 2));
}

// --- Redirects: the repository's paths (docs/layout.md) to their pages. --
{
  const lines = ['/docs /rust 301', '/docs/ /rust 301'];
  for (const [src, { url }] of fileUrls) {
    if (!src.startsWith('docs/')) continue;
    const bare = `/${src.replace(/\.md$/, '')}`;
    lines.push(`/${src} ${url} 301`, `${bare} ${url} 301`);
  }
  lines.push('/CHANGELOG.md /rust/changelog 301', '/README.md /rust 301');
  fs.writeFileSync(path.join(website, 'public/_redirects'), `${lines.join('\n')}\n`);
}

// --- Performance: the published run (site/, written and checked by
// scripts/build_site.py) and the builder's own words for its scenes and
// metrics, so the site says what the builder says.
{
  const { execFileSync } = await import('node:child_process');
  const text = execFileSync('python3', ['-c', `
import sys, json
sys.path.insert(0, 'scripts')
import build_site as b
print(json.dumps({
  'scenes': {k: {'title': t, 'note': n} for k, (t, n) in b.SCENES.items()},
  'modes': b.MODES,
  'metrics': {k: {'title': t, 'unit': u, 'digits': d, 'scale': s, 'source': src, 'applies': a} for k, (t, u, d, s, src, a) in b.METRICS.items()},
  'micro_scenes': {k: {'title': t, 'note': n} for k, (t, n) in b.MICRO_SCENES.items()},
  'rune_notes': b.RUNE_NOTES,
  'windows_metrics': {k: {'title': t, 'unit': u, 'digits': d, 'scale': s, 'source': src, 'applies': a} for k, (t, u, d, s, src, a) in getattr(b, 'WINDOWS_METRICS', {}).items()},
}))
`], { cwd: root, encoding: 'utf8' });
  const data = JSON.parse(fs.readFileSync(path.join(root, 'site/data.json'), 'utf8'));
  const micro = fs
    .readFileSync(path.join(root, 'site/micro.jsonl'), 'utf8')
    .split('\n')
    .filter((line) => line.startsWith('{'))
    .map((line) => JSON.parse(line));
  const generated = path.join(website, 'src/generated');
  fs.mkdirSync(generated, { recursive: true });
  const words = JSON.parse(text);
  fs.writeFileSync(path.join(generated, 'perf.json'), JSON.stringify({ ...data, text: words, micro }));
  // Windows' run, on a page of its own when it is published (never ranked with macOS's).
  const windowsData = path.join(root, 'site/windows/data.json');
  fs.writeFileSync(
    path.join(generated, 'perf-windows.json'),
    JSON.stringify(fs.existsSync(windowsData) ? { available: true, ...JSON.parse(fs.readFileSync(windowsData, 'utf8')), metricFormats: words.windows_metrics, modes: words.modes, sceneText: words.scenes } : { available: false }),
  );
}

// --- The home page's words: README's "What it has" and its two samples. ----
{
  const { default: remarkRehype } = await import('remark-rehype');
  const { default: rehypeStringify } = await import('rehype-stringify');
  const readme = source('README.md');
  const nodes = readme.tree.children;
  const sectionOf = (title) => {
    const start = nodes.findIndex((n) => n.type === 'heading' && n.depth === 2 && toString(n) === title);
    if (start === -1) return fail(`README.md: no section "${title}" for the home page`), [];
    const end = nodes.findIndex((n, i) => i > start && n.type === 'heading' && n.depth <= 2);
    return nodes.slice(start + 1, end === -1 ? undefined : end);
  };
  const toHtml = unified().use(remarkRehype).use(rehypeStringify);
  const html = (node) => toHtml.stringify(toHtml.runSync({ type: 'root', children: [node] }));
  const list = sectionOf('What it has').find((n) => n.type === 'list');
  const features = (list?.children ?? []).map((item) => {
    const para = structuredClone(item.children[0]);
    visit(para, 'link', (node) => (node.url = resolveLink('README.md', node.url)));
    const [lead, ...rest] = para.children;
    const title = lead?.type === 'strong' ? toString(lead).replace(/:$/, '') : '';
    const body = { ...para, children: lead?.type === 'strong' ? rest : para.children };
    return { title, html: html(body).replace(/^<p>:?\s*/, '').replace(/<\/p>$/, '') };
  });
  const quick = sectionOf('Quick start').filter((n) => n.type === 'code');
  const rust = quick.find((n) => n.lang?.startsWith('rust'));
  const luau = quick.find((n) => n.lang === 'lua' || n.lang === 'luau');
  if (!rust || !luau) fail('README.md: Quick start needs a rust and a lua sample for the home page');
  // The same counter in both languages: getting started's "State" sections,
  // the Rust one's component without its main.
  const guide = source('docs/getting-started.md').tree.children;
  const stateCode = (part) => {
    const at = guide.findIndex((n) => n.type === 'heading' && n.depth === 2 && toString(n) === part);
    const state = guide.findIndex((n, i) => i > at && n.type === 'heading' && n.depth === 3 && toString(n) === 'State');
    const code = guide.slice(state).find((n) => n.type === 'code');
    if (at === -1 || state === -1 || !code) fail(`docs/getting-started.md: no "${part}" › "State" sample for the home page`);
    return code?.value ?? '';
  };
  const rustState = stateCode('In Rust');
  const counter = rustState.slice(rustState.indexOf('fn counter'), rustState.indexOf('\nfn main')).trim();
  if (!counter.startsWith('fn counter')) fail('docs/getting-started.md: the Rust "State" sample has no fn counter()');
  const samples = { rust: `use zgx::compose::prelude::*;\n\n${counter}`, luau: stateCode('In Luau') };
  const lede = toString(nodes.find((n) => n.type === 'paragraph'));
  fs.writeFileSync(
    path.join(website, 'src/generated/readme.json'),
    JSON.stringify({ lede, features, rust: rust?.value, luau: luau?.value, samples }),
  );
}

if (errors.length) {
  console.error(`sync-docs: ${errors.length} problem(s)\n${errors.map((e) => `  ${e}`).join('\n')}`);
  process.exit(1);
}
console.log(`sync-docs: ${pages.length} pages in ${sets.length} sets`);
