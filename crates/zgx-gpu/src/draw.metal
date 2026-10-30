// zgx entry points: fs=fs fs_basic=fs_basic fs_basic_both=fs_basic_both fs_both=fs_both vs=vs vs_basic=vs_basic
// language: metal2.4
#include <metal_stdlib>
#include <simd/simd.h>

using metal::uint;

struct _mslBufferSizes {
    uint size2;
};

struct Vertex {
    metal::float4 rect;
    metal::float4 uv;
    metal::float4 color;
    metal::float4 fade;
    metal::float4 options;
    metal::float4 shape;
    metal::float4 border;
    metal::float4 mask;
};
struct Out {
    metal::float4 position;
    metal::float2 uv;
    char _pad2[8];
    metal::float4 color;
    metal::float2 point;
    char _pad4[8];
    metal::float4 fade;
    metal::float4 options;
    metal::float4 shape;
    metal::float4 border;
    metal::float4 mask;
};
struct Region {
    metal::float4 round0_;
    metal::float4 radii0_;
    metal::float4 round1_;
    metal::float4 radii1_;
    metal::float4 fade_box;
    metal::float4 fade;
    metal::float4 color0_;
    metal::float4 color1_;
    metal::float4 color2_;
    metal::float4 tint;
};
typedef Region type_4[1];
struct Both {
    metal::float4 retained;
    metal::float4 drawable;
};
struct Basic {
    metal::float4 position;
    metal::float2 uv;
    char _pad2[8];
    metal::float4 color;
    float texture;
    char _pad4[12];
};

float rounded(
    metal::float4 b,
    metal::float4 r,
    float feather,
    metal::float2 p,
    constant metal::float4& viewport
) {
    if (b.z < 0.0) {
        return 1.0;
    }
    metal::float2 half_ = b.zw * 0.5;
    metal::float2 q = (p - b.xy) - half_;
    float radius = (q.y < 0.0) ? ((q.x > 0.0) ? r.y : r.x) : ((q.x > 0.0) ? r.z : r.w);
    metal::float2 d = (metal::abs(q) - half_) + metal::float2(radius);
    float sdf = (metal::length(metal::max(d, metal::float2(0.0))) + metal::min(metal::max(d.x, d.y), 0.0)) - radius;
    if (feather > 0.0) {
        float t = metal::clamp(-(sdf) / feather, 0.0, 1.0);
        return (t * t) * (3.0 - (2.0 * t));
    }
    float _e60 = viewport.z;
    return metal::clamp(0.5 - (sdf * _e60), 0.0, 1.0);
}

uint naga_f2u32(float value) {
    return static_cast<uint>(metal::clamp(value, 0.0, 4294967000.0));
}

metal::float4 tinted(
    float index,
    metal::float4 premultiplied,
    device type_4 const& regions,
    constant _mslBufferSizes& _buffer_sizes
) {
    bool local = {};
    if (index < 0.5) {
        return premultiplied;
    }
    Region g = regions[naga_f2u32(index) - 1u];
    if (!((g.tint.x < 0.5))) {
        local = premultiplied.w <= 0.0;
    } else {
        local = true;
    }
    bool _e21 = local;
    if (_e21) {
        return premultiplied;
    }
    metal::float4 c = metal::float4(premultiplied.xyz / metal::float3(premultiplied.w), 1.0);
    metal::float3 rgb_1 = metal::clamp(metal::float3(metal::dot(g.color0_, c), metal::dot(g.color1_, c), metal::dot(g.color2_, c)), metal::float3(0.0), metal::float3(1.0));
    return metal::float4(rgb_1 * premultiplied.w, premultiplied.w);
}

float faded(
    metal::float4 b_1,
    metal::float4 f,
    metal::float2 p_1
) {
    float k = 1.0;
    if (f.x > 0.0) {
        float t_1 = metal::clamp((p_1.y - b_1.y) / f.x, 0.0, 1.0);
        float _e16 = k;
        k = _e16 * (t_1 * t_1);
    }
    if (f.y > 0.0) {
        float t_2 = metal::clamp((b_1.z - p_1.x) / f.y, 0.0, 1.0);
        float _e30 = k;
        k = _e30 * (t_2 * t_2);
    }
    if (f.z > 0.0) {
        float t_3 = metal::clamp((b_1.w - p_1.y) / f.z, 0.0, 1.0);
        float _e44 = k;
        k = _e44 * (t_3 * t_3);
    }
    if (f.w > 0.0) {
        float t_4 = metal::clamp((p_1.x - b_1.x) / f.w, 0.0, 1.0);
        float _e58 = k;
        k = _e58 * (t_4 * t_4);
    }
    float _e61 = k;
    return _e61;
}

float region(
    float index_1,
    metal::float2 position_1,
    constant metal::float4& viewport,
    device type_4 const& regions,
    constant _mslBufferSizes& _buffer_sizes
) {
    float k_1 = {};
    Region g_1 = regions[naga_f2u32(index_1) - 1u];
    float _e10 = viewport.z;
    metal::float2 p_2 = position_1 / metal::float2(_e10);
    float _e17 = rounded(g_1.round0_, g_1.radii0_, g_1.tint.y, p_2, viewport);
    float _e22 = rounded(g_1.round1_, g_1.radii1_, g_1.tint.z, p_2, viewport);
    k_1 = _e17 * _e22;
    float _e25 = k_1;
    float _e28 = faded(g_1.fade_box, g_1.fade, p_2);
    k_1 = _e25 * _e28;
    if (g_1.tint.w > 0.5) {
        Region o_2 = regions[naga_f2u32(g_1.tint.w) - 1u];
        float _e42 = k_1;
        float _e45 = faded(o_2.fade_box, o_2.fade, p_2);
        k_1 = _e42 * _e45;
    }
    float _e47 = k_1;
    return _e47;
}

metal::float2 centred(
    metal::float2 at,
    metal::float4 box,
    constant metal::float4& viewport
) {
    float _e9 = viewport.z;
    metal::float2 middle = metal::rint(((box.xy + (box.zw * 0.5)) * _e9) * 64.0) / metal::float2(64.0);
    float _e21 = viewport.z;
    return metal::abs(at - middle) / metal::float2(_e21);
}

metal::float4 shade_texel(
    Out input,
    metal::float4 given,
    bool has_texel,
    constant metal::float4& viewport,
    metal::texture2d<float, metal::access::sample> atlas,
    device type_4 const& regions,
    metal::sampler tex_sampler,
    constant _mslBufferSizes& _buffer_sizes
) {
    Out v_5 = {};
    metal::float4 color_1 = {};
    bool local_1 = {};
    bool local_2 = {};
    float corner = {};
    bool local_3 = {};
    bool premultiplied_1 = false;
    metal::float3 rgb = metal::float3(0.0);
    float scale = 1.0;
    metal::float4 tex = {};
    v_5 = input;
    float _e6 = v_5.options.z;
    if (_e6 < 0.5) {
        metal::float4 _e11 = v_5.position;
        float _e15 = viewport.z;
        v_5.point = _e11.xy / metal::float2(_e15);
    }
    metal::float4 _e19 = v_5.color;
    color_1 = _e19;
    float _e23 = v_5.options.z;
    if (_e23 < 0.5) {
        float _e30 = v_5.shape.w;
        if (!((_e30 > 0.5))) {
            float _e38 = v_5.shape.z;
            local_2 = _e38 > 0.0;
        } else {
            local_2 = true;
        }
        bool _e42 = local_2;
        local_1 = _e42;
    } else {
        local_1 = false;
    }
    bool _e44 = local_1;
    if (_e44) {
        float _e47 = v_5.shape.x;
        corner = _e47;
        float _e51 = v_5.options.w;
        if (_e51 > 0.5) {
            float _e56 = v_5.point.x;
            float _e59 = v_5.fade.x;
            float _e62 = v_5.fade.z;
            bool right = _e56 > (_e59 + (_e62 * 0.5));
            float _e69 = v_5.point.y;
            float _e72 = v_5.fade.y;
            float _e75 = v_5.fade.w;
            bool bottom = _e69 > (_e72 + (_e75 * 0.5));
            float _e82 = v_5.border.x;
            float _e85 = v_5.border.y;
            float _e89 = v_5.border.w;
            float _e92 = v_5.border.z;
            corner = bottom ? (right ? _e92 : _e89) : (right ? _e85 : _e82);
        }
        float _e95 = corner;
        float _e98 = v_5.fade.z;
        float _e101 = v_5.fade.w;
        float radius_1 = metal::clamp(_e95, 0.0, metal::min(_e98, _e101) * 0.5);
        metal::float4 _e108 = v_5.position;
        metal::float4 _e111 = v_5.fade;
        metal::float2 _e112 = centred(_e108.xy, _e111, viewport);
        metal::float4 _e114 = v_5.fade;
        metal::float2 q_1 = _e112 - ((_e114.zw * 0.5) - metal::float2(radius_1));
        float distance = (metal::length(metal::max(q_1, metal::float2(0.0))) + metal::min(metal::max(q_1.x, q_1.y), 0.0)) - radius_1;
        metal::float2 outside = metal::max(q_1, metal::float2(0.0));
        float reach = metal::length(outside);
        metal::float2 gradient = (reach > 0.0) ? (outside / metal::float2(metal::max(reach, 0.00000000000000000001))) : ((q_1.x > q_1.y) ? metal::float2(1.0, 0.0) : metal::float2(0.0, 1.0));
        float _e155 = viewport.z;
        metal::float2 slope = gradient / metal::float2(_e155);
        float _e160 = v_5.shape.y;
        if (_e160 < 0.0) {
            float _e165 = v_5.shape.x;
            uint a = naga_f2u32(_e165);
            float _e169 = v_5.shape.z;
            uint b_2 = naga_f2u32(_e169);
            metal::float4 sides = metal::float4(static_cast<float>(a & 4095u), static_cast<float>(a >> 12u), static_cast<float>(b_2 & 4095u), static_cast<float>(b_2 >> 12u)) / metal::float4(16.0);
            float _e189 = v_5.point.x;
            float _e192 = v_5.fade.x;
            float _e195 = v_5.fade.z;
            bool right_1 = _e189 > (_e192 + (_e195 * 0.5));
            float _e202 = v_5.point.y;
            float _e205 = v_5.fade.y;
            float _e208 = v_5.fade.w;
            bool bottom_1 = _e202 > (_e205 + (_e208 * 0.5));
            float across = right_1 ? sides.y : sides.w;
            float down = bottom_1 ? sides.z : sides.x;
            metal::float4 _e220 = v_5.fade;
            metal::float4 _e225 = v_5.fade;
            metal::float4 box_1 = metal::float4(_e220.xy + sides.wx, metal::max((_e225.zw - sides.wx) - sides.yz, metal::float2(0.0)));
            float inner_radius = metal::clamp(radius_1 - ((across + down) * 0.5), 0.0, metal::min(box_1.z, box_1.w) * 0.5);
            metal::float4 _e247 = v_5.position;
            metal::float2 _e249 = centred(_e247.xy, box_1, viewport);
            metal::float2 iq = _e249 - ((box_1.zw * 0.5) - metal::float2(inner_radius));
            float inside = (metal::length(metal::max(iq, metal::float2(0.0))) + metal::min(metal::max(iq.x, iq.y), 0.0)) - inner_radius;
            float aa = metal::max(slope.x + slope.y, 0.5);
            if (box_1.z > 0.0) {
                local_3 = box_1.w > 0.0;
            } else {
                local_3 = false;
            }
            bool _e287 = local_3;
            float hollow = _e287 ? metal::clamp(0.5 - (inside / aa), 0.0, 1.0) : 0.0;
            float _e291 = color_1.w;
            color_1.w = _e291 * metal::max(metal::clamp(0.5 - (distance / aa), 0.0, 1.0) - hollow, 0.0);
        } else {
            float _e304 = v_5.shape.z;
            if (_e304 > 0.0) {
                float _e309 = v_5.shape.z;
                float x = distance / (_e309 * 1.4142135);
                float x2_ = x * x;
                float e = metal::sign(x) * metal::sqrt(1.0 - metal::exp((-(x2_) * (1.2732395 + (0.147 * x2_))) / (1.0 + (0.147 * x2_))));
                float _e332 = color_1.w;
                color_1.w = _e332 * (0.5 - (0.5 * e));
            } else {
                float dx = slope.x;
                float dy = slope.y;
                float aa_1 = metal::max((radius_1 <= 0.0) ? metal::max(dx, dy) : (dx + dy), 0.5);
                float outer = metal::clamp(0.5 - (distance / aa_1), 0.0, 1.0);
                float _e355 = v_5.shape.y;
                float inner = metal::clamp(0.5 - ((distance + _e355) / aa_1), 0.0, 1.0);
                float _e364 = color_1.w;
                float _e368 = v_5.border.w;
                float alpha = (_e364 * inner) + (_e368 * (outer - inner));
                metal::float4 _e372 = color_1;
                float _e375 = color_1.w;
                metal::float4 _e379 = v_5.border;
                float _e383 = v_5.border.w;
                metal::float3 rgb_2 = (alpha > 0.0) ? ((((_e372.xyz * _e375) * inner) + ((_e379.xyz * _e383) * (outer - inner))) / metal::float3(metal::max(alpha, 0.00001))) : metal::float3(0.0);
                color_1 = metal::float4(rgb_2, alpha);
            }
        }
    }
    float _e407 = v_5.options.y;
    if (_e407 > 0.5) {
        tex = given;
        if (!(has_texel)) {
            metal::float2 _e415 = v_5.uv;
            metal::float4 _e416 = atlas.sample(tex_sampler, _e415);
            tex = _e416;
        }
        float _e419 = v_5.options.y;
        if (_e419 > 1.5) {
            premultiplied_1 = true;
            metal::float4 _e423 = color_1;
            metal::float4 _e425 = tex;
            float _e429 = color_1.w;
            rgb = (_e423.xyz * _e425.xyz) * _e429;
            float _e433 = color_1.w;
            float _e435 = tex.w;
            color_1.w = _e433 * _e435;
        } else {
            metal::float4 _e437 = color_1;
            metal::float4 _e439 = tex;
            float _e443 = color_1.w;
            float _e445 = tex.w;
            color_1 = metal::float4(_e437.xyz * _e439.xyz, _e443 * _e445);
        }
    }
    float _e450 = v_5.options.x;
    if (_e450 > 0.0) {
        metal::float2 _e454 = v_5.point;
        metal::float4 _e456 = v_5.fade;
        metal::float4 _e460 = v_5.fade;
        metal::float4 _e463 = v_5.fade;
        metal::float2 _e467 = v_5.point;
        metal::float2 distance_1 = metal::min(_e454 - _e456.xy, (_e460.xy + _e463.zw) - _e467);
        float _e475 = v_5.options.x;
        float f_1 = metal::clamp(metal::min(distance_1.x, distance_1.y) / _e475, 0.0, 1.0);
        float _e481 = color_1.w;
        color_1.w = _e481 * f_1;
        float _e483 = scale;
        scale = _e483 * f_1;
    }
    float _e487 = v_5.mask.x;
    if (_e487 > 0.5) {
        float _e492 = v_5.mask.x;
        metal::float4 _e494 = v_5.position;
        float _e496 = region(_e492, _e494.xy, viewport, regions, _buffer_sizes);
        float _e498 = color_1.w;
        color_1.w = _e498 * _e496;
        float _e500 = scale;
        scale = _e500 * _e496;
    }
    float _e504 = v_5.mask.x;
    metal::float4 _e505 = color_1;
    float _e508 = color_1.w;
    float _e511 = color_1.w;
    metal::float3 _e513 = rgb;
    float _e514 = scale;
    float _e517 = color_1.w;
    bool _e519 = premultiplied_1;
    metal::float4 _e521 = tinted(_e504, _e519 ? metal::float4(_e513 * _e514, _e517) : metal::float4(_e505.xyz * _e508, _e511), regions, _buffer_sizes);
    return _e521;
}

metal::float4 shade(
    Out input_1,
    constant metal::float4& viewport,
    metal::texture2d<float, metal::access::sample> atlas,
    device type_4 const& regions,
    metal::sampler tex_sampler,
    constant _mslBufferSizes& _buffer_sizes
) {
    metal::float4 _e4 = shade_texel(input_1, metal::float4(0.0), false, viewport, atlas, regions, tex_sampler, _buffer_sizes);
    return _e4;
}

metal::float4 basic(
    Basic v_6,
    metal::texture2d<float, metal::access::sample> atlas,
    metal::sampler tex_sampler
) {
    if (v_6.texture > 0.5) {
        metal::float4 tex_1 = atlas.sample(tex_sampler, v_6.uv);
        float a_1 = v_6.color.w * tex_1.w;
        return metal::float4((v_6.color.xyz * tex_1.xyz) * ((v_6.texture > 1.5) ? v_6.color.w : a_1), a_1);
    }
    return metal::float4(v_6.color.xyz * v_6.color.w, v_6.color.w);
}

struct vsInput {
    metal::float4 rect [[attribute(0)]];
    metal::float4 uv [[attribute(1)]];
    metal::float4 color [[attribute(2)]];
    metal::float4 fade [[attribute(3)]];
    metal::float4 options [[attribute(4)]];
    metal::float4 shape [[attribute(5)]];
    metal::float4 border [[attribute(6)]];
    metal::float4 mask [[attribute(7)]];
};
struct vsOutput {
    metal::float4 position [[position]];
    metal::float2 uv [[user(loc0), center_perspective]];
    metal::float4 color [[user(loc1), flat]];
    metal::float2 point [[user(loc2), center_perspective]];
    metal::float4 fade [[user(loc3), flat]];
    metal::float4 options [[user(loc4), flat]];
    metal::float4 shape [[user(loc5), flat]];
    metal::float4 border [[user(loc6), flat]];
    metal::float4 mask [[user(loc7), flat]];
};
vertex vsOutput vs(
  vsInput varyings [[stage_in]]
, uint i [[vertex_id]]
, constant metal::float4& viewport [[buffer(0)]]
) {
    const Vertex v = { varyings.rect, varyings.uv, varyings.color, varyings.fade, varyings.options, varyings.shape, varyings.border, varyings.mask };
    metal::float2 position = {};
    float w = 1.0;
    Out o = {};
    metal::float2 corner_1 = metal::float2(static_cast<float>(i & 1u), static_cast<float>(i >> 1u));
    metal::float2 p_3 = v.rect.xy + (corner_1 * v.rect.zw);
    position = p_3;
    if (v.options.z > 0.5) {
        position = metal::float2(metal::dot(v.shape.xy, p_3) + v.border.x, metal::dot(v.shape.zw, p_3) + v.border.y);
    }
    if (v.options.z > 1.5) {
        w = metal::max(metal::dot(v.border.zw, p_3) + 1.0, 0.0001);
    }
    float _e49 = position.x;
    float _e52 = viewport.x;
    float _e56 = w;
    float _e58 = w;
    float _e60 = position.y;
    float _e63 = viewport.y;
    float _e68 = w;
    o.position = metal::float4(((_e49 / _e52) * 2.0) - _e56, _e58 - ((_e60 / _e63) * 2.0), 0.0, _e68);
    o.uv = v.uv.xy + (corner_1 * v.uv.zw);
    o.color = v.color;
    o.point = p_3;
    o.fade = v.fade;
    o.options = v.options;
    o.shape = v.shape;
    o.border = v.border;
    o.mask = v.mask;
    Out _e91 = o;
    const auto _tmp = _e91;
    return vsOutput { _tmp.position, _tmp.uv, _tmp.color, _tmp.point, _tmp.fade, _tmp.options, _tmp.shape, _tmp.border, _tmp.mask };
}


struct fsInput {
    metal::float2 uv [[user(loc0), center_perspective]];
    metal::float4 color [[user(loc1), flat]];
    metal::float2 point [[user(loc2), center_perspective]];
    metal::float4 fade [[user(loc3), flat]];
    metal::float4 options [[user(loc4), flat]];
    metal::float4 shape [[user(loc5), flat]];
    metal::float4 border [[user(loc6), flat]];
    metal::float4 mask [[user(loc7), flat]];
};
struct fsOutput {
    metal::float4 member_1 [[color(0)]];
};
fragment fsOutput fs(
  fsInput varyings_1 [[stage_in]]
, metal::float4 position_2 [[position]]
, constant metal::float4& viewport [[buffer(0)]]
, metal::texture2d<float, metal::access::sample> atlas [[texture(0)]]
, device type_4 const& regions [[buffer(2)]]
, constant _mslBufferSizes& _buffer_sizes [[buffer(3)]]
) {
    constexpr metal::sampler tex_sampler(
        metal::s_address::clamp_to_edge,
        metal::t_address::clamp_to_edge,
        metal::r_address::clamp_to_edge,
        metal::mag_filter::linear,
        metal::min_filter::linear,
        metal::coord::normalized
    );
    const Out v_1 = { position_2, varyings_1.uv, {}, varyings_1.color, varyings_1.point, {}, varyings_1.fade, varyings_1.options, varyings_1.shape, varyings_1.border, varyings_1.mask };
    metal::float4 _e1 = shade(v_1, viewport, atlas, regions, tex_sampler, _buffer_sizes);
    return fsOutput { _e1 };
}


struct fs_bothInput {
    metal::float2 uv [[user(loc0), center_perspective]];
    metal::float4 color [[user(loc1), flat]];
    metal::float2 point [[user(loc2), center_perspective]];
    metal::float4 fade [[user(loc3), flat]];
    metal::float4 options [[user(loc4), flat]];
    metal::float4 shape [[user(loc5), flat]];
    metal::float4 border [[user(loc6), flat]];
    metal::float4 mask [[user(loc7), flat]];
};
struct fs_bothOutput {
    metal::float4 retained [[color(0)]];
    metal::float4 drawable [[color(1)]];
};
fragment fs_bothOutput fs_both(
  fs_bothInput varyings_2 [[stage_in]]
, metal::float4 position_3 [[position]]
, constant metal::float4& viewport [[buffer(0)]]
, metal::texture2d<float, metal::access::sample> atlas [[texture(0)]]
, device type_4 const& regions [[buffer(2)]]
, constant _mslBufferSizes& _buffer_sizes [[buffer(3)]]
) {
    constexpr metal::sampler tex_sampler(
        metal::s_address::clamp_to_edge,
        metal::t_address::clamp_to_edge,
        metal::r_address::clamp_to_edge,
        metal::mag_filter::linear,
        metal::min_filter::linear,
        metal::coord::normalized
    );
    const Out v_2 = { position_3, varyings_2.uv, {}, varyings_2.color, varyings_2.point, {}, varyings_2.fade, varyings_2.options, varyings_2.shape, varyings_2.border, varyings_2.mask };
    metal::float4 _e1 = shade(v_2, viewport, atlas, regions, tex_sampler, _buffer_sizes);
    const auto _tmp = Both {_e1, _e1};
    return fs_bothOutput { _tmp.retained, _tmp.drawable };
}


struct vs_basicInput {
    metal::float4 rect [[attribute(0)]];
    metal::float4 uv [[attribute(1)]];
    metal::float4 color [[attribute(2)]];
    metal::float4 options [[attribute(4)]];
};
struct vs_basicOutput {
    metal::float4 position [[position]];
    metal::float2 uv [[user(loc0), center_perspective]];
    metal::float4 color [[user(loc1), flat]];
    float texture [[user(loc2), flat]];
};
vertex vs_basicOutput vs_basic(
  vs_basicInput varyings_3 [[stage_in]]
, uint i_1 [[vertex_id]]
, constant metal::float4& viewport [[buffer(0)]]
) {
    const auto rect = varyings_3.rect;
    const auto uv = varyings_3.uv;
    const auto color = varyings_3.color;
    const auto options = varyings_3.options;
    Basic o_1 = {};
    metal::float2 corner_2 = metal::float2(static_cast<float>(i_1 & 1u), static_cast<float>(i_1 >> 1u));
    metal::float2 p_4 = rect.xy + (corner_2 * rect.zw);
    float _e21 = viewport.x;
    float _e30 = viewport.y;
    o_1.position = metal::float4(((p_4.x / _e21) * 2.0) - 1.0, 1.0 - ((p_4.y / _e30) * 2.0), 0.0, 1.0);
    o_1.uv = uv.xy + (corner_2 * uv.zw);
    o_1.color = color;
    o_1.texture = options.y;
    Basic _e47 = o_1;
    const auto _tmp = _e47;
    return vs_basicOutput { _tmp.position, _tmp.uv, _tmp.color, _tmp.texture };
}


struct fs_basicInput {
    metal::float2 uv [[user(loc0), center_perspective]];
    metal::float4 color [[user(loc1), flat]];
    float texture [[user(loc2), flat]];
};
struct fs_basicOutput {
    metal::float4 member_4 [[color(0)]];
};
fragment fs_basicOutput fs_basic(
  fs_basicInput varyings_4 [[stage_in]]
, metal::float4 position_4 [[position]]
, metal::texture2d<float, metal::access::sample> atlas [[texture(0)]]
) {
    constexpr metal::sampler tex_sampler(
        metal::s_address::clamp_to_edge,
        metal::t_address::clamp_to_edge,
        metal::r_address::clamp_to_edge,
        metal::mag_filter::linear,
        metal::min_filter::linear,
        metal::coord::normalized
    );
    const Basic v_3 = { position_4, varyings_4.uv, {}, varyings_4.color, varyings_4.texture };
    metal::float4 _e1 = basic(v_3, atlas, tex_sampler);
    return fs_basicOutput { _e1 };
}


struct fs_basic_bothInput {
    metal::float2 uv [[user(loc0), center_perspective]];
    metal::float4 color [[user(loc1), flat]];
    float texture [[user(loc2), flat]];
};
struct fs_basic_bothOutput {
    metal::float4 retained [[color(0)]];
    metal::float4 drawable [[color(1)]];
};
fragment fs_basic_bothOutput fs_basic_both(
  fs_basic_bothInput varyings_5 [[stage_in]]
, metal::float4 position_5 [[position]]
, metal::texture2d<float, metal::access::sample> atlas [[texture(0)]]
) {
    constexpr metal::sampler tex_sampler(
        metal::s_address::clamp_to_edge,
        metal::t_address::clamp_to_edge,
        metal::r_address::clamp_to_edge,
        metal::mag_filter::linear,
        metal::min_filter::linear,
        metal::coord::normalized
    );
    const Basic v_4 = { position_5, varyings_5.uv, {}, varyings_5.color, varyings_5.texture };
    metal::float4 _e1 = basic(v_4, atlas, tex_sampler);
    const auto _tmp = Both {_e1, _e1};
    return fs_basic_bothOutput { _tmp.retained, _tmp.drawable };
}
