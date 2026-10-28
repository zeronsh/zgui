// Where each of the repository's docs appears on the site.
//
// The site has no copies: `scripts/sync-docs.mjs` reads these files at build
// time. Every Markdown file directly in docs/, docs/agents/ and
// docs/reference/ is either placed here or listed in `internal`; the build
// stops on one that is neither, so a new doc is never silently missing.
//
// A page is `src` (a path from the repository root), its `slug` in its set,
// and optionally:
//   title        the sidebar's and the page's title (default: the file's H1)
//   description  the lede and meta description (default: its first sentence)
//   intro        keep the text before the first `##` (default: true unless
//                `sections` is given)
//   sections     keep only these `##` sections, by title (a prefix matches)
//   when         a file the page needs; without it the page is left out
//   unwrap       drop the first section's heading (the page's title) and
//                lift its subsections a level
// Strings starting with `---` are sidebar separators. `page` with `mdx` is a
// hand-written page in website/content-src/: MDX, or Markdown whose
// `<!-- usage FILE CONST -->` and `<!-- lint-rules -->` the sync fills in
// from the source.

export const repo = 'https://github.com/zeronsh/zgx';

/** Links the docs make to what this site now is: [where, what to call it]. */
export const rewrites = {
  'https://zgui-performance.pages.dev': ['/performance', 'the performance results'],
  'https://zgui-performance.pages.dev/': ['/performance', 'the performance results'],
  'https://zgx.zeron.sh': ['/', 'zgx.zeron.sh'],
};
export const branch = 'main';

/** Working notes, evidence and handoffs: not public pages (linked on GitHub). */
export const internal = [
  'docs/ROADMAP.md',
  'docs/MACOS-HANDOFF.md',
  'docs/performance-feature-audit.md',
  'docs/gpui-parity.md',
  'docs/design-3d.md',
];

export const sets = [
  {
    dir: 'rust',
    title: 'zgx',
    description: 'The Rust framework: views, layout, text, windows',
    icon: 'Box',
    pages: [
      '---Start---',
      { slug: 'index', src: 'README.md', title: 'Overview', sections: ['What it has', 'Quick start'], intro: true,
        description: 'A retained-mode GUI framework in Rust for macOS, Windows and Linux.' },
      { slug: 'getting-started', src: 'docs/getting-started.md', title: 'Getting started', intro: true, sections: ['In Rust'], unwrap: true,
        description: 'From nothing to a window, a list, selectable text, a settings window and a shipped app, in Rust.' },
      { slug: 'application', src: 'docs/application.md', title: 'Desktop applications' },
      '---Views---',
      { slug: 'composition', src: 'docs/composition.md', title: 'Components and composition' },
      { slug: 'layout', src: 'docs/layout.md', title: 'Layout' },
      { slug: 'styling', src: 'docs/styling.md', title: 'Styling' },
      { slug: 'paint-styles', src: 'docs/paint-styles.md', title: 'Paint styles',
        description: 'Gradients, brushes, per-corner radii, per-edge borders, border styles and layered shadows.' },
      { slug: 'rich-text', src: 'docs/rich-text.md', title: 'Rich text' },
      { slug: 'canvas', src: 'docs/canvas.md', title: 'Canvas',
        description: 'Retained drawing in a styled view: paths, fills and strokes, redrawn only when what they read changes.' },
      { slug: 'images-and-animation', src: 'docs/images-and-animation.md', title: 'Images and animation' },
      { slug: 'image-transforms', src: 'docs/affine-images.md', title: 'Image transforms',
        description: 'Rotate, scale, reflect, shear and translate images and SVG about their centre, as paint.' },
      '---Behaviour---',
      { slug: 'actions', src: 'docs/actions.md', title: 'Actions and key bindings' },
      { slug: 'scoped-tasks', src: 'docs/scoped-tasks.md', title: 'Scoped tasks' },
      { slug: 'multi-window', src: 'docs/multi-window.md', title: 'Popups, drags and windows' },
      { slug: 'drag-drop', src: 'docs/drag-drop.md', title: 'Drag and drop',
        description: 'Typed drags between views and windows: previews, spring-loaded targets, file drops, and drags out to other apps.' },
      { slug: 'platform-services', src: 'docs/platform-services.md', title: 'Dialogs and menus' },
      '---Internals---',
      { slug: 'architecture', src: 'docs/architecture.md', title: 'Architecture' },
      '---Project---',
      { slug: 'changelog', src: 'CHANGELOG.md', title: 'Changelog',
        description: 'What changed for apps, newest first, with how to migrate.' },
      { slug: 'releasing', src: 'docs/releasing.md', title: 'Releasing' },
    ],
  },
  {
    dir: 'luau',
    title: 'zgx-script',
    description: 'Luau apps that hot reload, and the API reference',
    icon: 'FileCode',
    pages: [
      '---Start---',
      { slug: 'index', src: 'docs/scripting.md', title: 'Overview', intro: true, sections: ['Comparison with Rust views'],
        description: 'Interfaces from Luau scripts: built once, lowered to retained views, hot reloaded with their state kept.' },
      { slug: 'getting-started', src: 'docs/getting-started.md', title: 'Getting started', intro: true, sections: ['In Luau', 'Next'], unwrap: true,
        description: 'From nothing to a window, a list, selectable text, a settings window and a shipped app, in Luau.' },
      { slug: 'coming-from-react', src: 'docs/coming-from-react.md', title: 'Coming from React' },
      { slug: 'app-structure', src: 'docs/reference/app-structure.md', title: 'App structure' },
      '---Guides---',
      { slug: 'types-and-lint', src: 'docs/scripting.md', title: 'Types, check and lint', sections: ['Types', 'Diagnostics'],
        description: 'Type check scripts with zgx check, catch costly patterns with zgx lint, and read what a reload reports.' },
      { slug: 'elements', src: 'docs/scripting.md', title: 'Elements', sections: ['Elements'], unwrap: true,
        description: 'What a script builds from: boxes, text, controls, web views, terminals, state, plugins, testing and shipping.' },
      { slug: 'theming', src: 'docs/scripting.md', title: 'Theming', sections: ['Theming'], unwrap: true,
        description: 'Theme tokens, light and dark, and the system’s accent, contrast and text size.' },
      { slug: 'animation', src: 'docs/scripting.md', title: 'Animation', sections: ['Animation'], unwrap: true,
        description: 'anime.js v4’s API on a Rust engine that ticks once per display frame and sleeps at rest.' },
      { slug: 'design-systems', src: 'docs/reference/design.md', title: 'Design systems' },
      '---API reference---',
      { slug: 'reference', src: 'docs/reference/index.md', title: 'Reading the reference' },
      { slug: 'reference/api', mdx: 'luau/api.mdx' },
      { slug: 'reference/state', src: 'docs/reference/state.md', title: 'State and components' },
      { slug: 'reference/properties', src: 'docs/reference/properties.md', title: 'Properties' },
      { slug: 'reference/events', src: 'docs/reference/events.md', title: 'Events' },
      { slug: 'reference/containers', src: 'docs/reference/containers.md', title: 'Containers' },
      { slug: 'reference/controls', src: 'docs/reference/controls.md', title: 'Controls' },
      { slug: 'reference/text', src: 'docs/reference/text.md', title: 'Text' },
      { slug: 'reference/dynamic', src: 'docs/reference/dynamic.md', title: 'Dynamic content' },
      { slug: 'reference/scrolling', src: 'docs/reference/scrolling.md', title: 'Scrolling and lists' },
      { slug: 'reference/overlays', src: 'docs/reference/overlays.md', title: 'Overlays and menus' },
      { slug: 'reference/media', src: 'docs/reference/media.md', title: 'Images and canvas' },
      { slug: 'reference/3d', src: 'docs/reference/3d.md', title: '3D scenes' },
      { slug: 'reference/native', src: 'docs/reference/native.md', title: 'Web views and terminals' },
      { slug: 'reference/anime', src: 'docs/reference/anime.md', title: 'anime and bindings' },
    ],
  },
  {
    dir: 'platforms',
    title: 'Platforms',
    description: 'macOS, Windows and Linux hosts, and their checks',
    icon: 'MonitorSmartphone',
    pages: [
      { slug: 'index', mdx: 'platforms/index.mdx' },
      '---Hosts---',
      { slug: 'windows', src: 'docs/windows.md', title: 'Windows' },
      { slug: 'linux', src: 'docs/linux.md', title: 'Linux' },
      '---Native surfaces---',
      { slug: 'window-controls', src: 'docs/window-controls.md', title: 'Window controls' },
      { slug: 'native-surfaces', src: 'docs/native-surfaces.md', title: 'CoreVideo surfaces (macOS)',
        description: 'Show CoreVideo frames as a retained, styled view, fitted like an image.' },
      { slug: 'gpu-views', src: 'docs/gpu-views.md', title: 'GPU views (wgpu)',
        description: "An app's own wgpu drawing (3D, a game, video) in a box, composited as an image." },
      '---Evidence---',
      { slug: 'validation', src: 'docs/validation.md', title: 'Validation record' },
    ],
  },
  {
    dir: 'devtools',
    title: 'Devtools',
    description: 'Drive a running app from a shell or over MCP',
    icon: 'SquareTerminal',
    pages: [
      { slug: 'index', src: 'docs/agents/inspect.md', title: 'zgx inspect', intro: true, sections: ['Start'],
        description: 'See and drive a running zgx Luau app: its tree, states and errors; clicks, typing and keys.' },
      { slug: 'targets', src: 'docs/agents/inspect.md', title: 'Targets', sections: ['Targets'], unwrap: true,
        description: 'One selector syntax for every command and for Rust tests: #id, "name", role:, @n, >>, row n, [n].' },
      { slug: 'reading', src: 'docs/agents/inspect.md', title: 'Reading', sections: ['Reading'], unwrap: true,
        description: 'tree, node, find, state, errors, lint, frames and snapshot: what is on screen and why.' },
      { slug: 'acting', src: 'docs/agents/inspect.md', title: 'Acting', sections: ['Acting'], unwrap: true,
        description: 'click, type, key, scroll, command, set, wait and sleep.' },
      { slug: 'gestures', src: 'docs/agents/inspect.md', title: 'Record and replay gestures', sections: ['Gestures'], unwrap: true,
        description: 'Record a trackpad gesture once, replay it through the window’s own input path, and read how its frames landed.' },
      { slug: 'switches', src: 'docs/agents/inspect.md', title: 'Switches', sections: ['Switches'], unwrap: true,
        description: 'Switch the engine’s fast paths off and on in the running app, to compare both ways in one window.' },
      { slug: 'mcp', src: 'docs/agents/inspect.md', title: 'MCP server', sections: ['As MCP tools'], unwrap: true,
        description: 'zgx inspect mcp serves every command as an MCP tool on stdio.' },
      { slug: 'slow-interactions', src: 'docs/agents/inspect.md', title: 'Diagnose a slow interaction', sections: ['Diagnose'], unwrap: true,
        description: 'Find what runs on each frame of a stutter, which script line made it, and what to write instead.' },
      { slug: 'recipes', src: 'docs/agents/inspect.md', title: 'Recipes and safety', sections: ['Recipes', 'Safety and cost'],
        description: 'Common sequences, and what serving devtools costs and exposes.' },
      '---Tools---',
      { slug: 'cli', mdx: 'devtools/cli.md' },
      { slug: 'profiling', src: 'scripts/perf/README.md', title: 'Profiling',
        description: 'ZGX_PROFILE splits a frame by phase; the perf suite compares two revisions in retired instructions.' },
    ],
  },
  {
    dir: 'performance',
    title: 'Performance',
    description: 'Against GPUI, MyGo and Rune, trial by trial',
    icon: 'Gauge',
    pages: [
      { slug: 'index', mdx: 'performance/index.mdx' },
      { slug: 'windows', mdx: 'performance/windows.mdx', when: 'site/windows/data.json' },
      { slug: 'microbenchmarks', mdx: 'performance/microbenchmarks.mdx' },
      { slug: 'gpui', src: 'README.md', title: 'zgx and GPUI', sections: ['Performance'], unwrap: true,
        description: 'The heavy dashboard in zgx, GPUI and GPUI’s fast path on an Apple Silicon Mac, with and without PGO.' },
      '---Method---',
      { slug: 'methodology', src: 'docs/benchmarking.md', title: 'Methodology' },
    ],
  },
];
