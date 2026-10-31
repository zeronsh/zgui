// What area.wgsl's `zgx_area` stands in for where areas never draw on the
// GPU (Windows, `AREAS_ON_GPU`): nothing.
fn zgx_area(v:Out)->vec4<f32> { return vec4(0.); }
