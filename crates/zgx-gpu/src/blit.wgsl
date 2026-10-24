@group(0) @binding(0) var image:texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32>{let p=vec2(f32((i<<1u)&2u),f32(i&2u))*2.-1.;return vec4(p,0.,1.);}
@fragment fn fs(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32>{var c=textureLoad(image,vec2<i32>(p.xy),0);if POST_ALPHA && c.a>0. {c=vec4(c.rgb/c.a,c.a);}if OUTPUT_SRGB {let rgb=select(c.rgb/12.92,pow((c.rgb+vec3(0.055))/1.055,vec3(2.4)),c.rgb>vec3(0.04045));return vec4(rgb,c.a);}return c;}
