@group(0) @binding(0) var y_plane: texture_2d<f32>;
@group(0) @binding(1) var uv_plane: texture_2d<f32>;
@group(0) @binding(2) var output_frame: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(3) var linear_sampler: sampler;
@compute @workgroup_size(8,8)
fn convert(@builtin(global_invocation_id) position: vec3<u32>) {
    let size=textureDimensions(output_frame);
    if any(position.xy>=size) {return;}
    let y=textureLoad(y_plane,position.xy,0).r;
    let uv=textureSampleLevel(uv_plane,linear_sampler,(vec2<f32>(position.xy)+vec2<f32>(0.5))/vec2<f32>(size),0.0).rg-vec2<f32>(0.5);
    let rgb=vec3<f32>(y+1.402*uv.y,y-0.3441*uv.x-0.7141*uv.y,y+1.772*uv.x);
    textureStore(output_frame,position.xy,vec4<f32>(rgb,1.0));
}
