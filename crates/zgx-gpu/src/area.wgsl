// Filled areas (`shape.x` 1, `zgx_polyline` calls `zgx_area`), appended to
// polyline.wgsl. Not on Windows: FXC's optimizer recursed on `zgx_area`
// until it overflowed any stack (512 MiB tried), so there it compiles
// area_stub.wgsl, and canvases holding an area rasterize (`AREAS_ON_GPU`).
// A filled area (`shape.x`), by the same search: a polygon whose edges run
// left to right in two chains, `mask.z` points from `mask.y` and `shape.y`
// after them, one above the other at every x, as a chart's area under its
// line is. The nearest of its edges in reach, signed by whether the pixel
// lies between the chains, is the exact distance to the one outline: no
// pieces to meet inside it. Its paint is `color`, or `shape.z` gradient
// stops after the chains (three points each: offset and red, green and
// blue, alpha; premultiplied, as the raster's are) along `border` (a start
// and the way to its end), found for each pixel: nothing to raster or keep.
// `zgx_seek` in as many steps as `count` has bits, the same for every pixel
// of a draw: an area takes six searches a pixel, and llvmpipe never finished
// that many loops ended by a `break` that differs between pixels. Not a
// constant either: FXC unrolls a constant count, six times inlined, until
// its own recursion overflows the stack.
fn zgx_area_seek(first:u32, count:u32, x:f32)->u32 {
 var lo=0u; var hi=count;
 let steps=firstLeadingBit(max(count,1u))+1u;
 for (var step=0u; step<steps; step++) {
  if lo<hi {
   let mid=(lo+hi)/2u;
   if points[first+mid].x<x {lo=mid+1u;} else {hi=mid;}
  }
 }
 return lo;
}
fn zgx_chain_y(first:u32, count:u32, x:f32)->f32 {
 let k=zgx_area_seek(first,count,x);
 if k==0u {return points[first].y;}
 if k>=count {return points[first+count-1u].y;}
 let a=points[first+k-1u]; let b=points[first+k]; let w=b.x-a.x;
 return a.y+(b.y-a.y)*select((x-a.x)/w,1.,w<=0.);
}
fn zgx_chain_near(first:u32, count:u32, p:vec2<f32>, reach:f32)->f32 {
 let start=zgx_area_seek(first,count,p.x-reach);
 let lo=select(start-1u,0u,start==0u);
 let hi=min(min(zgx_area_seek(first,count,p.x+reach+1e-3),count-1u),lo+32u);
 var d=1e9;
 for (var k=lo; k<hi; k++) {
  d=min(d,zgx_distance(p,points[first+k],points[first+k+1u]));
 }
 return d;
}
// Stop by stop, each drawing the colour toward it as far as `t` has passed
// the last: no leaving the loop, so it costs the same for every pixel.
fn zgx_gradient(v:Out, p:vec2<f32>, at:u32, count:u32)->vec4<f32> {
 let t=clamp(dot(p-v.border.xy,v.border.zw)/dot(v.border.zw,v.border.zw),0.,1.);
 let start=points[at];
 var color=vec4(start.y,points[at+1u],points[at+2u].x);
 var last=start.x;
 for (var k=1u; k<count; k++) {
  let s=at+k*3u;
  let stop=points[s];
  let next=vec4(stop.y,points[s+1u],points[s+2u].x);
  let width=stop.x-last;
  color=mix(color,next,select(clamp((t-last)/width,0.,1.),step(stop.x,t),width<=0.));
  last=stop.x;
 }
 return color;
}
fn zgx_area(v:Out)->vec4<f32> {
 let p=v.position.xy/viewport.z;
 let first=u32(v.mask.y)-1u; let top=u32(v.mask.z);
 let second=first+top; let bottom=u32(v.shape.y); let stops=u32(v.shape.z);
 let aa=max(1./viewport.z,.5);
 let reach=aa+1.;
 var d=min(zgx_chain_near(first,top,p,reach),zgx_chain_near(second,bottom,p,reach));
 // The ends between the chains, where they don't meet in a point.
 d=min(d,zgx_distance(p,points[first],points[second]));
 d=min(d,zgx_distance(p,points[first+top-1u],points[second+bottom-1u]));
 let y0=zgx_chain_y(first,top,p.x); let y1=zgx_chain_y(second,bottom,p.x);
 let inside=p.x>=points[first].x && p.x<=points[first+top-1u].x
  && p.y>=min(y0,y1) && p.y<=max(y0,y1);
 let coverage=clamp(.5-select(d,-d,inside)/aa,0.,1.);
 var color=vec4(v.color.rgb*v.color.a,v.color.a);
 if stops>0u {color=zgx_gradient(v,p,second+bottom,stops);}
 color*=coverage;
 if v.mask.x>.5 {color*=region(v.mask.x,v.position.xy);}
 return tinted(v.mask.x,color);
}
