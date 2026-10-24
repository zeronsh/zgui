// Copy texels from another texture of the same size, shifted: a surface
// catching up with the shown one (no shift) or a scroll's move. One instance
// per rectangle: its device-pixel rect and, in place of uv, the shift.
@group(0) @binding(0) var source:texture_2d<f32>;
struct Out { @builtin(position) position:vec4<f32>, @location(0) @interpolate(flat) shift:vec2<i32> }
@vertex fn vs(@builtin(vertex_index) i:u32,@location(0) rect:vec4<f32>,@location(1) shift:vec4<f32>)->Out{
 // Corners of two triangles: (0,0) (1,0) (0,1) (0,1) (1,0) (1,1).
 let corner=vec2(f32(i&1u),f32(i>>1u));
 let pixel=rect.xy+corner*rect.zw;
 let size=vec2<f32>(textureDimensions(source));
 var o:Out; o.position=vec4(pixel.x/size.x*2.-1.,1.-pixel.y/size.y*2.,0.,1.);
 o.shift=vec2<i32>(shift.xy); return o;
}
@fragment fn fs(v:Out)->@location(0) vec4<f32>{ return textureLoad(source,vec2<i32>(v.position.xy)+v.shift,0); }
