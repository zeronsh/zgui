# Docs over MCP

These docs are an MCP server, so an agent can look up zgx's API while it writes an app, rather than guessing from what it remembers. It's public and read-only, and it needs no key:

```text
https://zgx.zeron.sh/api/mcp
```

It speaks streamable HTTP. This is separate from [`zgx inspect mcp`](inspect.md#as-mcp-tools), which drives a running app over stdio; an agent building an app wants both.

## Tools

| Tool | Input | Answer |
| --- | --- | --- |
| `list_pages` | none | Every page, with its URL and a line on what it covers (the site's `/llms.txt`) |
| `search` | `query` | JSON: pages and sections that match, best first, up to 30, each with `url`, `type` and `content` |
| `get_page` | `url` | The page as Markdown |

`get_page` takes a URL as `list_pages` or `search` gives it: `/luau/reference/state`, `/luau/reference/state#routing`, or the whole `https://zgx.zeron.sh/luau/reference/state.md`. An anchor is ignored, and the whole page comes back.

## Setup

**Claude Code:**

```sh
claude mcp add --transport http zgx-docs https://zgx.zeron.sh/api/mcp
```

Add `--scope project` to write it to the repository's `.mcp.json`, for everyone working on the app.

**Codex:**

```sh
codex mcp add zgx-docs --url https://zgx.zeron.sh/api/mcp
```

**Cursor**, in `.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "zgx-docs": { "url": "https://zgx.zeron.sh/api/mcp" }
  }
}
```

**VS Code**, in `.vscode/mcp.json`:

```json
{
  "servers": {
    "zgx-docs": { "type": "http", "url": "https://zgx.zeron.sh/api/mcp" }
  }
}
```

**Any other client** that takes a remote server is given the URL and streamable HTTP. One that only starts stdio servers can reach it through `mcp-remote`:

```json
{
  "mcpServers": {
    "zgx-docs": { "command": "npx", "args": ["-y", "mcp-remote", "https://zgx.zeron.sh/api/mcp"] }
  }
}
```

## Without MCP

The same pages are plain files. `/llms.txt` lists every page, `/llms-full.txt` is all of them in one file, and any page's Markdown is at its URL plus `.md` (`/devtools/targets.md`).
