// Canvas polyline strokes, appended to draw.wgsl as a module of their own
// (its own pipeline: `shade` and the quads it draws are untouched). A
// quad covers one stroke: `mask.y` is its first point's index plus one in
// `points`, `mask.z` its point count; `border` is its half width, cap
// (0 butt, 1 square, 2 round), join (0 miter, 1 bevel, 2 round) and miter
// limit. Its points run left to right (x never decreasing), so a pixel
// finds the segments within its reach by a binary search, not a walk of
// them all, and shades the stroke's exact shape, antialiased over a device
// pixel as `shade`'s boxes are.
//
// The distance to the line less the half width is the exact signed
// distance to it stroked with round joins and caps: one shape, so its
// depth holds across the segments (pieces measured each alone would meet
// inside it at depth 0, darkening every join). Other joins and caps then
// cut or add near their own point alone.
@group(2) @binding(0) var<storage, read> points:array<vec2<f32>>;

fn zgx_distance(p:vec2<f32>, a:vec2<f32>, b:vec2<f32>)->f32 {
 let pa=p-a; let ba=b-a;
 return length(pa-ba*clamp(dot(pa,ba)/max(dot(ba,ba),1e-12),0.,1.));
}
// `d` with the join at `v` between a segment coming in along `d0` and one
// going out along `d1` mitred or bevelled: round, it is `d` already.
fn zgx_join(d:f32, p:vec2<f32>, v:vec2<f32>, d0:vec2<f32>, d1:vec2<f32>, h:f32, join:f32, limit:f32)->f32 {
 let turn=d0.x*d1.y-d0.y*d1.x;
 if join>1.5 || abs(turn)<1e-6 {return d;}
 // The outside of the turn, where the segments' edges part at `a` and `b`.
 let side=select(1.,-1.,turn>0.);
 let n0=vec2(-d0.y,d0.x)*side; let n1=vec2(-d1.y,d1.x)*side;
 let m=normalize(n0+n1);
 let cos_half=sqrt(max((1.+dot(n0,n1))*.5,0.));
 let q=p-v;
 if join<.5 && cos_half>1e-6 && 1./cos_half<=limit {
  // A miter adds the kite from `v` by `a` to the tip where the edges meet
  // and back by `b`: convex, so inside it the nearest of its sides.
  let a=v+n0*h; let b=v+n1*h; let tip=v+m*(h/cos_half);
  let inside=max(max(dot(q,n0)-h,dot(q,n1)-h),max(-dot(q,d0),dot(q,d1)));
  let kite=select(min(zgx_distance(p,a,tip),zgx_distance(p,tip,b)),inside,inside<=0.);
  return min(d,kite);
 }
 // A bevel cuts the round join's disk beyond the chord from `a` to `b`,
 // and its antialiased edge (a pixel past it): the wedge between the
 // segments' ends, where nothing else lies.
 if dot(q,d0)<0. || dot(q,d1)>0. {return d;}
 return max(d,min(dot(q,m)-h*cos_half,h+1.-length(q)));
}
// `d` with the end at `e`, the stroke leaving it along `out`, butt (the
// round end's half disk cut, and its antialiased edge a pixel past it) or
// square (a box half the width long added).
fn zgx_cap(d:f32, p:vec2<f32>, e:vec2<f32>, out:vec2<f32>, h:f32, cap:f32)->f32 {
 let q=p-e; let along=dot(q,out);
 if cap<.5 {return max(d,min(along,h+1.-length(q)));}
 if cap>1.5 {return d;}
 let box=abs(vec2(along-h*.5,dot(q,vec2(-out.y,out.x))))-vec2(h*.5,h);
 return min(d,length(max(box,vec2(0.)))+min(max(box.x,box.y),0.));
}
// The first index in points[first..first+count] whose x is at least `x`.
fn zgx_seek(first:u32, count:u32, x:f32)->u32 {
 var lo=0u; var hi=count;
 loop {
  if lo>=hi {break;}
  let mid=(lo+hi)/2u;
  if points[first+mid].x<x {lo=mid+1u;} else {hi=mid;}
 }
 return lo;
}
fn zgx_polyline(input:Out)->vec4<f32> {
 let v=input;
 let p=v.position.xy/viewport.z;
 let first=u32(v.mask.y)-1u; let count=u32(v.mask.z);
 let h=v.border.x; let cap=v.border.y; let join=v.border.z; let limit=v.border.w;
 // Nothing past a miter's tip (or a square cap's corner) reaches further.
 let reach=h*max(limit,1.42)+1.;
 let start=zgx_seek(first,count,p.x-reach);
 let lo=select(start-1u,0u,start==0u);
 // At most 32 segments in reach (the canvas checks its strokes' spacing).
 let hi=min(min(zgx_seek(first,count,p.x+reach+1e-3),count-1u),lo+32u);
 var d=1e9;
 for (var k=lo; k<hi; k++) {
  d=min(d,zgx_distance(p,points[first+k],points[first+k+1u]));
 }
 d-=h;
 if join<1.5 {
  for (var k=max(lo,1u); k<hi; k++) {
   let a=points[first+k-1u]; let b=points[first+k]; let c=points[first+k+1u];
   d=zgx_join(d,p,b,normalize(b-a),normalize(c-b),h,join,limit);
  }
 }
 if cap<1.5 {
  if lo==0u {d=zgx_cap(d,p,points[first],normalize(points[first]-points[first+1u]),h,cap);}
  if hi==count-1u {
   let e=points[first+count-1u];
   d=zgx_cap(d,p,e,normalize(e-points[first+count-2u]),h,cap);
  }
 }
 // Antialiased over a device pixel, as `shade`'s boxes are.
 let aa=max(1./viewport.z,.5);
 var color=v.color;
 color.a*=clamp(.5-d/aa,0.,1.);
 if v.mask.x>.5 {color.a*=region(v.mask.x,v.position.xy);}
 return tinted(v.mask.x,vec4(color.rgb*color.a,color.a));
}
@fragment fn fs_polyline(v:Out)->@location(0) vec4<f32> { return zgx_polyline(v); }
@fragment fn fs_polyline_both(v:Out)->Both { let c=zgx_polyline(v); return Both(c,c); }
