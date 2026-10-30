import re,sys
t=open(sys.argv[1],errors='replace').read()
blocks=re.split(r'\n(?=(?:method call|signal|method return|error) time=)',t)
ev=[]
for b in blocks:
    h=b.split('\n',1)[0]; m=re.match(r'(\S+(?: \S+)?) time=([\d.]+).*?member=(\w+)',h)
    ts=re.search(r'time=([\d.]+)',h)
    if 'member=ProcessKeyEvent' in h:
        u=re.findall(r'uint32 (\d+)',b)
        ev.append((float(ts[1]),'KEY',chr(int(u[0])) if 32<=int(u[0])<127 else hex(int(u[0])),'up' if len(u)>2 and int(u[2])&(1<<30) else 'down'))
    elif 'member=UpdatePreeditText' in h:
        s=re.findall(r'string "([^"]*)"',b)
        vis='true' if 'boolean true' in b else 'false'
        ev.append((float(ts[1]),'PREEDIT',[x for x in s if x not in('IBusText','IBusAttrList','IBusAttribute')],vis))
    elif re.search(r'member=(CommitText|ForwardKeyEvent|Reset|FocusIn|FocusOut|SetCursorLocation)\b',h) and 'SetCursorLocation' not in h:
        ev.append((float(ts[1]),re.search(r'member=(\w+)',h)[1],'',''))
t0=ev[0][0]
want=sys.argv[2] if len(sys.argv)>2 else None
keys=[i for i,e in enumerate(ev) if e[1]=='KEY' and e[2]=='w' and e[3]=='down']
for k in keys[-1:]:
    for e in ev[max(0,k-3):k+14]: print(f'{e[0]-t0:8.3f}',e[1],e[2],e[3])
