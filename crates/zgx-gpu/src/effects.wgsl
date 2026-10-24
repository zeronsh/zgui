// Layer effects (`Effects::filter_blur`, `Scene::set_mask`): a cached layer's
// texture blurred and masked into its slot of the effect atlas, which then
// composites as an image. A blur runs across into a scratch atlas, then down
// into the effect atlas, masking as it goes; a mask alone is the down pass
// with nothing to blur. Every job is a quad over its slot (and a texel of
// gutter, written transparent) in the target, its parameters per instance:
// `rect`: the quad (target texels); `uv.xy`: the target's size.
// Across, `color`: where the quad's first texel starts in the source
// (texels), the factor the slot is averaged down by, and sigma in slot
// texels. Down, `color.xy`: the source texel under the quad's first texel,
// and `color.w` sigma; `options`: the source texels it may read (x0, y0, x1,
// y1), outside which the source is transparent.
// `fade`: the slot's texels (x0, y0, x1, y1), outside which it is clear.
// `shape`, `border.x` on: the mask texel at the quad's origin and its step
// per slot texel; the content shows by its alpha, and not past its edges.
struct Job { @location(0) rect:vec4<f32>, @location(1) uv:vec4<f32>, @location(2) color:vec4<f32>, @location(3) fade:vec4<f32>, @location(4) options:vec4<f32>, @location(5) shape:vec4<f32>, @location(6) border:vec4<f32> }
struct Out { @builtin(position) position:vec4<f32>, @location(0) @interpolate(flat) rect:vec4<f32>, @location(1) @interpolate(flat) color:vec4<f32>, @location(2) @interpolate(flat) fade:vec4<f32>, @location(3) @interpolate(flat) options:vec4<f32>, @location(4) @interpolate(flat) shape:vec4<f32>, @location(5) @interpolate(flat) border:vec4<f32> }
@group(0) @binding(1) var source:texture_2d<f32>;
@group(0) @binding(2) var linear:sampler;
@group(1) @binding(0) var mask:texture_2d<f32>;
@vertex fn vs(job:Job,@builtin(vertex_index) i:u32)->Out {
 let corner=vec2(f32(i&1u),f32(i>>1u));
 let p=(job.rect.xy+corner*job.rect.zw)/job.uv.xy;
 var o:Out; o.position=vec4(p.x*2.-1.,1.-p.y*2.,0.,1.);
 o.rect=job.rect;o.color=job.color;o.fade=job.fade;o.options=job.options;o.shape=job.shape;o.border=job.border;return o;
}
fn inside(v:Out)->bool{return all(v.position.xy>=v.fade.xy)&&all(v.position.xy<v.fade.zw);}
// Across: per tap, the mean of the factor-wide square of source texels it
// stands for (bilinear samples on texel corners average four at a time),
// taps a factor apart weighted by the Gaussian. Past the layer's content its
// texture is transparent, as are the clamped edges.
@fragment fn fs_across(v:Out)->@location(0) vec4<f32>{
 if !inside(v){return vec4(0.);}
 let f=v.color.z;let s=max(v.color.w,.05);let r=i32(ceil(3.*s));
 let size=vec2<f32>(textureDimensions(source));
 let center=v.color.xy+(v.position.xy-v.rect.xy)*f;
 let n=max(i32(f)/2,1);let first=center-vec2(f*.5)+select(vec2(.5),vec2(1.),f>1.5);
 var sum=vec4(0.);var total=0.;
 for(var k=-r;k<=r;k++){
  let w=exp(-.5*f32(k*k)/(s*s));
  var block=vec4(0.);
  for(var y=0;y<n;y++){for(var x=0;x<n;x++){
   block+=textureSampleLevel(source,linear,(first+vec2(f32(k)*f+f32(2*x),f32(2*y)))/size,0.);
  }}
  sum+=block*(w/f32(n*n));total+=w;
 }
 return sum/total;
}
// Down: the Gaussian down the column, then the mask.
@fragment fn fs_down(v:Out)->@location(0) vec4<f32>{
 if !inside(v){return vec4(0.);}
 let s=v.color.w;let r=select(0,i32(ceil(3.*s)),s>0.);
 let at=vec2<i32>(floor(v.position.xy-v.rect.xy+v.color.xy));
 let range=vec4<i32>(v.options);
 var sum=vec4(0.);var total=0.;
 if at.x>=range.x&&at.x<range.z{
  for(var k=-r;k<=r;k++){
   let w=select(1.,exp(-.5*f32(k*k)/(s*s)),r>0);
   let y=at.y+k;
   if y>=range.y&&y<range.w{sum+=textureLoad(source,vec2(at.x,y),0)*w;}
   total+=w;
  }
 }else{total=1.;}
 var color=sum/total;
 if v.border.x>.5{
  let m=floor(v.shape.xy+(v.position.xy-v.rect.xy)*v.shape.zw);
  let dims=vec2<f32>(textureDimensions(mask));
  if all(m>=vec2(0.))&&all(m<dims){color*=textureLoad(mask,vec2<i32>(m),0).a;}else{color=vec4(0.);}
 }
 return color;
}
