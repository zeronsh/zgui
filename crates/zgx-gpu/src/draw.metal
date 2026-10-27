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

float region(
    float index_1,
    metal::float2 position_1,
    constant metal::float4& viewport,
    device type_4 const& regions,
    constant _mslBufferSizes& _buffer_sizes
) {
    float k = {};
    Region g_1 = regions[naga_f2u32(index_1) - 1u];
    float _e10 = viewport.z;
    metal::float2 p_1 = position_1 / metal::float2(_e10);
    float _e17 = rounded(g_1.round0_, g_1.radii0_, g_1.tint.y, p_1, viewport);
    float _e22 = rounded(g_1.round1_, g_1.radii1_, g_1.tint.z, p_1, viewport);
    k = _e17 * _e22;
    if (g_1.fade.x > 0.0) {
        float t_1 = metal::clamp((p_1.y - g_1.fade_box.y) / g_1.fade.x, 0.0, 1.0);
        float _e39 = k;
        k = _e39 * (t_1 * t_1);
    }
    if (g_1.fade.y > 0.0) {
        float t_2 = metal::clamp((g_1.fade_box.z - p_1.x) / g_1.fade.y, 0.0, 1.0);
        float _e56 = k;
        k = _e56 * (t_2 * t_2);
    }
    if (g_1.fade.z > 0.0) {
        float t_3 = metal::clamp((g_1.fade_box.w - p_1.y) / g_1.fade.z, 0.0, 1.0);
        float _e73 = k;
        k = _e73 * (t_3 * t_3);
    }
    if (g_1.fade.w > 0.0) {
        float t_4 = metal::clamp((p_1.x - g_1.fade_box.x) / g_1.fade.w, 0.0, 1.0);
        float _e90 = k;
        k = _e90 * (t_4 * t_4);
    }
    float _e93 = k;
    return _e93;
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
    bool local_3 = {};
    bool local_4 = {};
    bool local_5 = {};
    bool local_6 = {};
    float corner = {};
    bool local_7 = {};
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
    metal::float4 _e22 = v_5.position;
    float _e34 = viewport.z;
    metal::float2 block = ((metal::floor(_e22.xy * 0.5) * 2.0) + metal::float2(1.0)) / metal::float2(_e34);
    float _e39 = v_5.shape.x;
    float _e42 = v_5.border.x;
    float _e45 = v_5.border.y;
    float _e49 = v_5.border.z;
    float _e52 = v_5.border.w;
    float _e57 = v_5.options.w;
    float largest = (_e57 > 0.5) ? metal::max(metal::max(_e42, _e45), metal::max(_e49, _e52)) : _e39;
    float _e65 = v_5.shape.y;
    float _e69 = viewport.z;
    float _e77 = viewport.z;
    float reach = (((metal::max(largest, 0.0) + _e65) + metal::max(1.0 / _e69, 0.25)) + (0.5 / _e77)) + 0.01;
    float _e85 = v_5.options.z;
    if (_e85 < 0.5) {
        float _e92 = v_5.shape.w;
        local_1 = _e92 > 0.5;
    } else {
        local_1 = false;
    }
    bool _e96 = local_1;
    if (_e96) {
        float _e101 = v_5.shape.y;
        local_2 = _e101 >= 0.0;
    } else {
        local_2 = false;
    }
    bool _e105 = local_2;
    if (_e105) {
        float _e110 = v_5.shape.z;
        local_3 = _e110 <= 0.0;
    } else {
        local_3 = false;
    }
    bool _e114 = local_3;
    if (_e114) {
        metal::float4 _e118 = v_5.fade;
        metal::float4 _e122 = v_5.fade;
        metal::float4 _e129 = v_5.fade;
        local_4 = metal::all(metal::abs((block - _e118.xy) - (_e122.zw * 0.5)) <= ((_e129.zw * 0.5) - metal::float2(reach)));
    } else {
        local_4 = false;
    }
    bool _e138 = local_4;
    if (_e138) {
        float _e142 = color_1.w;
        float _e146 = v_5.border.w;
        float alpha = (_e142 * 1.0) + (_e146 * (1.0 - 1.0));
        metal::float4 _e150 = color_1;
        float _e153 = color_1.w;
        metal::float4 _e157 = v_5.border;
        float _e161 = v_5.border.w;
        metal::float3 rgb_2 = (alpha > 0.0) ? ((((_e150.xyz * _e153) * 1.0) + ((_e157.xyz * _e161) * (1.0 - 1.0))) / metal::float3(metal::max(alpha, 0.00001))) : metal::float3(0.0);
        color_1 = metal::float4(rgb_2, alpha);
    } else {
        float _e178 = v_5.options.z;
        if (_e178 < 0.5) {
            float _e185 = v_5.shape.w;
            if (!((_e185 > 0.5))) {
                float _e193 = v_5.shape.z;
                local_6 = _e193 > 0.0;
            } else {
                local_6 = true;
            }
            bool _e197 = local_6;
            local_5 = _e197;
        } else {
            local_5 = false;
        }
        bool _e199 = local_5;
        if (_e199) {
            float _e202 = v_5.shape.x;
            corner = _e202;
            float _e206 = v_5.options.w;
            if (_e206 > 0.5) {
                float _e211 = v_5.point.x;
                float _e214 = v_5.fade.x;
                float _e217 = v_5.fade.z;
                bool right = _e211 > (_e214 + (_e217 * 0.5));
                float _e224 = v_5.point.y;
                float _e227 = v_5.fade.y;
                float _e230 = v_5.fade.w;
                bool bottom = _e224 > (_e227 + (_e230 * 0.5));
                float _e237 = v_5.border.x;
                float _e240 = v_5.border.y;
                float _e244 = v_5.border.w;
                float _e247 = v_5.border.z;
                corner = bottom ? (right ? _e247 : _e244) : (right ? _e240 : _e237);
            }
            float _e250 = corner;
            float _e253 = v_5.fade.z;
            float _e256 = v_5.fade.w;
            float radius_1 = metal::clamp(_e250, 0.0, metal::min(_e253, _e256) * 0.5);
            metal::float2 _e263 = v_5.point;
            metal::float4 _e265 = v_5.fade;
            metal::float4 _e269 = v_5.fade;
            metal::float4 _e276 = v_5.fade;
            metal::float2 q_1 = metal::abs((_e263 - _e265.xy) - (_e269.zw * 0.5)) - ((_e276.zw * 0.5) - metal::float2(radius_1));
            float distance = (metal::length(metal::max(q_1, metal::float2(0.0))) + metal::min(metal::max(q_1.x, q_1.y), 0.0)) - radius_1;
            float _e296 = v_5.shape.y;
            if (_e296 < 0.0) {
                float _e301 = v_5.shape.x;
                uint a = naga_f2u32(_e301);
                float _e305 = v_5.shape.z;
                uint b_1 = naga_f2u32(_e305);
                metal::float4 sides = metal::float4(static_cast<float>(a & 4095u), static_cast<float>(a >> 12u), static_cast<float>(b_1 & 4095u), static_cast<float>(b_1 >> 12u)) / metal::float4(16.0);
                float _e325 = v_5.point.x;
                float _e328 = v_5.fade.x;
                float _e331 = v_5.fade.z;
                bool right_1 = _e325 > (_e328 + (_e331 * 0.5));
                float _e338 = v_5.point.y;
                float _e341 = v_5.fade.y;
                float _e344 = v_5.fade.w;
                bool bottom_1 = _e338 > (_e341 + (_e344 * 0.5));
                float across = right_1 ? sides.y : sides.w;
                float down = bottom_1 ? sides.z : sides.x;
                metal::float4 _e356 = v_5.fade;
                metal::float4 _e361 = v_5.fade;
                metal::float4 box = metal::float4(_e356.xy + sides.wx, metal::max((_e361.zw - sides.wx) - sides.yz, metal::float2(0.0)));
                float inner_radius = metal::clamp(radius_1 - ((across + down) * 0.5), 0.0, metal::min(box.z, box.w) * 0.5);
                metal::float2 _e383 = v_5.point;
                metal::float2 iq = metal::abs((_e383 - box.xy) - (box.zw * 0.5)) - ((box.zw * 0.5) - metal::float2(inner_radius));
                float inside = (metal::length(metal::max(iq, metal::float2(0.0))) + metal::min(metal::max(iq.x, iq.y), 0.0)) - inner_radius;
                float _e408 = metal::fwidth(distance);
                float aa = metal::max(_e408, 0.5);
                if (box.z > 0.0) {
                    local_7 = box.w > 0.0;
                } else {
                    local_7 = false;
                }
                bool _e426 = local_7;
                float hollow = _e426 ? metal::clamp(0.5 - (inside / aa), 0.0, 1.0) : 0.0;
                float _e430 = color_1.w;
                color_1.w = _e430 * metal::max(metal::clamp(0.5 - (distance / aa), 0.0, 1.0) - hollow, 0.0);
            } else {
                float _e443 = v_5.shape.z;
                if (_e443 > 0.0) {
                    float _e448 = v_5.shape.z;
                    float x = distance / (_e448 * 1.4142135);
                    float x2_ = x * x;
                    float e = metal::sign(x) * metal::sqrt(1.0 - metal::exp((-(x2_) * (1.2732395 + (0.147 * x2_))) / (1.0 + (0.147 * x2_))));
                    float _e471 = color_1.w;
                    color_1.w = _e471 * (0.5 - (0.5 * e));
                } else {
                    float _e477 = metal::dfdx(distance);
                    float dx = metal::abs(_e477);
                    float _e479 = metal::dfdy(distance);
                    float dy = metal::abs(_e479);
                    float aa_1 = metal::max((radius_1 <= 0.0) ? metal::max(dx, dy) : (dx + dy), 0.5);
                    float outer = metal::clamp(0.5 - (distance / aa_1), 0.0, 1.0);
                    float _e496 = v_5.shape.y;
                    float inner = metal::clamp(0.5 - ((distance + _e496) / aa_1), 0.0, 1.0);
                    float _e505 = color_1.w;
                    float _e509 = v_5.border.w;
                    float alpha_1 = (_e505 * inner) + (_e509 * (outer - inner));
                    metal::float4 _e513 = color_1;
                    float _e516 = color_1.w;
                    metal::float4 _e520 = v_5.border;
                    float _e524 = v_5.border.w;
                    metal::float3 rgb_3 = (alpha_1 > 0.0) ? ((((_e513.xyz * _e516) * inner) + ((_e520.xyz * _e524) * (outer - inner))) / metal::float3(metal::max(alpha_1, 0.00001))) : metal::float3(0.0);
                    color_1 = metal::float4(rgb_3, alpha_1);
                }
            }
        }
    }
    float _e548 = v_5.options.y;
    if (_e548 > 0.5) {
        tex = given;
        if (!(has_texel)) {
            metal::float2 _e556 = v_5.uv;
            metal::float4 _e557 = atlas.sample(tex_sampler, _e556);
            tex = _e557;
        }
        float _e560 = v_5.options.y;
        if (_e560 > 1.5) {
            premultiplied_1 = true;
            metal::float4 _e564 = color_1;
            metal::float4 _e566 = tex;
            float _e570 = color_1.w;
            rgb = (_e564.xyz * _e566.xyz) * _e570;
            float _e574 = color_1.w;
            float _e576 = tex.w;
            color_1.w = _e574 * _e576;
        } else {
            metal::float4 _e578 = color_1;
            metal::float4 _e580 = tex;
            float _e584 = color_1.w;
            float _e586 = tex.w;
            color_1 = metal::float4(_e578.xyz * _e580.xyz, _e584 * _e586);
        }
    }
    float _e591 = v_5.options.x;
    if (_e591 > 0.0) {
        metal::float2 _e595 = v_5.point;
        metal::float4 _e597 = v_5.fade;
        metal::float4 _e601 = v_5.fade;
        metal::float4 _e604 = v_5.fade;
        metal::float2 _e608 = v_5.point;
        metal::float2 distance_1 = metal::min(_e595 - _e597.xy, (_e601.xy + _e604.zw) - _e608);
        float _e616 = v_5.options.x;
        float f = metal::clamp(metal::min(distance_1.x, distance_1.y) / _e616, 0.0, 1.0);
        float _e622 = color_1.w;
        color_1.w = _e622 * f;
        float _e624 = scale;
        scale = _e624 * f;
    }
    float _e628 = v_5.mask.x;
    if (_e628 > 0.5) {
        float _e633 = v_5.mask.x;
        metal::float4 _e635 = v_5.position;
        float _e637 = region(_e633, _e635.xy, viewport, regions, _buffer_sizes);
        float _e639 = color_1.w;
        color_1.w = _e639 * _e637;
        float _e641 = scale;
        scale = _e641 * _e637;
    }
    float _e645 = v_5.mask.x;
    metal::float4 _e646 = color_1;
    float _e649 = color_1.w;
    float _e652 = color_1.w;
    metal::float3 _e654 = rgb;
    float _e655 = scale;
    float _e658 = color_1.w;
    bool _e660 = premultiplied_1;
    metal::float4 _e662 = tinted(_e645, _e660 ? metal::float4(_e654 * _e655, _e658) : metal::float4(_e646.xyz * _e649, _e652), regions, _buffer_sizes);
    return _e662;
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
    metal::float2 p_2 = v.rect.xy + (corner_1 * v.rect.zw);
    position = p_2;
    if (v.options.z > 0.5) {
        position = metal::float2(metal::dot(v.shape.xy, p_2) + v.border.x, metal::dot(v.shape.zw, p_2) + v.border.y);
    }
    if (v.options.z > 1.5) {
        w = metal::max(metal::dot(v.border.zw, p_2) + 1.0, 0.0001);
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
    o.point = p_2;
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
    metal::float2 p_3 = rect.xy + (corner_2 * rect.zw);
    float _e21 = viewport.x;
    float _e30 = viewport.y;
    o_1.position = metal::float4(((p_3.x / _e21) * 2.0) - 1.0, 1.0 - ((p_3.y / _e30) * 2.0), 0.0, 1.0);
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
