// Single-pass presentation: copy the previous retained frame texel-for-texel
// into the next retained target and the drawable.
@group(0) @binding(0) var previous:texture_2d<f32>;
struct Both { @location(0) retained:vec4<f32>, @location(1) drawable:vec4<f32> }
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let p=vec2(f32((i<<1u)&2u),f32(i&2u))*2.-1.;return vec4(p,0.,1.);}
@fragment fn fs(@builtin(position) p:vec4<f32>)->Both{ let c=textureLoad(previous,vec2<i32>(p.xy),0); return Both(c,c); }
