// Writes queued texels into a texture with a render pass instead of a
// transfer. rects[0].xy is the target size; upload i uses rects[1+2i] =
// (x, y, width, height) and rects[2+2i].x = its first texel in `pixels`.
@group(0) @binding(0) var<storage, read> rects: array<vec4<u32>>;
@group(0) @binding(1) var<storage, read> pixels: array<u32>;
struct Out { @builtin(position) position: vec4<f32>, @location(0) @interpolate(flat) index: u32 }
@vertex fn vs(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32) -> Out {
  // A four-vertex strip; corners from bits (see draw.wgsl).
  let corner = vec2(f32(v & 1u), f32(v >> 1u));
  let r = rects[1u + 2u * i];
  let size = vec2(f32(rects[0].x), f32(rects[0].y));
  let p = vec2(f32(r.x), f32(r.y)) + corner * vec2(f32(r.z), f32(r.w));
  var o: Out;
  o.position = vec4(p.x / size.x * 2. - 1., 1. - p.y / size.y * 2., 0., 1.);
  o.index = i;
  return o;
}
@fragment fn fs(in: Out) -> @location(0) vec4<f32> {
  let r = rects[1u + 2u * in.index];
  let local = vec2<u32>(in.position.xy) - r.xy;
  return unpack4x8unorm(pixels[rects[2u + 2u * in.index].x + local.y * r.z + local.x]);
}
