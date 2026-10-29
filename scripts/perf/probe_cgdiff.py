import re,sys
def load(p):
    d={}
    for line in open(p):
        m=re.match(r'\s*([\d,]+)\s+(?:\([^)]*\)\s+)?(.*)$', line)
        if m:
            name=re.sub(r'\s*\[[^\]]*\]\s*$','',m.group(2).strip())
            name=re.sub(r'::h[0-9a-f]{16}','',name)
            d[name]=d.get(name,0)+int(m.group(1).replace(',',''))
    return d
a=load(sys.argv[1]); b=load(sys.argv[2])
print('total', sum(b.values())-sum(a.values()))
rows=sorted(((b.get(k,0)-a.get(k,0),a.get(k,0),k) for k in set(a)|set(b)), key=lambda r:-abs(r[0]))
for d,base,k in rows[:40]:
    if d: print(f'{d:+9d} {base:11d} {k[:150]}')
