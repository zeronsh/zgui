'use strict';
// zgx on macOS: grouped bar charts of every accepted scene (data.json, built
// by scripts/build_compare_site.py). Plain SVG; labels go in via textContent.
const SVG = 'http://www.w3.org/2000/svg';
const SERIES = { zgx: '--series-1', gpui: '--series-2', 'gpui-fast': '--series-3', quickgui: '--series-4' };
const MODES = { idle: 'Idle', stream: 'Streaming', scroll: 'Scrolling', both: 'Both', animate: 'Animating' };
const SCENES = {
  lab: {
    name: 'Lab: text + 100k-row list',
    note: 'A streaming transcript beside a 100,000-row virtual list: idle, streaming, scrolling, and both at once.',
    lead: 'both',
  },
  heavy: {
    name: 'Heavy dashboard',
    note: 'A 1280×800 dashboard: 6 tiles, 30 cards of 26 sparkline bars, a status table and a log. Most values change 60 times a second; the table scrolls.',
    lead: 'stream',
  },
  animate: {
    name: 'Animation',
    note: '48 blinking dots and a shimmering line of text, animated at the display’s rate (120 Hz). Frameworks draw as often as they manage to present, so per-frame costs are shown too.',
    lead: 'animate',
  },
};

const el = (tag, attrs = {}, text) => {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  if (text !== undefined) node.textContent = text;
  return node;
};
const svgEl = (tag, attrs = {}) => {
  const node = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  return node;
};
const color = (framework) => `var(${SERIES[framework]})`;
const MIB = 1024 * 1024;

// A metric of a group: {median, min, max}, transformed by `f`.
const stat = (entry, key, f = (v) => v) => {
  const s = entry && entry[key];
  return s ? { median: f(s.median), min: f(s.min), max: f(s.max) } : null;
};
// Per-frame values are derived from each run's own frame rate.
const perFrame = (group, key, scale) => {
  const value = stat(group, key);
  const fps = group.reported_fps && group.reported_fps.median;
  if (!value || !fps) return null;
  const f = (v) => (v * scale) / fps;
  return { median: f(value.median), min: f(value.min), max: f(value.max) };
};

const METRICS = [
  { id: 'cpu', title: 'CPU', unit: '% of one core', digits: 2,
    get: (s, g) => stat(g, 'cpu_percent_one_core'),
    caption: 'The app’s own CPU time, user and system, over the sampled 15 seconds.' },
  { id: 'gpu-work', title: 'GPU work', unit: 'ms of GPU execution / second', digits: 1, skipIdle: true,
    get: (s, g, p, t) => stat(t, 'gpu_busy_ms_per_second'),
    caption: 'How long the GPU ran the app’s vertex and fragment work each second, from Metal System Trace (separate 5-second captures).' },
  { id: 'gpu', title: 'GPU time (IOKit)', unit: 'megacycles / second', digits: 1, skipIdle: true,
    get: (s, g) => stat(g, 'gpu_mcycles_per_second'),
    caption: 'The app’s accumulated GPU time × the GPU’s mean active clock. Includes each command buffer’s fixed cost, so it favours drawing fewer frames; noisy on this desktop.' },
  { id: 'memory', title: 'Memory', unit: 'MiB physical footprint', digits: 0,
    get: (s, g) => stat(g, 'mean_footprint_bytes', (v) => v / MIB),
    caption: 'Mean footprint while sampled, as Activity Monitor reports it. ~180–190 MiB of it is the Metal driver’s, in every GPU app.' },
  { id: 'pacing', title: 'Frame pacing', unit: 'ms, 99th-percentile frame interval', digits: 1, skipIdle: true, pacing: true,
    get: (s, g, p) => stat(p, 'interval_p99_ms'),
    caption: 'Intervals between frames that reached the screen, from a capture of each window.' },
  { id: 'late', title: 'Late frames', unit: '% of frame intervals over 25 ms', digits: 1, skipIdle: true, except: 'animate',
    get: (s, g, p) => stat(p, 'long_interval_fraction', (v) => v * 100),
    caption: 'Updates come 60 times a second, one every 16.7 ms: an interval past 25 ms means a frame missed its refresh. Steadier than the 99th percentile, which where an app’s timer falls against the refresh moves by 5 ms or more from capture to capture.' },
  { id: 'fps', title: 'Frames drawn', unit: 'frames / second', digits: 0, only: 'animate', higher: true,
    get: (s, g) => stat(g, 'reported_fps'),
    caption: 'Frames each app drew while sampled. Higher is better: the display refreshes at 120 Hz.' },
  { id: 'cpu-frame', title: 'CPU per frame', unit: 'ms', digits: 3, only: 'animate',
    get: (s, g) => perFrame(g, 'cpu_percent_one_core', 10),
    caption: 'CPU time per drawn frame: the CPU above divided by the frames drawn.' },
  { id: 'gpu-frame', title: 'GPU work per frame', unit: 'µs of GPU execution per command buffer', digits: 0, only: 'animate',
    get: (s, g, p, t) => stat(t, 'gpu_busy_us_per_command_buffer'),
    caption: 'GPU execution per command buffer, from the same traces. Each app submits one command buffer a frame here.' },
];

let data, scene, tooltip;
// Whether a metric applies to the selected scene.
const shown = (m) => (!m.only || m.only === scene.scene) && m.except !== scene.scene;

function niceMax(value) {
  if (!(value > 0)) return 1;
  const exponent = Math.pow(10, Math.floor(Math.log10(value)));
  for (const step of [1, 1.2, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10]) {
    if (step * exponent >= value) return step * exponent;
  }
  return 10 * exponent;
}
function ticks(max) {
  // Steps of 1, 2 or 5 times a power of ten, about five of them.
  const rough = max / 5;
  const magnitude = Math.pow(10, Math.floor(Math.log10(rough)));
  const residual = rough / magnitude;
  const step = (residual > 5 ? 10 : residual > 2 ? 5 : residual > 1 ? 2 : 1) * magnitude;
  const out = [];
  for (let v = 0; v <= max + step * 1e-6; v += step) out.push(+v.toFixed(10));
  return out;
}
const decimals = (step) => Math.max(0, -Math.floor(Math.log10(step) + 1e-9));
const format = (v, digits) => v.toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits });
const percent = (ratio) => `${ratio < 0 ? '−' : '+'}${Math.abs(Math.round(ratio * 100))}%`;

function entries(metric) {
  const s = scene;
  const modes = s.modes.filter((m) => !(metric.skipIdle && m === 'idle'));
  return modes.map((mode) => ({
    mode,
    bars: s.frameworks.map((framework) => {
      const group = s.groups.find((g) => g.framework === framework && g.mode === mode);
      const pacing = s.pacing.find((p) => p.framework === framework && p.mode === mode);
      const trace = (s.gpu_trace || []).find((t) => t.framework === framework && t.mode === mode);
      return { framework, value: group ? metric.get(s, group, pacing, trace) : null };
    }).filter((b) => b.value),
  })).filter((row) => row.bars.length);
}

function chart(metric) {
  const rows = entries(metric);
  if (!rows.length) return null;
  const card = el('article', { class: 'chart-card' });
  const title = el('div', { class: 'title' });
  title.append(el('h3', {}, metric.title), el('span', { class: 'unit' }, `${metric.unit} · ${metric.higher ? 'higher' : 'lower'} is better`));
  card.append(title);

  const W = 600, left = 92, right = 64, bar = 12, gap = 2, groupGap = 16, axis = 26;
  const top = 6;
  const max = niceMax(Math.max(...rows.flatMap((r) => r.bars.map((b) => b.value.max))) * 1.04);
  const x = (v) => left + (v / max) * (W - left - right);
  const groupHeight = (n) => n * bar + (n - 1) * gap;
  const height = top + rows.reduce((h, r) => h + groupHeight(r.bars.length) + groupGap, 0) - groupGap + axis;
  const svg = svgEl('svg', { viewBox: `0 0 ${W} ${height}`, role: 'img' });
  svg.setAttribute('aria-label', `${metric.title}, ${metric.unit}, by mode and framework`);

  const plotBottom = height - axis;
  const marks = ticks(max);
  const places = marks.length > 1 ? decimals(marks[1]) : 0;
  for (const t of marks) {
    svg.append(svgEl('line', { class: t === 0 ? 'base' : 'grid', x1: x(t), x2: x(t), y1: top - 4, y2: plotBottom + 2 }));
    const label = svgEl('text', { x: x(t), y: plotBottom + 18, 'text-anchor': 'middle' });
    label.textContent = format(t, places);
    svg.append(label);
  }

  let y = top;
  for (const row of rows) {
    const h = groupHeight(row.bars.length);
    const name = svgEl('text', { class: 'row', x: left - 10, y: y + h / 2 + 4, 'text-anchor': 'end' });
    name.textContent = MODES[row.mode] || row.mode;
    svg.append(name);
    const zgx = row.bars.find((b) => b.framework === 'zgx');
    for (const b of row.bars) {
      const { median, min, max: high } = b.value;
      const x0 = x(0), x1 = Math.max(x(median), x0 + 1);
      const r = Math.min(4, (x1 - x0) / 2);
      // Rounded at the data end, square at the baseline.
      const path = `M${x0},${y} H${x1 - r} Q${x1},${y} ${x1},${y + r} V${y + bar - r} Q${x1},${y + bar} ${x1 - r},${y + bar} H${x0} Z`;
      const hit = svgEl('rect', { class: 'hit', x: 0, y: y - gap / 2, width: W, height: bar + gap, tabindex: 0 });
      const label = `${data.names[b.framework]}, ${MODES[row.mode] || row.mode}: ${format(median, metric.digits)} (${format(min, metric.digits)}–${format(high, metric.digits)})`;
      hit.setAttribute('aria-label', label);
      const shape = svgEl('path', { class: 'bar', d: path, fill: color(b.framework) });
      shape.style.pointerEvents = 'none';
      const whiskerY = y + bar / 2;
      const whisker = svgEl('g', { class: 'whisker' });
      whisker.style.pointerEvents = 'none';
      whisker.append(
        svgEl('line', { x1: x(min), x2: x(high), y1: whiskerY, y2: whiskerY }),
        svgEl('line', { x1: x(min), x2: x(min), y1: whiskerY - 3, y2: whiskerY + 3 }),
        svgEl('line', { x1: x(high), x2: x(high), y1: whiskerY - 3, y2: whiskerY + 3 }),
      );
      svg.append(hit, shape, whisker);
      // Direct labels on zgx only; the rest are in the tooltip and table.
      if (b.framework === 'zgx') {
        const value = svgEl('text', { class: 'value', x: Math.max(x(high), x1) + 6, y: y + bar - 2 });
        value.textContent = format(median, metric.digits);
        value.style.pointerEvents = 'none';
        svg.append(value);
      }
      const show = (event) => showTooltip(event, metric, row.mode, b, zgx, shape);
      hit.addEventListener('pointermove', show);
      hit.addEventListener('focus', show);
      hit.addEventListener('pointerleave', () => hideTooltip(shape));
      hit.addEventListener('blur', () => hideTooltip(shape));
      y += bar + gap;
    }
    y += groupGap - gap;
  }
  card.append(svg, el('p', { class: 'caption' }, metric.caption));
  return card;
}

function showTooltip(event, metric, mode, b, zgx, shape) {
  shape.style.opacity = 0.75;
  tooltip.replaceChildren();
  tooltip.append(el('div', { class: 'value' }, `${format(b.value.median, metric.digits)}`));
  const who = el('div', { class: 'who' });
  const key = el('i');
  key.style.background = color(b.framework);
  who.append(key, document.createTextNode(`${data.names[b.framework]} · ${MODES[mode] || mode}`));
  tooltip.append(who);
  tooltip.append(el('div', { class: 'muted' }, `${metric.unit}, range ${format(b.value.min, metric.digits)}–${format(b.value.max, metric.digits)}`));
  if (zgx && b.framework !== 'zgx' && zgx.value.median > 0) {
    const ratio = b.value.median / zgx.value.median - 1;
    tooltip.append(el('div', { class: 'muted' }, `${percent(ratio)} vs zgx`));
  }
  tooltip.hidden = false;
  const rect = event.clientX !== undefined && event.type === 'pointermove'
    ? { x: event.clientX, y: event.clientY }
    : (() => { const r = event.target.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top }; })();
  const w = tooltip.offsetWidth, h = tooltip.offsetHeight;
  tooltip.style.left = `${Math.min(window.innerWidth - w - 8, rect.x + 14)}px`;
  tooltip.style.top = `${Math.max(8, rect.y - h - 10)}px`;
}
function hideTooltip(shape) {
  shape.style.opacity = '';
  tooltip.hidden = true;
}

// Headline tiles: zgx against each rival in the scene's lead mode.
function tiles() {
  const box = document.getElementById('tiles');
  box.replaceChildren();
  const lead = SCENES[scene.scene].lead;
  // GPU work where traced; IOKit's GPU time otherwise.
  for (const id of ['cpu', scene.gpu_trace ? 'gpu-work' : 'gpu', 'memory']) {
    const metric = METRICS.find((m) => m.id === id);
    const row = entries(metric).find((r) => r.mode === lead);
    const zgx = row && row.bars.find((b) => b.framework === 'zgx');
    if (!zgx) continue;
    const tile = el('div', { class: 'tile' });
    tile.append(el('p', { class: 'label' }, `${metric.title} · ${MODES[lead] || lead}`));
    tile.append(el('p', { class: 'value' }, `${format(zgx.value.median, metric.digits)}`));
    const rivals = row.bars.filter((b) => b.framework !== 'zgx').map((b) => {
      const ratio = zgx.value.median / b.value.median - 1;
      return `${percent(ratio)} vs ${data.names[b.framework]}`;
    });
    tile.append(el('p', { class: 'detail' }, `${metric.unit} · zgx ${rivals.join(' · ')}`));
    box.append(tile);
  }
}

function table() {
  const t = document.getElementById('table');
  t.replaceChildren();
  const columns = METRICS.filter((m) => shown(m) && (m.id !== 'gpu-work' || scene.gpu_trace));
  const head = el('tr');
  head.append(el('th', {}, 'Mode'), el('th', {}, 'Framework'));
  for (const m of columns) head.append(el('th', {}, `${m.title} (${m.unit})`));
  const thead = el('thead');
  thead.append(head);
  const tbody = el('tbody');
  for (const mode of scene.modes) {
    for (const framework of scene.frameworks) {
      const group = scene.groups.find((g) => g.framework === framework && g.mode === mode);
      const pacing = scene.pacing.find((p) => p.framework === framework && p.mode === mode);
      const trace = (scene.gpu_trace || []).find((t) => t.framework === framework && t.mode === mode);
      if (!group) continue;
      const tr = el('tr', { class: framework === 'zgx' ? 'zgx' : '' });
      tr.append(el('td', {}, MODES[mode] || mode));
      const name = el('td');
      const key = el('span', { class: 'key' });
      key.style.background = color(framework);
      name.append(key, document.createTextNode(data.names[framework]));
      tr.append(name);
      for (const m of columns) {
        const v = m.get(scene, group, pacing, trace);
        const td = el('td');
        if (v && !(m.skipIdle && mode === 'idle')) {
          td.append(document.createTextNode(format(v.median, m.digits)));
          td.append(el('span', { class: 'range' }, `${format(v.min, m.digits)}–${format(v.max, m.digits)}`));
        } else {
          td.textContent = '—';
        }
        tr.append(td);
      }
      tbody.append(tr);
    }
  }
  t.append(thead, tbody);
}

function render() {
  for (const button of document.querySelectorAll('#scenes button')) {
    button.setAttribute('aria-pressed', String(button.dataset.scene === scene.scene));
  }
  document.getElementById('scene-note').textContent = SCENES[scene.scene].note;
  const legend = document.getElementById('legend');
  legend.replaceChildren();
  for (const framework of scene.frameworks) {
    const key = el('span', { class: 'key' });
    const swatch = el('span', { class: 'swatch' });
    swatch.style.background = color(framework);
    key.append(swatch, document.createTextNode(data.names[framework]));
    legend.append(key);
  }
  legend.append(el('small', {}, 'Median of 3 runs (frame pacing: 8) · whiskers: min–max'));
  const charts = document.getElementById('charts');
  charts.replaceChildren(...METRICS.filter(shown).map(chart).filter(Boolean));
  tiles();
  table();
  const run = scene.run;
  const env = run.environment;
  document.getElementById('run').textContent =
    `${env.chip} (${env.machine}), macOS ${env.macos}, ${env.power} · measured ${run.started_utc.slice(0, 10)} · ` +
    `${run.trials} timed trials, ${run.pacing_trials} pacing trials in this workload`;
  document.getElementById('source').textContent =
    `zgx at ${run.commit.slice(0, 7)}${run.source_dirty ? ` with uncommitted changes (diff sha256 ${run.source_diff_sha256.slice(0, 12)}…)` : ''}. ` +
    'GPUI and GPUI Fast (longbridge/gpui-fast 0.1.4) run the same workloads through adapters in the zgx repository’s comparisons directory.';
}

async function main() {
  tooltip = document.getElementById('tooltip');
  data = await (await fetch('data.json')).json();
  const tabs = document.getElementById('scenes');
  for (const s of data.scenes) {
    const button = el('button', { 'data-scene': s.scene, 'aria-pressed': 'false' }, SCENES[s.scene]?.name || s.scene);
    button.addEventListener('click', () => {
      scene = s;
      history.replaceState(null, '', `#${s.scene}`);
      render();
    });
    tabs.append(button);
  }
  const wanted = location.hash.slice(1);
  scene = data.scenes.find((s) => s.scene === wanted) || data.scenes.find((s) => s.scene === 'heavy') || data.scenes[0];
  render();
}
main();
