const names={zgx:'zgx',gpui:'GPUI',quickgui:'QuickGUI'},colors={zgx:'#588547',gpui:'#7d8daa',quickgui:'#caa572'};
const descriptions={idle:'The interface is visible and unchanged. No streaming or scrolling updates.',stream:'Text arrives continuously while the virtualized list stays still.',scroll:'The 100,000-row virtualized list scrolls while the text stays unchanged.',both:'Text streams while the virtualized list scrolls — both update paths working together.'};
// Each platform's accepted results, and how its memory is measured.
const platforms={
 linux:{base:'',memory:'mean_rss_bytes',peak:'peak_rss_bytes',unit:'MiB · process RSS',heading:'RSS',note:'Average resident memory during the sampled window.',pill:'Linux · llvmpipe',title:'Linux, software rendering',scope:'this software-renderer session',environment:m=>`Mesa llvmpipe on Xvfb. LP_NUM_THREADS=4 per Mesa pool, not per process. ${m.host_note} This run is not a hardware-GPU or macOS result.`},
 macos:{base:'macos/',memory:'mean_footprint_bytes',peak:'peak_footprint_bytes',unit:'MiB · physical footprint',heading:'footprint',note:'Average physical footprint: Activity Monitor’s memory figure, including GPU memory the process owns on the Mac’s unified memory.',pill:'macOS · Metal',title:'macOS, Metal GPU',scope:'this Mac',environment:m=>`${m.environment}. Windows on screen, presenting through Core Animation. ${m.host_note} ${m.memory}`},
};
platforms['macos-heavy']={...platforms.macos,base:'macos-heavy/',pill:'macOS · Metal · heavy dashboard',scope:'this heavy dashboard on this Mac'};
const heavyDescriptions={idle:'The heavy dashboard is visible with all data unchanged.',stream:'Live values, 780 sparkline bars and the log update while the table stays still.',scroll:'The 100,000-row, five-column table scrolls while live data stays unchanged.',both:'Live values, sparklines and the log update while the table scrolls.'};
const platformFromHash=()=>Object.hasOwn(platforms,location.hash.slice(1))?location.hash.slice(1):'linux';
const loaded={};let platform=platformFromHash(),mode='both',data,metadata;
const fmt=(x,d=2)=>Number(x).toLocaleString('en-US',{minimumFractionDigits:d,maximumFractionDigits:d});
const groupsFor=()=>Object.keys(names).map(n=>data.groups.find(g=>g.framework===n&&g.mode===mode)).filter(Boolean);
function graph(metric,scale,container){
 const groups=groupsFor();
 const maximum=Math.max(...groups.map(g=>g[metric].max/scale));const step=maximum>150?50:maximum>50?25:maximum>10?10:maximum>1?1:.05;const ceiling=Math.max(step,Math.ceil(maximum/step)*step);
 const left=80,right=390,w=right-left;let svg='<svg viewBox="0 0 460 232" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">';
 for(let i=0;i<=4;i++){let x=left+w*i/4;svg+=`<line x1="${x}" y1="15" x2="${x}" y2="185" stroke="#e6e9df"/><text x="${x}" y="209" text-anchor="middle" fill="#81887a" font-size="10">${fmt(ceiling*i/4,Number.isInteger(ceiling/4)?0:Number.isInteger(ceiling/4*10)?1:2)}</text>`;}
 groups.forEach((g,i)=>{const m=g[metric],y=30+i*56,x=left+w*m.median/scale/ceiling,xmin=left+w*m.min/scale/ceiling,xmax=left+w*m.max/scale/ceiling;svg+=`<text x="65" y="${y+21}" text-anchor="end" fill="#46513f" font-size="12">${names[g.framework]}</text><rect x="${left}" y="${y}" width="${Math.max(0,x-left)}" height="32" rx="3" fill="${colors[g.framework]}"/><line x1="${xmin}" y1="${y+16}" x2="${xmax}" y2="${y+16}" stroke="#263121" stroke-width="1.4"/><path d="M${xmin},${y+10}v12 M${xmax},${y+10}v12" stroke="#263121" stroke-width="1.4"/><text x="${Math.max(x,xmax)+8}" y="${y+21}" fill="#263121" font-size="12" font-weight="600">${fmt(m.median/scale)}</text>`;});svg+='</svg>';
 const el=document.getElementById(container);el.innerHTML=svg;el.setAttribute('aria-label',groups.map(g=>`${names[g.framework]} ${fmt(g[metric].median/scale)}, range ${fmt(g[metric].min/scale)} to ${fmt(g[metric].max/scale)}`).join('; '));
}
function render(){
 const p=platforms[platform];
 document.querySelectorAll('[data-mode]').forEach(b=>{b.classList.toggle('selected',b.dataset.mode===mode);b.setAttribute('aria-pressed',String(b.dataset.mode===mode));});
 document.querySelectorAll('[data-platform]').forEach(b=>{b.classList.toggle('selected',b.dataset.platform===platform);b.setAttribute('aria-pressed',String(b.dataset.platform===platform));});
 document.getElementById('mode-description').textContent=(platform==='macos-heavy'?heavyDescriptions:descriptions)[mode];
 document.getElementById('framework-legend').innerHTML=groupsFor().map(g=>`<span><b class="${g.framework}"></b>${names[g.framework]}</span>`).join('')+'<small>Median of 3 runs · whiskers show min–max</small>';
 document.getElementById('memory-unit').textContent=p.unit;document.getElementById('memory-note').textContent=p.note;
 document.getElementById('mean-memory-heading').textContent=`Mean ${p.heading}, MiB`;document.getElementById('peak-memory-heading').textContent=`Peak ${p.heading}, MiB`;
 graph('cpu_percent_one_core',1,'cpu-chart');graph(p.memory,1048576,'memory-chart');
 const groups=groupsFor();
 document.getElementById('results-table').innerHTML=groups.map(g=>{let cells=['cpu_percent_one_core',p.memory,p.peak].map((key,i)=>{let s=i?1048576:1,v=g[key];return `<td>${fmt(v.median/s)}<small>${fmt(v.min/s)}–${fmt(v.max/s)}</small></td>`;}).join('');return `<tr class="${g.framework==='zgx'?'highlight':''}"><td>${names[g.framework]}</td>${cells}<td>${mode==='idle'?'—':`${fmt(g.workload_ticks.min/20)}–${fmt(g.workload_ticks.max/20)}`}</td></tr>`;}).join('');
 const z=groups.find(g=>g.framework==='zgx').cpu_percent_one_core.median;
 const compare=(v,n)=>v>0?`<strong>${fmt(Math.abs(100*(1-z/v)),1)}% ${z<=v?'less':'more'} CPU</strong> than ${n}`:`${z===0?'ties':'exceeds'} ${n} at the sampler’s zero-tick floor`;
 const insight=document.getElementById('insight');
 insight.innerHTML=mode==='idle'?`zgx recorded <strong>${fmt(z)}% CPU</strong> while idle. Zero means no CPU-tick increase at the sampler’s resolution, not literally zero work.`:`In this workload, zgx uses ${groups.filter(g=>g.framework!=='zgx').map(g=>compare(g.cpu_percent_one_core.median,names[g.framework])).join(' and ')} by median. These comparisons apply to ${p.scope}.`;
 const rangesOverlap=(a,b)=>a.min<=b.max&&b.min<=a.max;
 const close=groups.slice(1).filter(other=>Math.abs(z-other.cpu_percent_one_core.median)/Math.max(z,other.cpu_percent_one_core.median,0.0001)<0.02&&rangesOverlap(groups[0].cpu_percent_one_core,other.cpu_percent_one_core));
 if(mode!=='idle'&&close.length)insight.innerHTML+=` <strong>CPU is close to ${close.map(g=>names[g.framework]).join(' and ')}</strong> (within 2% by median, with overlapping observed ranges).`;
 const lowest=groups.reduce((a,b)=>a[p.memory].median<b[p.memory].median?a:b);
 insight.innerHTML+=` ${names[lowest.framework]} has the lowest median memory footprint here: <strong>${fmt(lowest[p.memory].median/1048576)} MiB</strong>.`;
 document.getElementById('table-caption').textContent=`${{idle:'Idle',stream:'Streaming text',scroll:'Scrolling list',both:'Both together'}[mode]} — median and observed range across three runs`;
}
function describe(){
 const p=platforms[platform],m=metadata,heavy=platform==='macos-heavy';
 const revision=m.source_dirty?`parent commit ${m.commit.slice(0,7)} + dirty source`:`accepted commit ${m.commit.slice(0,7)}`;
 document.getElementById('scene-intro').textContent=heavy?'A live dashboard with 30 service cards, 780 sparkline bars, a 100,000-row table and streaming log. zgx and GPUI on the same Mac.':'The same streaming text and 100,000-row virtual list. Three Rust GUI frameworks. CPU and memory, side by side.';
 document.getElementById('scene-title').textContent=heavy?'Heavy dashboard · two frameworks':'Streaming and list lab · three frameworks';
 document.getElementById('scene-note').textContent=heavy?'This is a separate, heavier scene; do not compare its bars directly with the lab scene. QuickGUI is excluded because its heavy adapter panics at startup when its text renderer exceeds 32 batches. It is not a zero measurement.':'Matched lab scene within each platform. Linux software rendering and macOS Metal use different machines and memory metrics.';
 document.getElementById('same-work').textContent=heavy?'1280 × 800 dashboard: 6 stat tiles, 30 service cards with 780 sparkline bars, a virtualized 100,000-row, five-column table and a five-line log. Stream updates all live data; scroll moves only the table.':'100,000 fixed-height rows, viewport virtualization and deterministic streaming text. Each framework runs idle, streaming, scrolling and combined modes.';
 document.getElementById('trial-count').textContent=`${m.trials} separate process trials`;
 document.getElementById('measurement-limits').textContent='Active runs must deliver 58–61 logical updates per requested second. That checks work completion, not presented FPS or latency. '+(platform==='linux'?'RSS excludes separate GPU memory and other processes.':'Physical footprint includes process-owned GPU memory. CPU excludes GPU execution and WindowServer compositing.');
 document.getElementById('run-meta').innerHTML=`<span class="pill">${m.date}</span><span class="pill">${p.pill}</span><span class="pill">${revision}</span><span class="pill pass">${m.trials} / ${m.trials} trials accepted</span>`;
 document.getElementById('environment-title').textContent=p.title;document.getElementById('environment').textContent=p.environment(m);
 document.getElementById('footer-date').textContent=`Measured ${m.date}`;
 document.getElementById('evidence-description').textContent=`${revision} · ${m.samples.toLocaleString()} samples · ${fmt(m.updates.min)}–${fmt(m.updates.max)} logical updates/requested sec. ${m.host_note}${m.source_note?' '+m.source_note:''}`;
 for(const [id,file] of [['download-csv','data.csv'],['download-json','data.json'],['download-measurement','measurement.json']])document.getElementById(id).href=p.base+file;
}
function load(){
 const p=platforms[platform];
 loaded[platform]??=Promise.all([fetch(p.base+'data.json').then(r=>{if(!r.ok)throw Error('Summary unavailable');return r.json()}),fetch(p.base+'measurement.json').then(r=>{if(!r.ok)throw Error('Measurement unavailable');return r.json()})]);
 const chosen=platform;
 loaded[chosen].then(([d,m])=>{if(chosen!==platform)return;data=d;metadata=m;describe();render();}).catch(e=>{document.getElementById('run-meta').textContent='Measurement data could not be loaded. Please reload.';console.error(e)});
}
document.querySelectorAll('[data-mode]').forEach(b=>b.addEventListener('click',()=>{mode=b.dataset.mode;if(data)render()}));
document.querySelectorAll('[data-platform]').forEach(b=>b.addEventListener('click',()=>{platform=b.dataset.platform;history.replaceState(null,'',platform==='linux'?location.pathname:'#'+platform);load()}));
window.addEventListener('hashchange',()=>{if(location.hash&&!Object.hasOwn(platforms,location.hash.slice(1)))return;const next=platformFromHash();if(next!==platform){platform=next;load();}});
load();
