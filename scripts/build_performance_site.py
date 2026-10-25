#!/usr/bin/env python3
"""Copy only accepted, public benchmark aggregates into the static results site."""
import argparse,csv,hashlib,json,math,statistics
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('results',type=Path)
p.add_argument('--output',type=Path,default=Path('site'))
a=p.parse_args();r=a.results
summary=json.loads((r/'summary.json').read_text());audit=json.loads((r/'audit.json').read_text());proof=json.loads((r/'zgx-build-manifest.json').read_text())
trials=list(csv.DictReader((r/'current.csv').open()))
assert len(trials)==36 and len(summary['groups'])==12
frameworks=['zgx','gpui','quickgui'];modes=['idle','stream','scroll','both']
assert {(t['framework'],t['mode'],int(t['repeat'])) for t in trials} == {(f,m,n) for f in frameworks for m in modes for n in range(3)}
assert {(g['framework'],g['mode']) for g in summary['groups']} == {(f,m) for f in frameworks for m in modes}
for group in summary['groups']:
 rows=[t for t in trials if (t['framework'],t['mode'])==(group['framework'],group['mode'])]
 assert group['repeats']==3
 for key in ['cpu_percent_one_core','mean_rss_bytes','peak_rss_bytes','workload_ticks','wall_seconds']:
  values=[float(t[key]) for t in rows]
  assert all(math.isfinite(v) and v>=0 for v in values)
  for field,value in [('median',statistics.median(values)),('min',min(values)),('max',max(values))]:
   assert math.isclose(group[key][field],value,rel_tol=1e-10,abs_tol=1e-10), (group['framework'],group['mode'],key,field)
assert sum(int(t['samples']) for t in trials)==audit['samples']
assert audit['near_60hz_workload_updates'] and audit['trials']==36
assert all(int(t['exit_code'])==0 for t in trials)
assert all(58<=int(t['workload_ticks'])/20<=61 for t in trials if t['mode']!='idle')
assert all(int(t['workload_ticks'])==0 for t in trials if t['mode']=='idle')
a.output.mkdir(exist_ok=True,parents=True)
# Do not publish source archives, host process lists, local paths, or private repo logs.
public={'groups':summary['groups']}
(a.output/'data.json').write_text(json.dumps(public,indent=2)+'\n')
fields=['framework','mode','repeat','wall_seconds','cpu_seconds','cpu_percent_one_core','mean_rss_bytes','peak_rss_bytes','samples','workload_ticks','exit_code']
with (a.output/'data.csv').open('w',newline='') as output:
 writer=csv.DictWriter(output,fieldnames=fields,extrasaction='ignore',lineterminator='\n');writer.writeheader();writer.writerows(trials)
metadata={'date':proof['captured_at_utc'][:10],'commit':proof['commit'],'trials':audit['trials'],'samples':audit['samples'],'updates':audit['active_ticks_per_requested_second'],'environment':'Linux Xvfb, Mesa llvmpipe software Vulkan; LP_NUM_THREADS=4 per pool','host_note':'Shared host; no CPU affinity or thermal isolation. See methodology for limits.','protocol':{'requested_seconds':20,'warmup_seconds':5,'repeats':3,'acceptance_updates_per_second':[58,61]},'frameworks':{'gpui':'0.2.2','quickgui':'811d6e2816d5229711f59683c4c9dfbb6fc74133','zgx':proof['commit']},'csv_sha256':hashlib.sha256((a.output/'data.csv').read_bytes()).hexdigest(),'source_csv_sha256':hashlib.sha256((r/'current.csv').read_bytes()).hexdigest(),'binaries':{}}
for file in ['zgx-build-manifest.json','reference-build-manifest.json']:
 for name,b in json.loads((r/file).read_text())['binaries'].items():metadata['binaries'][name]={'sha256':b['sha256'],'bytes':b['bytes']}
independent=r/'independent-audit.json'
if independent.exists():
 checked=json.loads(independent.read_text());assert checked['status']=='passed'
 metadata['host_note']=('Unrelated workspace builds occurred during sampling; this shared host was not isolated.' if checked.get('observed_build_process_names') else 'No build processes were observed, but this shared host was not isolated.')
(a.output/'measurement.json').write_text(json.dumps(metadata,indent=2)+'\n')
print('Prepared accepted public results for',metadata['commit'])
