// Plain Markdown of every page, for agents: `/llms.txt` (an index),
// `/llms-full.txt` (everything) and `<page>.md`.
import { source } from '@/lib/source';
import { sets } from '../../docs.manifest.mjs';
import { siteUrl } from '@/lib/shared';
import { luauApi } from '@/lib/luau-api';

/** Components whose content agents need as Markdown too. */
const components: Record<string, (attrs: Record<string, string>) => string> = {
  LuauApi: () =>
    luauApi()
      .map((m) => `- \`${m.group}.${m.name}: ${m.type}\`${m.doc ? `\n  ${m.doc}` : ''}`)
      .join('\n'),
};

type Page = ReturnType<typeof source.getPages>[number];
type Node = { type: string; url?: string; children?: Node[]; index?: Node };

/** Each set's pages in sidebar order. */
export function pagesInOrder(): { dir: string; pages: Page[] }[] {
  const byUrl = new Map(source.getPages().map((p) => [p.url, p]));
  const walk = (node: Node, out: Page[]) => {
    if (node.index?.url) out.push(byUrl.get(node.index.url)!);
    if (node.type === 'page' && node.url && byUrl.has(node.url)) out.push(byUrl.get(node.url)!);
    node.children?.forEach((child) => walk(child, out));
    return out;
  };
  const root = source.getPageTree() as unknown as Node;
  return sets.map((set) => {
    const folder = root.children!.find((c) => c.type === 'folder' && (c.index?.url === `/${set.dir}` || c.children?.some((p) => p.url?.startsWith(`/${set.dir}/`))))!;
    return { dir: set.dir, pages: [...new Set(walk(folder, []))] };
  });
}

/** A page's Markdown: its title, description, and body (MDX imports and components left out). */
export function pageMarkdown(page: Page): string {
  const raw = page.data._raw;
  let body = raw.body ?? '';
  if (raw.filePath?.endsWith('.mdx')) {
    body = body
      .replace(/^import .*$/gm, '')
      .replace(/^<([A-Z]\w*)([^>]*)\/>$/gm, (_, name: string, rest: string) => {
        const attrs = Object.fromEntries([...rest.matchAll(/(\w+)="([^"]*)"/g)].map((m) => [m[1], m[2]]));
        return components[name]?.(attrs) ?? `_(${name}: see ${siteUrl}${page.url})_`;
      })
      .replace(/\n{3,}/g, '\n\n');
  }
  const lines = [`# ${page.data.title}`, ''];
  if (page.data.description) lines.push(`> ${page.data.description}`, '');
  lines.push(`Source: ${siteUrl}${page.url}`, '', body.trim(), '');
  return lines.join('\n');
}

export function llmsIndex(): string {
  const out = [
    '# zgx',
    '',
    '> A retained-mode GUI framework in Rust for macOS, Windows and Linux, with Luau scripts that hot reload. Every page below is also Markdown at its URL plus `.md`; all of them in one file: /llms-full.txt. An MCP server lists, searches and reads them at /api/mcp.',
    '',
  ];
  for (const set of sets) {
    out.push(`## ${set.title}`, '', set.description, '');
    for (const page of pagesInOrder().find((s) => s.dir === set.dir)!.pages) {
      out.push(`- [${page.data.title}](${siteUrl}${page.url}.md)${page.data.description ? `: ${page.data.description}` : ''}`);
    }
    out.push('');
  }
  return out.join('\n');
}

export function llmsFull(): string {
  return pagesInOrder()
    .flatMap((s) => s.pages)
    .map(pageMarkdown)
    .join('\n---\n\n');
}
