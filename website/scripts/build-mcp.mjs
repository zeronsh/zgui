// What the docs' MCP server (functions/api/mcp.ts) serves, from the site just
// built: the pages `/llms.txt` lists, their Markdown, and the search index.
// Run after `astro build` (part of `npm run build`).
import fs from 'node:fs';
import path from 'node:path';
import { gzipSync } from 'node:zlib';

const dist = path.resolve('dist');
const out = path.resolve('src/generated/mcp');

// A Function's bundle may be 3 MiB compressed on Cloudflare's free plan; the
// server's own code takes about 0.25 of that.
const budget = 2.5 * 1024 * 1024;

const index = fs.readFileSync(path.join(dist, 'llms.txt'), 'utf8');
const urls = [...index.matchAll(/\]\(https:\/\/zgx\.zeron\.sh(\/[^)\s]*)\.md\)/g)].map((m) => m[1]);
const pages = urls.map((url) => ({ url, markdown: fs.readFileSync(path.join(dist, `${url}.md`), 'utf8') }));
const pagesText = JSON.stringify({ index, pages });
const searchText = fs.readFileSync(path.join(dist, 'api/search.json'), 'utf8');

fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, 'pages.txt'), pagesText);
fs.writeFileSync(path.join(out, 'search.txt'), searchText);

const compressed = gzipSync(pagesText).length + gzipSync(searchText).length;
const mib = (n) => `${(n / 1024 / 1024).toFixed(2)} MiB`;
if (compressed > budget) {
  console.error(`build-mcp: the docs are ${mib(compressed)} compressed, over the ${mib(budget)} budget for a Function`);
  process.exit(1);
}
console.log(`build-mcp: ${pages.length} pages and the search index, ${mib(compressed)} compressed`);
