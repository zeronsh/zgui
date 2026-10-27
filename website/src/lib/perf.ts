// The published benchmark (site/data.json and site/micro.jsonl, written and
// checked by scripts/build_site.py), and the same arithmetic the builder
// uses for its headline: every number on the site comes from here.
import perf from '@/generated/perf.json';

export type Stat = { median: number; min: number; max: number };
export type Cell = { framework: string; mode: string; repeats: number } & Record<string, Stat | number | string>;
export type Scene = Omit<(typeof perf.scenes)[number], 'cells'> & { cells: Cell[] };
type MicroRow = {
  framework: string;
  scene: string;
  figure: string;
  render?: boolean;
  raster?: string;
  note?: string;
  median_ns: number;
  p10_ns: number;
  p90_ns: number;
};

export const data = perf;
export const text = perf.text;
export const names = perf.frameworks as Record<string, string>;
export const scenes = perf.scenes as unknown as Scene[];
export const commit = perf.commit;
export const verdict = perf.verdict;
export const repo = 'https://github.com/zeronsh/zgx';

export const scene = (name: string) => scenes.find((s) => s.scene === name)!;
export const cell = (s: Scene, framework: string, mode: string) =>
  s.cells.find((c) => c.framework === framework && c.mode === mode);
export function value(s: Scene, framework: string, mode: string, key: string, field: keyof Stat = 'median') {
  const stat = cell(s, framework, mode)?.[key] as Stat | undefined;
  return stat?.[field];
}

/** As the builder formats: fixed places, thousands separated. */
export function fmt(v: number, digits: number) {
  return v.toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

export function percentLess(ours: number, theirs: number) {
  const change = 1 - ours / theirs;
  return `${Math.abs(change * 100).toFixed(0)}% ${change >= 0 ? 'less' : 'more'}`;
}

const heavy = scene('heavy');
export const rivals = heavy.frameworks.filter((f) => !f.startsWith('zgx'));
const active = scenes.flatMap((s) => s.modes.filter((m) => m !== 'idle').map((m) => [s, m] as const));

/** The headline's figures: scripts/build_site.py's `headline()`, the same arithmetic. */
export function headline() {
  const against = (key: string, mode: string) =>
    rivals.map((r) => ({ name: names[r], text: percentLess(value(heavy, 'zgx', mode, key)!, value(heavy, r, mode, key)!) }));
  const longZgx = Math.max(...active.map(([s, m]) => value(s, 'zgx', m, 'long_interval_fraction')!));
  const longRivals = Math.max(
    ...active.flatMap(([s, m]) => rivals.filter((r) => s.frameworks.includes(r)).map((r) => value(s, r, m, 'long_interval_fraction')!)),
  );
  const led = (key: string) =>
    active.filter(([s, m]) => rivals.filter((r) => s.frameworks.includes(r)).every((r) => value(s, 'zgx', m, key)! < value(s, r, m, key)!)).length;
  return {
    tests: active.length,
    ledCpu: led('cpu_percent_one_core'),
    ledGpu: led('gpu_power_mw'),
    cards: [
      {
        value: `${fmt(value(heavy, 'zgx', 'both', 'cpu_percent_one_core')!, 1)}%`,
        label: 'CPU, heavy dashboard (both)',
        detail: against('cpu_percent_one_core', 'both'),
      },
      {
        value: `${fmt(value(heavy, 'zgx', 'both', 'gpu_power_mw')!, 0)} mW`,
        label: 'GPU power, heavy dashboard (both)',
        detail: against('gpu_power_mw', 'both'),
      },
      {
        value: `${longZgx === 0 ? '0' : (longZgx * 100).toFixed(1)}%`,
        label: 'Long frames (over 25 ms), at most',
        detail: [{ name: 'rivals', text: `up to ${(longRivals * 100).toFixed(1)}%` }],
      },
      {
        value: `${fmt(value(heavy, 'zgx', 'idle', 'cpu_percent_one_core')!, 2)}%`,
        label: 'CPU while idle',
        detail: rivals.map((r) => ({ name: names[r], text: `${fmt(value(heavy, r, 'idle', 'cpu_percent_one_core')!, 2)}%` })),
      },
    ],
  };
}

export type Bar = { framework: string; name: string; median: number; min: number; max: number; places: number };
export type Group = { mode: string; label: string; bars: Bar[] };

/** One metric of one scene, by mode: the builder's `chart()`. */
export function chart(s: Scene, metric: string) {
  const m = text.metrics[metric as keyof typeof text.metrics];
  const modes = s.modes.filter((mode) => m.applies === 'all' || mode !== 'idle');
  const groups: Group[] = modes.map((mode) => ({
    mode,
    label: text.modes[mode as keyof typeof text.modes],
    bars: s.frameworks.flatMap((f) => {
      const stat = cell(s, f, mode)?.[m.source] as Stat | undefined;
      if (!stat) return [];
      // Idle CPU is a fraction of a percent: two places show it.
      const places = metric === 'cpu' && mode === 'idle' ? 2 : m.digits;
      return [{ framework: f, name: names[f], median: stat.median * m.scale, min: stat.min * m.scale, max: stat.max * m.scale, places }];
    }),
  }));
  const top = Math.max(...groups.flatMap((g) => g.bars.map((b) => b.median))) || 1;
  return { title: m.title, unit: m.unit, groups, top };
}

/** The microbenchmarks' frames, by scene and column: the builder's `load_micro()`. */
export function microFrames() {
  const frames: Record<string, Record<string, MicroRow>> = {};
  for (const row of perf.micro as MicroRow[]) {
    if (row.figure !== 'frame' || row.note) continue;
    let column = row.framework;
    if (row.framework === 'zgx' || row.framework === 'zgx-luau') column = row.framework + (row.render ? '' : '-noencode');
    else if (row.framework === 'mygo') column = row.raster === '0.05x' ? 'mygo' : 'mygo-1x';
    (frames[row.scene] ??= {})[column] = row;
  }
  return frames;
}

export const microGroups = [
  { label: 'Without the GPU encode', members: [['zgx-noencode', 'zgx'], ['zgx-luau-noencode', 'zgx (Luau)'], ['gpui-fast', 'GPUI Fast']] },
  { label: 'With the GPU encode', members: [['zgx', 'zgx'], ['zgx-luau', 'zgx (Luau)'], ['rune', 'Rune'], ['mygo', 'MyGo']] },
] as const;

/** Where a scene's trials are: the run directory on GitHub. */
export const runUrl = (s: Scene) => `${repo}/tree/main/${s.run}`;
export const environment = heavy.environment;
export const toolchains = heavy.toolchains as Record<string, any>;
export const protocol = heavy.protocol;
export const date = heavy.started_utc.slice(0, 10);

const sceneShort: Record<string, string> = { heavy: 'Dashboard', lab: 'Text and list', animate: 'Animation', vlist: 'Long list' };

/**
 * Every active test (a scene and a mode), each rival's figure as a multiple
 * of zgx's: the home page's chart. Above 1 a rival uses more than zgx.
 */
export function relativeTests(source: 'cpu_percent_one_core' | 'gpu_power_mw') {
  return active.map(([s, mode]) => {
    const ours = value(s, 'zgx', mode, source)!;
    return {
      scene: sceneShort[s.scene] ?? s.scene,
      mode: s.modes.length > 1 ? text.modes[mode as keyof typeof text.modes] : '',
      zgx: ours,
      rivals: rivals
        .filter((r) => s.frameworks.includes(r))
        .map((r) => ({ framework: r, name: names[r], value: value(s, r, mode, source)!, ratio: value(s, r, mode, source)! / ours })),
    };
  });
}

type StartupStat = { median: number; min: number; max: number };
type Startup = {
  commit: string;
  machine: string;
  probe: string;
  apps: Record<string, string>;
  protocol: string;
  medians: Record<string, Record<'window_ms' | 'content_ms' | 'instructions' | 'cpu_ms', StartupStat>>;
};
/** Launch to first content, zgx against GPUI Fast (data.json's `startup`), when the run has one. */
export const startup = ((perf as Record<string, unknown>).startup ?? null) as Startup | null;

/** The startup comparison's lead: how much sooner, and how many fewer instructions. */
export function startupLead() {
  if (!startup) return null;
  const ours = startup.medians.zgx;
  const theirs = startup.medians['gpui-fast'];
  if (!ours || !theirs) return null;
  return {
    sooner: theirs.content_ms.median - ours.content_ms.median,
    fewer: 1 - ours.instructions.median / theirs.instructions.median,
    content: ours.content_ms.median,
    theirContent: theirs.content_ms.median,
  };
}

type ReleaseChange = { from: number; to: number; change: number };
type Release = {
  gate: {
    from: { version: string; commit: string };
    to: { version: string; commit: string };
    unit: string;
    how: string;
    scenes: Record<string, [number, number, number]>;
  };
  previous_commit: string;
  previous_date: string;
  rebuilt?: string | null;
  changes: { scene: string; mode: string; framework: string; change: Record<string, ReleaseChange> }[];
};
/** Release over release (data.json's `release`): the gate's exact counts and the suite's per-cell change, when the run has one. */
export const release = ((perf as Record<string, unknown>).release ?? null) as Release | null;

/** The field scene (data.json's `field`): a focused text field left alone, run apart from the suite, when there is one. */
export const field = ((perf as Record<string, unknown>).field ?? null) as Scene | null;
