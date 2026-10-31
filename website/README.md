# zgx.zeron.sh

The public site and docs for zgx: a home page and five doc sets (zgx in
Rust, zgx-script, platforms, devtools, performance), each a root folder
with its own sidebar and the switcher between them.

```sh
cd website
npm ci
npm run dev        # http://localhost:4321, docs synced from ../docs first
npm run build      # dist/: static HTML, search index, OG images, llms.txt
npx astro preview  # serve dist/
```

A push to `main` that touches `website/`, `docs/` or the files the site
reads deploys it (`.github/workflows/site.yml`, Cloudflare Pages project
`zgx-site`).

## Where the words come from

Nothing here copies the repository's docs. `scripts/sync-docs.mjs` runs
before every `dev` and `build` and writes `content/` (not committed) from:

- **`docs/`**, placed by `docs.manifest.mjs`: which set each file belongs
  to, its slug and sidebar title, and for a long file which `##` sections
  make which page. Links between docs become links between pages (anchors
  included), links into the rest of the repository go to GitHub, images
  are copied, code fences get a language Shiki knows, and Rustdoc's hidden
  lines are dropped.
- **`content-src/`**: the few hand-written pages (the platforms overview,
  the CLI reference, the performance pages), MDX whose components read
  their facts from the source: `zgx --help` from `zgx.rs`, the lint rules
  from `lint.rs`, every benchmark number from `site/data.json` and
  `site/micro.jsonl` (written and checked by `scripts/build_site.py`, whose
  scene and metric words are imported from it, not retyped).
- **`README.md`**: the home page's lede, feature list and both samples.

The build fails when a doc in `docs/`, `docs/agents/` or `docs/reference/`
is neither placed nor listed as internal, when a split doc has a section no
page takes, or when an image is missing: a new doc can't silently not
appear. Internal notes (`ROADMAP`, `MACOS-HANDOFF`, the feature audit, GPUI
parity, `results/`, `platform-validation/`) stay on GitHub.

To add a doc: write it in `docs/`, then place it in `docs.manifest.mjs`.

## Stack, and why

**Astro with Fumadocs, through Fumadocs' published packages.** Fumadocs
supports Astro with `fumadocs-core` (the source loader, page tree, static
search, Markdown plugins) and `fumadocs-ui` rendered as a React island
(`fumadocs-ui/provider/astro`). This site follows the official Astro
example as released (fumadocs 16.17.8):

- Markdown goes through Astro's unified processor with Fumadocs' remark and
  rehype plugins (heading ids, search's structured data, Shiki in both
  themes). Astro's own highlighter is off (`markdown.syntaxHighlight`).
- Each docs page is one island, `<Docs client:load>`: Fumadocs' layout,
  sidebar, switcher, table of contents and search. The page's content is
  rendered by Astro and passed in as static HTML.
- The page tree is serialized once, icons rendered to HTML on the server
  (`lucide-icons` plugin), so no icon library ships to the browser. The
  island copies the tree before deserializing it: Fumadocs' loader does it
  in place, and Astro serializes props after rendering them.
- Pages are plain multi-page navigations (Astro prefetches links on hover),
  so `pathname` is the page's URL, not `Astro.url` (which ends in `.html`
  with `build.format: 'file'`).
- `fumadocs-ui` is the Base UI build (`@fumadocs/base-ui`), as the example
  pins it.

Fumadocs has since merged a fuller integration, `@fumadocs/astro`
(fuma-nama/fumadocs#3751, 2026-10-24: MDX components hydrated inside the
content, a Sätteri processor, `transition:persist`). It isn't on npm yet;
move to it once it is, and drop `CopyCode.astro` (code blocks' copy buttons,
which Markdown rendered outside React doesn't get from Fumadocs).

The home page and the performance charts are Astro components: static HTML
and CSS, no JavaScript beyond the theme toggle. Charts are tables with bars
drawn in CSS, readable by a screen reader, each row labelled by framework
so colour is never the only cue (zgx in its green, the rest in grey).

## For agents

`/llms.txt` lists every page; `/llms-full.txt` is all of them in one file;
any page's Markdown is at its URL plus `.md` (`/devtools/targets.md`).

`/api/mcp` serves the same pages over MCP (streamable HTTP), with Fumadocs'
`list_pages`, `get_page` and `search` tools. It's `functions/api/mcp.ts`, a
Cloudflare Pages Function and the only route that runs on demand: the site
stays Astro's static output, with no adapter, and wrangler sends only that path
to the Function. A Function can't read Astro's content, so
`scripts/build-mcp.mjs` hands it the built pages and search index as text; the
build fails if they pass the budget for a Function's size (3 MiB compressed on
Cloudflare's free plan). `npm run check:mcp` drives it with a real MCP client in
`wrangler pages dev`, or at a URL given after `--`.

## Checking the design

```sh
npx playwright install chromium
npx astro preview &
node scripts/shoot.mjs / /rust/layout /performance   # desktop and mobile, light and dark
```
