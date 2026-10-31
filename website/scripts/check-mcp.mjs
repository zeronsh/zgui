// The docs' MCP server, through a real client: initialize, list the tools,
// list pages, search, read. Against a URL (the live site) or, with none, the
// built site served by `wrangler pages dev` as Cloudflare runs it.
import { spawn } from 'node:child_process';
import { Client, StreamableHTTPClientTransport } from '@modelcontextprotocol/client';

const port = 8788;
let url = process.argv[2];
let dev;
if (!url) {
  url = `http://127.0.0.1:${port}/api/mcp`;
  dev = spawn('npx', ['wrangler', 'pages', 'dev', 'dist', '--port', String(port), '--ip', '127.0.0.1'], { stdio: 'ignore', detached: true });
  for (let i = 0; ; i++) {
    const up = await fetch(`http://127.0.0.1:${port}/llms.txt`).catch(() => null);
    if (up?.ok) break;
    if (i === 120) throw new Error('wrangler pages dev never answered');
    await new Promise((r) => setTimeout(r, 500));
  }
}

const check = (ok, what) => {
  if (!ok) throw new Error(`check-mcp: ${what}`);
  console.log(`ok: ${what}`);
};

try {
  const client = new Client({ name: 'zgx-docs-check', version: '1.0.0' });
  await client.connect(new StreamableHTTPClientTransport(new URL(url)));
  check(client.getServerVersion()?.name === 'zgx-docs', 'initialize names the server');

  const { tools } = await client.listTools();
  check(['get_page', 'list_pages', 'search'].every((name) => tools.some((t) => t.name === name)), `tools: ${tools.map((t) => t.name).join(', ')}`);

  const text = (result) => result.content[0].text;
  const index = text(await client.callTool({ name: 'list_pages', arguments: {} }));
  const links = [...index.matchAll(/\]\((https:\/\/[^)]+\.md)\)/g)].map((m) => m[1]);
  check(links.length > 50, `list_pages: ${links.length} pages`);

  const hits = JSON.parse(text(await client.callTool({ name: 'search', arguments: { query: 'reconnect socket' } })));
  check(hits.length > 0 && hits.length <= 30 && hits.some((h) => h.url.startsWith('/luau/')), `search: ${hits.length} results, first ${hits[0]?.url}`);

  for (const page of [links[0], '/devtools/targets', '/devtools/targets.md', hits[1].url]) {
    const read = await client.callTool({ name: 'get_page', arguments: { url: page } });
    check(!read.isError && text(read).startsWith('# '), `get_page ${page}`);
  }
  const missing = await client.callTool({ name: 'get_page', arguments: { url: '/nothing' } });
  check(missing.isError === true, 'get_page says when there is no page');
  await client.close();
} finally {
  if (dev) process.kill(-dev.pid);
}
