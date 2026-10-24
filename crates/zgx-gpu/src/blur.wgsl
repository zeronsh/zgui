// Backdrop filters, a batch at a time. A filter's area (its visible pixels:
// x0, y0, x1, y1) is blurred at one or more levels, each a slot of a shared
// atlas holding the area, or the part of it the level serves, averaged down
// by a power of two and blurred separably at that size: each sigma blurs
// where it spans 1.5 to 3 texels (more past an eighth of the size), so a
// wide blur costs about what a narrow one does; that coarse a grid loses no
// detail such a wide Gaussian keeps.
// With an edge fade, sigma ramps from 0 at the bounds' edges to full `fade`
// px inside them, quadratically (as `fade_edges` fades content), so the blur
// sets in gently: levels halve sigma down to 2 px, and each pixel blends
// the two levels around its own sigma (below the last, the backdrop as it
// is), a progressive blur rather than the sharp backdrop crossfaded into a
// blurred one.
// Like CSS backdrop filters, samples never leave the area (mirrored at its
// edges): content outside a frosted panel does not bleed into its edge, and
// changes there never make it re-blur. `shape`: corner radii (top-left,
// top-right, bottom-right, bottom-left); a rounded panel blurs only inside
// its curve, antialiased over one pixel.
// `effect`: opacity, fade, sigma (px), and the last level's sigma. `slots`:
// the first level's slot, the number of levels, and the slot holding the
// area's original pixels (where mixed over a copy).
struct Blur { bounds:vec4<f32>, effect:vec4<f32>, area:vec4<f32>, shape:vec4<f32>, slots:vec4<u32> }
// `atlas`: the slot's texels (x, y, width, height); `region`: the target
// pixel its first texel starts at, its factor, and its sigma in texels;
// `owner.x`: its filter.
struct Slot { atlas:vec4<f32>, region:vec4<f32>, owner:vec4<u32> }
@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var original:texture_2d<f32>;
@group(0) @binding(2) var<storage, read> blurs:array<Blur>;
@group(0) @binding(3) var<storage, read> slots:array<Slot>;
@group(0) @binding(4) var linear:sampler;
// A full-target triangle, scissored to a slot or an output; the instance
// picks the slot (filter passes) or the filter (drawing back).
struct Vertex { @builtin(position) p:vec4<f32>, @location(0) @interpolate(flat) index:u32 }
@vertex fn vs(@builtin(vertex_index) i:u32, @builtin(instance_index) index:u32)->Vertex{
 let p=vec2(f32((i<<1u)&2u),f32(i&2u))*2.-1.;return Vertex(vec4(p,0.,1.),index);
}
// Sigma at target pixel `p`.
fn sigma(b:Blur, p:vec2<f32>)->f32{
 if b.effect.y<=0.{return b.effect.z;}
 let d=min(p-b.bounds.xy,b.bounds.xy+b.bounds.zw-p);
 let t=clamp(min(d.x,d.y)/b.effect.y,0.,1.);
 return b.effect.z*t*t;
}
// Each texel: the mean of the area pixels in its factor-wide square, every
// area pixel read once; bilinear samples between pixels average four at a
// time. At factor 1, a copy.
@fragment fn fs_reduce(v:Vertex)->@location(0) vec4<f32>{
 let s=slots[v.index];let b=blurs[s.owner.x];let f=i32(s.region.z);
 let start=vec2<i32>(s.region.xy)+(vec2<i32>(v.p.xy)-vec2<i32>(s.atlas.xy))*f;
 let end=min(start+vec2(f),vec2<i32>(b.area.zw));
 var sum=vec4(0.);
 if f>1&&all(end-start==vec2(f)){
  let size=vec2<f32>(textureDimensions(source));
  for(var y=0;y<f;y+=2){for(var x=0;x<f;x+=2){
   sum+=textureSampleLevel(source,linear,(vec2<f32>(start+vec2(x,y))+1.)/size,0.);
  }}
  return sum*4./f32(f*f);
 }
 for(var y=start.y;y<end.y;y++){for(var x=start.x;x<end.x;x++){sum+=textureLoad(source,vec2(x,y),0);}}
 return sum/f32(max((end.x-start.x)*(end.y-start.y),1));
}
// Texel `x` of `n`, mirrored at the edges (then held there, if the taps
// reach past a whole mirror image of a tiny slot).
fn mirror(x:i32, n:i32)->i32{
 let m=select(x,-1-x,x<0);return clamp(select(m,2*n-1-m,m>=n),0,n-1);
}
// One separable half in a slot: Gaussian weights by recurrence, q^(i*i),
// adjacent taps merged into one bilinear sample where both lie inside it.
fn along(v:Vertex, step:vec2<i32>)->vec4<f32>{
 let s=slots[v.index];let o=vec2<i32>(s.atlas.xy);let n=vec2<i32>(s.atlas.zw);
 let l=vec2<i32>(v.p.xy)-o;let st=s.region.w;
 let r=i32(ceil(3.*st));let q=exp(-.5/(st*st));
 let at=dot(l,step);let extent=dot(n,step);
 let size=vec2<f32>(textureDimensions(source));let axis=vec2<f32>(step);
 var ratio=q;var w=1.;var sum=textureLoad(source,o+l,0);var total=1.;
 for(var i=1;i<=r;i+=2){
  w*=ratio;ratio*=q*q;let wa=w;
  w*=ratio;ratio*=q*q;let wb=select(0.,w,i<r);
  if at-i-1>=0&&at+i+1<extent{
   let offset=axis*(f32(i)+wb/(wa+wb));
   let center=vec2<f32>(o+l)+.5;
   sum+=(textureSampleLevel(source,linear,(center-offset)/size,0.)+textureSampleLevel(source,linear,(center+offset)/size,0.))*(wa+wb);
  }else{
   for(var k=0;k<2;k++){
    let j=i+k;let a=mirror(at-j,extent);let c=mirror(at+j,extent);
    let across=l-step*at;
    sum+=(textureLoad(source,o+across+step*a,0)+textureLoad(source,o+across+step*c,0))*select(wa,wb,k==1);
   }
  }
  total+=2.*(wa+wb);
 }
 return sum/total;
}
@fragment fn fs_across(v:Vertex)->@location(0) vec4<f32>{return along(v,vec2(1,0));}
@fragment fn fs_down(v:Vertex)->@location(0) vec4<f32>{return along(v,vec2(0,1));}
fn coverage(b:Blur, p:vec2<f32>)->f32{
 let half=b.bounds.zw*.5;let q=p-b.bounds.xy-half;
 let r=select(select(b.shape.w,b.shape.z,q.x>0.),select(b.shape.x,b.shape.y,q.x>0.),q.y<0.);
 if r<=0.{return 1.;}
 let d=abs(q)-half+vec2(r);let sdf=length(max(d,vec2(0.)))+min(max(d.x,d.y),0.)-r;
 return clamp(.5-sdf,0.,1.);
}
// How much of the blur replaces the original at `p`.
fn amount(b:Blur, p:vec2<f32>)->f32{return b.effect.x*coverage(b,p);}
// Level `k` at `p`, sampled up bilinearly inside its slot.
fn level(b:Blur, k:u32, p:vec2<f32>)->vec4<f32>{
 let s=slots[b.slots.x+k];
 let t=clamp((p-s.region.xy)/s.region.z-vec2(.5),vec2(0.),s.atlas.zw-vec2(1.));
 return textureSampleLevel(source,linear,(s.atlas.xy+t+.5)/vec2<f32>(textureDimensions(source)),0.);
}
// The blur at `p`, and how much of it replaces the backdrop: between levels
// an octave apart, blended by where sigma falls; below the last level, that
// level faded into the backdrop.
struct Blend { color:vec4<f32>, weight:f32 }
fn blurred(b:Blur, p:vec2<f32>)->Blend{
 let n=b.slots.y;
 if n==1u{return Blend(level(b,0u,p),1.);}
 let s=sigma(b,p);
 if s<=b.effect.w{return Blend(level(b,n-1u,p),s/b.effect.w);}
 let octave=log2(b.effect.z/s);let k=u32(min(floor(octave),f32(n-2u)));
 return Blend(mix(level(b,k,p),level(b,k+1u,p),clamp(octave-f32(k),0.,1.)),1.);
}
// Without dual-source blending: the area's pixels as they were, from their slot.
fn composite(b:Blur, p:vec4<f32>)->vec4<f32>{
 let s=slots[b.slots.z];
 let before=textureLoad(original,vec2<i32>(s.atlas.xy)+vec2<i32>(p.xy-s.region.xy),0);
 let blend=blurred(b,p.xy);
 return mix(before,blend.color,amount(b,p.xy)*blend.weight);
}
@fragment fn fs(v:Vertex)->@location(0) vec4<f32>{return composite(blurs[v.index],v.p);}
// Drawn at presentation: the retained target and the drawable.
struct Both { @location(0) retained:vec4<f32>, @location(1) drawable:vec4<f32> }
@fragment fn fs_both(v:Vertex)->Both{
 let color=composite(blurs[v.index],v.p);return Both(color,color);
}
