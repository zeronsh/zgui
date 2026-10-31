// The docs as an MCP server: https://zgx.zeron.sh/api/mcp, public and read
// only, over streamable HTTP. A Cloudflare Pages Function, the one route of
// the site that runs on demand: every page stays a static file.
//
// The pages and the search index are the ones the site just built, bundled as
// text by scripts/build-mcp.mjs (a Function can't read `astro:content`). They
// parse as the isolate starts, which has a limit of its own: parsing them per
// request would pass the 10 ms of CPU a request gets on Cloudflare's free plan.
import { createMcpHandler, McpServer } from '@modelcontextprotocol/server';
import { registerSearchTool, registerSourceTools } from 'fumadocs-core/mcp';
import { create, getByID, load, search } from 'zbsearch';
import pagesText from '../../src/generated/mcp/pages.txt';
import indexText from '../../src/generated/mcp/search.txt';

type Page = { url: string; markdown: string };
type Docs = { index: string; pages: Page[] };
type Section = { id: string; page_id: string; type: string; content: string; url: string };

const docs: Docs = JSON.parse(pagesText);
const pages = new Map(docs.pages.map((page) => [page.url, page]));

// What the static search uses, in the browser, on the same file.
const db = create({ schema: { content: 'string' }, language: 'english' });
load(db, JSON.parse(indexText));

const MAX_RESULTS = 30;
const MAX_QUERY = 200;

/** Any way of naming a page `list_pages` and `search` show: a path, a full URL, `.md`, an anchor. */
function pagePath(url: string): string {
  const path = url
    .trim()
    .replace(/^https?:\/\/[^/]+/, '')
    .replace(/[#?].*$/, '')
    .replace(/\.md$/, '')
    .replace(/\/+$/, '');
  return path.startsWith('/') ? path : `/${path}`;
}

/** Fumadocs' `SearchServer`: pages and their matching sections, as the site's search dialog shows them. */
const searchServer = {
  async search(query: string) {
    const term = query.trim().slice(0, MAX_QUERY);
    if (!term) return [];
    const found = await search(db, {
      term,
      limit: MAX_RESULTS,
      properties: ['content'],
      groupBy: { properties: ['page_id'], maxResult: 4 },
    });
    const results: { url: string; type: string; content: string }[] = [];
    for (const group of found.groups ?? []) {
      const page = getByID(db, group.values[0] as string) as unknown as Section | undefined;
      if (!page) continue;
      results.push({ url: page.url, type: 'page', content: page.content });
      for (const hit of group.result) {
        const section = hit.document as unknown as Section;
        if (section.type !== 'page') results.push({ url: section.url, type: section.type, content: section.content });
      }
    }
    return results.slice(0, MAX_RESULTS);
  },
};

const source = { getPageByUrl: (url: string) => pages.get(pagePath(url)) };
const llms = { index: () => docs.index, page: (page: Page) => page.markdown };

function server(): McpServer {
  const mcp = new McpServer(
    { name: 'zgx-docs', version: '1.0.0', websiteUrl: 'https://zgx.zeron.sh' },
    { instructions: 'Documentation of zgx, a retained-mode GUI framework: Rust views and Luau scripts. Find a page with `search` or `list_pages`, then read it with `get_page`.' },
  );
  registerSourceTools(mcp, source as never, llms as never);
  registerSearchTool(mcp, searchServer as never);
  return mcp;
}

const handler = createMcpHandler(server, { responseMode: 'json', maxRequestBodySize: 64 * 1024 });

// Open to pages in a browser too: nothing here is private, and nothing is sent with credentials.
function cors(request: Request, headers: Headers) {
  headers.set('Access-Control-Allow-Origin', '*');
  headers.set('Access-Control-Allow-Methods', 'POST, GET, DELETE, OPTIONS');
  headers.set('Access-Control-Allow-Headers', request.headers.get('Access-Control-Request-Headers') ?? 'content-type');
  headers.set('Access-Control-Expose-Headers', 'mcp-session-id, mcp-protocol-version');
  headers.set('Access-Control-Max-Age', '86400');
}

export async function onRequest({ request }: { request: Request }): Promise<Response> {
  if (request.method === 'OPTIONS') {
    const response = new Response(null, { status: 204 });
    cors(request, response.headers);
    return response;
  }
  const answer = await handler.fetch(request);
  const response = new Response(answer.body, answer);
  cors(request, response.headers);
  return response;
}
