// Blend modes (`zgx::scene::BlendMode`, CSS `mix-blend-mode`), as the W3C
// Compositing and Blending spec defines them: `B(cb, cs)` of the backdrop's
// and the source's straight colours, composited source-over:
//   co = (1 - ab) * cs·as + (1 - as) * cb·ab + as·ab * B(cb, cs), ao = as + ab - as·ab
// Appended to draw.wgsl; a blended layer's quad carries its mode in
// `mask.z` and reads the backdrop, a copy of the target under it, by pixel.
@group(2) @binding(0) var zgx_backdrop:texture_2d<f32>;

fn zgx_lum(c:vec3<f32>)->f32 { return dot(c,vec3(0.3,0.59,0.11)); }
fn zgx_clip_color(c:vec3<f32>)->vec3<f32> {
 let l=zgx_lum(c); let n=min(min(c.r,c.g),c.b); let x=max(max(c.r,c.g),c.b);
 var o=c;
 if n<0. {o=l+(o-l)*l/max(l-n,1e-6);}
 if x>1. {o=l+(o-l)*(1.-l)/max(x-l,1e-6);}
 return o;
}
fn zgx_set_lum(c:vec3<f32>, l:f32)->vec3<f32> { return zgx_clip_color(c+(l-zgx_lum(c))); }
fn zgx_sat(c:vec3<f32>)->f32 { return max(max(c.r,c.g),c.b)-min(min(c.r,c.g),c.b); }
// `c` with saturation `s`: its largest channel `s`, its smallest 0, the
// middle one in proportion.
fn zgx_set_sat(c:vec3<f32>, s:f32)->vec3<f32> {
 let n=min(min(c.r,c.g),c.b); let x=max(max(c.r,c.g),c.b);
 if x<=n {return vec3(0.);}
 return (c-n)*s/(x-n);
}
fn zgx_hard_light(cb:vec3<f32>, cs:vec3<f32>)->vec3<f32> {
 let multiply=cb*2.*cs; let screen=cb+(2.*cs-1.)-cb*(2.*cs-1.);
 return select(screen,multiply,cs<=vec3(.5));
}
fn zgx_soft_light(cb:vec3<f32>, cs:vec3<f32>)->vec3<f32> {
 let d=select(sqrt(cb),((16.*cb-12.)*cb+4.)*cb,cb<=vec3(.25));
 return select(cb+(2.*cs-1.)*(d-cb),cb-(1.-2.*cs)*cb*(1.-cb),cs<=vec3(.5));
}
fn zgx_dodge(cb:f32, cs:f32)->f32 {
 if cb<=0. {return 0.;}
 if cs>=1. {return 1.;}
 return min(1.,cb/(1.-cs));
}
fn zgx_burn(cb:f32, cs:f32)->f32 {
 if cb>=1. {return 1.;}
 if cs<=0. {return 0.;}
 return 1.-min(1.,(1.-cb)/cs);
}
fn zgx_mix(cb:vec3<f32>, cs:vec3<f32>, mode:u32)->vec3<f32> {
 switch mode {
  case 1u {return cb*cs;}
  case 2u {return cb+cs-cb*cs;}
  case 3u {return zgx_hard_light(cs,cb);}
  case 4u {return min(cb,cs);}
  case 5u {return max(cb,cs);}
  case 6u {return vec3(zgx_dodge(cb.r,cs.r),zgx_dodge(cb.g,cs.g),zgx_dodge(cb.b,cs.b));}
  case 7u {return vec3(zgx_burn(cb.r,cs.r),zgx_burn(cb.g,cs.g),zgx_burn(cb.b,cs.b));}
  case 8u {return zgx_hard_light(cb,cs);}
  case 9u {return zgx_soft_light(cb,cs);}
  case 10u {return abs(cb-cs);}
  case 11u {return cb+cs-2.*cb*cs;}
  case 12u {return zgx_set_lum(zgx_set_sat(cs,zgx_sat(cb)),zgx_lum(cb));}
  case 13u {return zgx_set_lum(zgx_set_sat(cb,zgx_sat(cs)),zgx_lum(cb));}
  case 14u {return zgx_set_lum(cs,zgx_lum(cb));}
  case 15u {return zgx_set_lum(cb,zgx_lum(cs));}
  default {return cs;}
 }
}
// Premultiplied source `s` over premultiplied backdrop `d` by `mode`.
fn zgx_blend(s:vec4<f32>, d:vec4<f32>, mode:u32)->vec4<f32> {
 let cs=select(vec3(0.),s.rgb/s.a,s.a>0.); let cb=select(vec3(0.),d.rgb/d.a,d.a>0.);
 let mixed=clamp(zgx_mix(cb,cs,mode),vec3(0.),vec3(1.));
 return vec4((1.-d.a)*s.rgb+(1.-s.a)*d.rgb+s.a*d.a*mixed,s.a+d.a-s.a*d.a);
}
// The backdrop's pixel under this fragment.
fn zgx_beneath(position:vec4<f32>)->vec4<f32> {
 return textureLoad(zgx_backdrop,vec2<i32>(position.xy),0);
}
