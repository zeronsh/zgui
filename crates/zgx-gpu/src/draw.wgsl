struct Vertex { @location(0) rect:vec4<f32>, @location(1) uv:vec4<f32>, @location(2) color:vec4<f32>, @location(3) fade:vec4<f32>, @location(4) options:vec4<f32>, @location(5) shape:vec4<f32>, @location(6) border:vec4<f32>, @location(7) mask:vec4<f32> }
// Only `uv` and `point` vary across a quad; the rest are per-quad constants.
// Flat, they cost nothing per pixel (a software rasterizer interpolates every
// smooth varying for every pixel it shades).
struct Out { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) @interpolate(flat) color:vec4<f32>, @location(2) point:vec2<f32>, @location(3) @interpolate(flat) fade:vec4<f32>, @location(4) @interpolate(flat) options:vec4<f32>, @location(5) @interpolate(flat) shape:vec4<f32>, @location(6) @interpolate(flat) border:vec4<f32>, @location(7) @interpolate(flat) mask:vec4<f32> }
// Group 1: one immutable buffer per viewport size, so layers never rewrite it.
@group(1) @binding(0) var<uniform> viewport:vec4<f32>;
@group(0) @binding(1) var atlas:texture_2d<f32>;
// A clip region quads share (`mask.x`: its index plus one, 0 for none): up
// to two rounded boxes (x, y, width, height; a negative width for none)
// with their radii (top-left, top-right, bottom-right, bottom-left), cut
// with antialiased corners; and a box (x0, y0, x1, y1) whose content fades
// out over bands at its edges (top, right, bottom, left), quadratically, as
// Zeron fades transcript edges. Logical units of the target, where the
// pixel is on screen (a transformed quad's `point` is its own).
// `color0..2` (`tint.x` on): its colour filters as rows of an affine map
// of straight RGB (brightness, contrast, saturate), applied as the colour
// is written. `tint.y`, `tint.z`: the rounded boxes' feathers, how far
// inside their edges their content fades out from (0: cut at the edge).
// `tint.w`: a second fade's region (index plus one; 0 for none), the next
// fading ancestor's out, multiplied in (nested fades apply the nearest two).
struct Region { round0:vec4<f32>, radii0:vec4<f32>, round1:vec4<f32>, radii1:vec4<f32>, fade_box:vec4<f32>, fade:vec4<f32>, color0:vec4<f32>, color1:vec4<f32>, color2:vec4<f32>, tint:vec4<f32> }
@group(1) @binding(1) var<storage, read> regions:array<Region>;
fn rounded(b:vec4<f32>, r:vec4<f32>, feather:f32, p:vec2<f32>)->f32 {
 if b.z<0. {return 1.;}
 let half=b.zw*.5; let q=p-b.xy-half;
 let radius=select(select(r.w,r.z,q.x>0.),select(r.x,r.y,q.x>0.),q.y<0.);
 let d=abs(q)-half+vec2(radius); let sdf=length(max(d,vec2(0.)))+min(max(d.x,d.y),0.)-radius;
 if feather>0. {let t=clamp(-sdf/feather,0.,1.); return t*t*(3.-2.*t);}
 return clamp(.5-sdf*viewport.z,0.,1.);
}
// `premultiplied` through region `index`'s colour filters, if any.
fn tinted(index:f32, premultiplied:vec4<f32>)->vec4<f32> {
 if index<.5 {return premultiplied;}
 let g=regions[u32(index)-1u];
 if g.tint.x<.5 || premultiplied.a<=0. {return premultiplied;}
 let c=vec4(premultiplied.rgb/premultiplied.a,1.);
 let rgb=clamp(vec3(dot(g.color0,c),dot(g.color1,c),dot(g.color2,c)),vec3(0.),vec3(1.));
 return vec4(rgb*premultiplied.a,premultiplied.a);
}
fn region(index:f32, position:vec2<f32>)->f32 {
 let g=regions[u32(index)-1u]; let p=position/viewport.z;
 var k=rounded(g.round0,g.radii0,g.tint.y,p)*rounded(g.round1,g.radii1,g.tint.z,p);
 k*=faded(g.fade_box,g.fade,p);
 // Under a second fade (`tint.w`: its record's index plus one), its too.
 if g.tint.w>.5 {let o=regions[u32(g.tint.w)-1u]; k*=faded(o.fade_box,o.fade,p);}
 return k;
}
// Each edge's band eases in (smoothstep): level at both ends, so its inner
// end meets the content without a line (t², still rising there at slope 2,
// showed one).
fn faded(b:vec4<f32>, f:vec4<f32>, p:vec2<f32>)->f32 {
 var k=1.;
 if f.x>0. {let t=clamp((p.y-b.y)/f.x,0.,1.); k*=t*t*(3.-2.*t);}
 if f.y>0. {let t=clamp((b.z-p.x)/f.y,0.,1.); k*=t*t*(3.-2.*t);}
 if f.z>0. {let t=clamp((b.w-p.y)/f.z,0.,1.); k*=t*t*(3.-2.*t);}
 if f.w>0. {let t=clamp((p.x-b.x)/f.w,0.,1.); k*=t*t*(3.-2.*t);}
 return k;
}
@group(0) @binding(2) var tex_sampler:sampler;
@vertex fn vs(v:Vertex,@builtin(vertex_index) i:u32)->Out {
 // A quad is a four-vertex strip, (0,0) (1,0) (0,1) (1,1): its two
 // triangles with two fewer vertex invocations than a list. Corners come
 // from the index's bits: a runtime index into a local array makes Metal
 // reserve per-thread scratch memory for every thread the GPU can run.
 let corner=vec2(f32(i&1u),f32(i>>1u));
 let p=v.rect.xy+corner*v.rect.zw;
 var position=p;
 if v.options.z>0.5 {position=vec2(dot(v.shape.xy,p)+v.border.x,dot(v.shape.zw,p)+v.border.y);}
 // In perspective (`options.z` 2) the corner divides by w, which the GPU
 // does after interpolating, so `uv` and `point` stay true across the quad.
 var w=1.;
 if v.options.z>1.5 {w=max(dot(v.border.zw,p)+1.,1e-4);}
 var o:Out; o.position=vec4(position.x/viewport.x*2.-w,w-position.y/viewport.y*2.,0.,w);o.uv=v.uv.xy+corner*v.uv.zw;o.color=v.color;o.point=p;o.fade=v.fade;o.options=v.options;o.shape=v.shape;o.border=v.border;o.mask=v.mask;return o;
}
fn shade(input:Out)->vec4<f32> { return shade_texel(input,vec4(0.),false); }
// How far `at` (a pixel's centre, device pixels) lies from the middle of
// `box` (logical), each way, in logical units: the box's middle on a 64th
// of a device pixel, so a box moved by whole pixels gives what it gave.
fn centred(at:vec2<f32>, box:vec4<f32>)->vec2<f32> {
 let middle=round((box.xy+box.zw*0.5)*viewport.z*64.)/64.;
 return abs(at-middle)/viewport.z;
}
// `shade` with the texel `given` when `has_texel` (image_effects.wgsl's,
// which works it out), rather than sampled at `uv`.
fn shade_texel(input:Out, given:vec4<f32>, has_texel:bool)->vec4<f32> {
 // Where the pixel is, in logical units: from its own position, so a quad
 // drawn in pieces (`push_panel`) shades every pixel as the whole would. A
 // transformed quad keeps its interpolated, untransformed position.
 var v=input;
 if v.options.z<0.5 {v.point=v.position.xy/viewport.z;}
 var color=v.color;
 if v.options.z<0.5 && (v.shape.w>0.5 || v.shape.z>0.) {
   var corner=v.shape.x;
   if v.options.w>0.5 {
     let right=v.point.x>v.fade.x+v.fade.z*0.5; let bottom=v.point.y>v.fade.y+v.fade.w*0.5;
     corner=select(select(v.border.x,v.border.y,right),select(v.border.w,v.border.z,right),bottom);
   }
   let radius=clamp(corner,0.,min(v.fade.z,v.fade.w)*0.5);
   // From the pixel's own centre in device pixels (exact) to the box's,
   // on a 64th of a pixel: moved by whole pixels, the same numbers.
   let q=centred(v.position.xy,v.fade)-(v.fade.zw*0.5-vec2(radius));
   let distance=length(max(q,vec2(0.)))+min(max(q.x,q.y),0.)-radius;
   // Its slopes a device pixel across and down, from its gradient: what
   // fine derivatives estimate, the same at every pixel (they difference
   // with the neighbour in the pixel's 2×2 block, so a corner moved by an
   // odd number of pixels shaded otherwise, and a scroll's copy of it
   // differed from it drawn where it went).
   let outside=max(q,vec2(0.));
   let reach=length(outside);
   let gradient=select(select(vec2(0.,1.),vec2(1.,0.),q.x>q.y),outside/max(reach,1e-20),reach>0.);
   let slope=gradient/viewport.z;
   if v.shape.y<0. {
     // A ring (a border alone, `ring_quad`): side widths (top, right,
     // bottom, left) packed two to a lane in 16ths of a pixel; inside, the
     // box inset by them, its corners as much rounder as they are thinner.
     let a=u32(v.shape.x); let b=u32(v.shape.z);
     let sides=vec4(f32(a&4095u),f32(a>>12u),f32(b&4095u),f32(b>>12u))/16.;
     let right=v.point.x>v.fade.x+v.fade.z*0.5; let bottom=v.point.y>v.fade.y+v.fade.w*0.5;
     let across=select(sides.w,sides.y,right); let down=select(sides.x,sides.z,bottom);
     let box=vec4(v.fade.xy+sides.wx,max(v.fade.zw-sides.wx-sides.yz,vec2(0.)));
     let inner_radius=clamp(radius-(across+down)*0.5,0.,min(box.z,box.w)*0.5);
     let iq=centred(v.position.xy,box)-(box.zw*0.5-vec2(inner_radius));
     let inside=length(max(iq,vec2(0.)))+min(max(iq.x,iq.y),0.)-inner_radius;
     // Its slopes (above), each pixel's own: Direct3D's `fwidth` is
     // coarse, one per 2×2 block, and softened rounded rings (a pixel 1.4 px
     // inside a 4 px border came out 94% covered).
     let aa=max(slope.x+slope.y,0.5);
     let hollow=select(0.,clamp(0.5-inside/aa,0.,1.),box.z>0.&&box.w>0.);
     color.a*=max(clamp(0.5-distance/aa,0.,1.)-hollow,0.);
   } else if v.shape.z>0. {
     // A box shadow (`shape.z` its sigma, half the CSS blur): the box
     // blurred covers 0.5·erfc(d/(sigma·√2)) at signed distance d, half at
     // its edge (not full: that read as a hard, dark band).
     let x=distance/(v.shape.z*1.41421356);
     let x2=x*x;
     let e=sign(x)*sqrt(1.-exp(-x2*(1.27323954+0.147*x2)/(1.+0.147*x2)));
     color.a*=0.5-0.5*e;
   }
   // A square box's corner on whole pixels is covered as its edges are:
   // `fwidth` (the slopes' sum) adds both axes' there when the pixel's 2×2
   // block reaches outside (a corner at an odd pixel), and shaded 3/4 what
   // the basic shader draws whole, so a box's pixels hung on its batch.
   else {let dx=slope.x;let dy=slope.y;let aa=max(select(dx+dy,max(dx,dy),radius<=0.),0.5);let outer=clamp(0.5-distance/aa,0.,1.);let inner=clamp(0.5-(distance+v.shape.y)/aa,0.,1.);let alpha=color.a*inner+v.border.a*(outer-inner);let rgb=select(vec3(0.),(color.rgb*color.a*inner+v.border.rgb*v.border.a*(outer-inner))/max(alpha,0.00001),alpha>0.);color=vec4(rgb,alpha);}
 }

 // Premultiplied texels carry their alpha already: the colour stays
 // premultiplied from here (`basic`'s product), scaled by whatever later
 // scales alpha, rather than divided by it and multiplied back.
 var premultiplied=false; var rgb=vec3(0.); var scale=1.;
 if v.options.y>0.5 {
   var tex=given; if !has_texel {tex=textureSample(atlas,tex_sampler,v.uv);}
   if v.options.y>1.5 {premultiplied=true; rgb=color.rgb*tex.rgb*color.a; color.a=color.a*tex.a;}
   else {color=vec4(color.rgb*tex.rgb,color.a*tex.a);}
 }
 if v.options.x>0. {let distance=min(v.point-v.fade.xy,v.fade.xy+v.fade.zw-v.point); let f=clamp(min(distance.x,distance.y)/v.options.x,0.,1.); color.a*=f; scale*=f;}
 // Rounded clips and `fade_edges` around it (see `Region`).
 if v.mask.x>.5 {let k=region(v.mask.x,v.position.xy); color.a*=k; scale*=k;}
 return tinted(v.mask.x,select(vec4(color.rgb*color.a,color.a),vec4(rgb*scale,color.a),premultiplied));
}
@fragment fn fs(v:Out)->@location(0) vec4<f32> { return shade(v); }
// Single-pass presentation: the same colour to the retained target and the drawable.
struct Both { @location(0) retained:vec4<f32>, @location(1) drawable:vec4<f32> }
@fragment fn fs_both(v:Out)->Both { let c=shade(v); return Both(c,c); }
// Plain quads (no corners, borders, shadows, fades, regions or transforms):
// most text and backgrounds. A software rasterizer runs a fragment shader on
// the CPU for every pixel, and every branch of `shade` with it, so these use
// this one where drawing is split (see `push_draw`). Same results as `shade`.
// `texture`: 0 for a flat colour, 1 for straight-alpha samples (glyphs), 2
// for premultiplied ones (icons, images), as `options.y`.
struct Basic { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) @interpolate(flat) color:vec4<f32>, @location(2) @interpolate(flat) texture:f32 }
@vertex fn vs_basic(@location(0) rect:vec4<f32>,@location(1) uv:vec4<f32>,@location(2) color:vec4<f32>,@location(4) options:vec4<f32>,@builtin(vertex_index) i:u32)->Basic {
 let corner=vec2(f32(i&1u),f32(i>>1u));
 let p=rect.xy+corner*rect.zw;
 var o:Basic; o.position=vec4(p.x/viewport.x*2.-1.,1.-p.y/viewport.y*2.,0.,1.);o.uv=uv.xy+corner*uv.zw;o.color=color;o.texture=options.y;return o;
}
// Straight samples: colour.rgb * tex.rgb * (colour.a * tex.a). Premultiplied
// ones already carry tex.a in tex.rgb: colour.rgb * tex.rgb * colour.a.
fn basic(v:Basic)->vec4<f32> {
 if v.texture>0.5 {let tex=textureSample(atlas,tex_sampler,v.uv); let a=v.color.a*tex.a; return vec4(v.color.rgb*tex.rgb*select(a,v.color.a,v.texture>1.5),a);}
 return vec4(v.color.rgb*v.color.a,v.color.a);
}
@fragment fn fs_basic(v:Basic)->@location(0) vec4<f32> { return basic(v); }
@fragment fn fs_basic_both(v:Basic)->Both { let c=basic(v); return Both(c,c); }
