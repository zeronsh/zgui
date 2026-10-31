struct Vertex {
    float4 rect : LOC0;
    float4 uv : LOC1;
    float4 color : LOC2;
    float4 fade : LOC3;
    float4 options : LOC4;
    float4 shape : LOC5;
    float4 border : LOC6;
    float4 mask : LOC7;
};

struct Out {
    float4 position : SV_Position;
    float2 uv : LOC0;
    nointerpolation float4 color : LOC1;
    float2 point_ : LOC2;
    nointerpolation float4 fade : LOC3;
    nointerpolation float4 options : LOC4;
    nointerpolation float4 shape : LOC5;
    nointerpolation float4 border : LOC6;
    nointerpolation float4 mask : LOC7;
};

struct Region {
    float4 round0_;
    float4 radii0_;
    float4 round1_;
    float4 radii1_;
    float4 fade_box;
    float4 fade;
    float4 color0_;
    float4 color1_;
    float4 color2_;
    float4 tint;
};

struct Both {
    float4 retained : SV_Target0;
    float4 drawable : SV_Target1;
};

struct Basic {
    float4 position : SV_Position;
    float2 uv : LOC0;
    nointerpolation float4 color : LOC1;
    nointerpolation float texture_ : LOC2;
};

cbuffer viewport : register(b0) { float4 viewport; }
Texture2D<float4> atlas : register(t0);
ByteAddressBuffer regions : register(t1);
SamplerState tex_sampler : register(s0);
ByteAddressBuffer points : register(t2);

struct VertexOutput_vs {
    float2 uv_1 : LOC0;
    nointerpolation float4 color_4 : LOC1;
    float2 point_ : LOC2;
    nointerpolation float4 fade : LOC3;
    nointerpolation float4 options_1 : LOC4;
    nointerpolation float4 shape : LOC5;
    nointerpolation float4 border : LOC6;
    nointerpolation float4 mask : LOC7;
    float4 position_2 : SV_Position;
};

struct FragmentInput_fs {
    float2 uv_2 : LOC0;
    nointerpolation float4 color_5 : LOC1;
    float2 point_1 : LOC2;
    nointerpolation float4 fade_1 : LOC3;
    nointerpolation float4 options_2 : LOC4;
    nointerpolation float4 shape_1 : LOC5;
    nointerpolation float4 border_1 : LOC6;
    nointerpolation float4 mask_1 : LOC7;
    float4 position_3 : SV_Position;
};

struct FragmentInput_fs_both {
    float2 uv_3 : LOC0;
    nointerpolation float4 color_6 : LOC1;
    float2 point_2 : LOC2;
    nointerpolation float4 fade_2 : LOC3;
    nointerpolation float4 options_3 : LOC4;
    nointerpolation float4 shape_2 : LOC5;
    nointerpolation float4 border_2 : LOC6;
    nointerpolation float4 mask_2 : LOC7;
    float4 position_4 : SV_Position;
};

struct VertexOutput_vs_basic {
    float2 uv_4 : LOC0;
    nointerpolation float4 color_7 : LOC1;
    nointerpolation float texture_ : LOC2;
    float4 position_5 : SV_Position;
};

struct FragmentInput_fs_basic {
    float2 uv_5 : LOC0;
    nointerpolation float4 color_8 : LOC1;
    nointerpolation float texture_1 : LOC2;
    float4 position_6 : SV_Position;
};

struct FragmentInput_fs_basic_both {
    float2 uv_6 : LOC0;
    nointerpolation float4 color_9 : LOC1;
    nointerpolation float texture_2 : LOC2;
    float4 position_7 : SV_Position;
};

struct FragmentInput_fs_polyline {
    float2 uv_7 : LOC0;
    nointerpolation float4 color_10 : LOC1;
    float2 point_3 : LOC2;
    nointerpolation float4 fade_3 : LOC3;
    nointerpolation float4 options_4 : LOC4;
    nointerpolation float4 shape_3 : LOC5;
    nointerpolation float4 border_3 : LOC6;
    nointerpolation float4 mask_3 : LOC7;
    float4 position_8 : SV_Position;
};

struct FragmentInput_fs_polyline_both {
    float2 uv_8 : LOC0;
    nointerpolation float4 color_11 : LOC1;
    float2 point_4 : LOC2;
    nointerpolation float4 fade_4 : LOC3;
    nointerpolation float4 options_5 : LOC4;
    nointerpolation float4 shape_4 : LOC5;
    nointerpolation float4 border_4 : LOC6;
    nointerpolation float4 mask_4 : LOC7;
    float4 position_9 : SV_Position;
};

float rounded(float4 b, float4 r, float feather, float2 p)
{
    if ((b.z < 0.0)) {
        return 1.0;
    }
    float2 half_ = (b.zw * 0.5);
    float2 q = ((p - b.xy) - half_);
    float radius = ((q.y < 0.0) ? ((q.x > 0.0) ? r.y : r.x) : ((q.x > 0.0) ? r.z : r.w));
    float2 d_3 = ((abs(q) - half_) + (radius).xx);
    float sdf = ((length(max(d_3, (0.0).xx)) + min(max(d_3.x, d_3.y), 0.0)) - radius);
    if ((feather > 0.0)) {
        float t = clamp((-(sdf) / feather), 0.0, 1.0);
        return ((t * t) * (3.0 - (2.0 * t)));
    }
    float _e60 = viewport.z;
    return clamp((0.5 - (sdf * _e60)), 0.0, 1.0);
}

uint naga_f2u32(float value) {
    return uint(clamp(value, 0.0, 4294967000.0));
}

Region ConstructRegion(float4 arg0, float4 arg1, float4 arg2, float4 arg3, float4 arg4, float4 arg5, float4 arg6, float4 arg7, float4 arg8, float4 arg9) {
    Region ret = (Region)0;
    ret.round0_ = arg0;
    ret.radii0_ = arg1;
    ret.round1_ = arg2;
    ret.radii1_ = arg3;
    ret.fade_box = arg4;
    ret.fade = arg5;
    ret.color0_ = arg6;
    ret.color1_ = arg7;
    ret.color2_ = arg8;
    ret.tint = arg9;
    return ret;
}

float4 tinted(float index, float4 premultiplied)
{
    bool local = (bool)0;

    if ((index < 0.5)) {
        return premultiplied;
    }
    Region g = ConstructRegion(asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+0)), asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+16)), asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+32)), asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+48)), asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+64)), asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+80)), asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+96)), asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+112)), asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+128)), asfloat(regions.Load4((naga_f2u32(index) - 1u)*160+144)));
    if (!((g.tint.x < 0.5))) {
        local = (premultiplied.w <= 0.0);
    } else {
        local = true;
    }
    bool _e21 = local;
    if (_e21) {
        return premultiplied;
    }
    float4 c = float4((premultiplied.xyz / (premultiplied.w).xxx), 1.0);
    float3 rgb_1 = clamp(float3(dot(g.color0_, c), dot(g.color1_, c), dot(g.color2_, c)), (0.0).xxx, (1.0).xxx);
    return float4((rgb_1 * premultiplied.w), premultiplied.w);
}

float faded(float4 b_1, float4 f, float2 p_1)
{
    float k = 1.0;

    if ((f.x > 0.0)) {
        float t_1 = clamp(((p_1.y - b_1.y) / f.x), 0.0, 1.0);
        float _e16 = k;
        k = (_e16 * (t_1 * t_1));
    }
    if ((f.y > 0.0)) {
        float t_2 = clamp(((b_1.z - p_1.x) / f.y), 0.0, 1.0);
        float _e30 = k;
        k = (_e30 * (t_2 * t_2));
    }
    if ((f.z > 0.0)) {
        float t_3 = clamp(((b_1.w - p_1.y) / f.z), 0.0, 1.0);
        float _e44 = k;
        k = (_e44 * (t_3 * t_3));
    }
    if ((f.w > 0.0)) {
        float t_4 = clamp(((p_1.x - b_1.x) / f.w), 0.0, 1.0);
        float _e58 = k;
        k = (_e58 * (t_4 * t_4));
    }
    float _e61 = k;
    return _e61;
}

float region(float index_1, float2 position_1)
{
    float k_1 = (float)0;

    Region g_1 = ConstructRegion(asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+0)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+16)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+32)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+48)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+64)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+80)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+96)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+112)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+128)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+144)));
    float _e10 = viewport.z;
    float2 p_6 = (position_1 / (_e10).xx);
    const float _e17 = rounded(g_1.round0_, g_1.radii0_, g_1.tint.y, p_6);
    const float _e22 = rounded(g_1.round1_, g_1.radii1_, g_1.tint.z, p_6);
    k_1 = (_e17 * _e22);
    float _e25 = k_1;
    const float _e28 = faded(g_1.fade_box, g_1.fade, p_6);
    k_1 = (_e25 * _e28);
    if ((g_1.tint.w > 0.5)) {
        Region o_2 = ConstructRegion(asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+0)), asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+16)), asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+32)), asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+48)), asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+64)), asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+80)), asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+96)), asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+112)), asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+128)), asfloat(regions.Load4((naga_f2u32(g_1.tint.w) - 1u)*160+144)));
        float _e42 = k_1;
        const float _e45 = faded(o_2.fade_box, o_2.fade, p_6);
        k_1 = (_e42 * _e45);
    }
    float _e47 = k_1;
    return _e47;
}

float2 centred(float2 at, float4 box)
{
    float _e9 = viewport.z;
    float2 middle = (round((((box.xy + (box.zw * 0.5)) * _e9) * 64.0)) / (64.0).xx);
    float _e21 = viewport.z;
    return (abs((at - middle)) / (_e21).xx);
}

float4 shade_texel(Out input, float4 given, bool has_texel)
{
    Out v_7 = (Out)0;
    float4 color_1 = (float4)0;
    bool local_1 = (bool)0;
    bool local_2 = (bool)0;
    float corner = (float)0;
    bool local_3 = (bool)0;
    bool premultiplied_1 = false;
    float3 rgb = (0.0).xxx;
    float scale = 1.0;
    float4 tex = (float4)0;

    v_7 = input;
    float _e6 = v_7.options.z;
    if ((_e6 < 0.5)) {
        float4 _e11 = v_7.position;
        float _e15 = viewport.z;
        v_7.point_ = (_e11.xy / (_e15).xx);
    }
    float4 _e19 = v_7.color;
    color_1 = _e19;
    float _e23 = v_7.options.z;
    if ((_e23 < 0.5)) {
        float _e30 = v_7.shape.w;
        if (!((_e30 > 0.5))) {
            float _e38 = v_7.shape.z;
            local_2 = (_e38 > 0.0);
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
        float _e47 = v_7.shape.x;
        corner = _e47;
        float _e51 = v_7.options.w;
        if ((_e51 > 0.5)) {
            float _e56 = v_7.point_.x;
            float _e59 = v_7.fade.x;
            float _e62 = v_7.fade.z;
            bool right = (_e56 > (_e59 + (_e62 * 0.5)));
            float _e69 = v_7.point_.y;
            float _e72 = v_7.fade.y;
            float _e75 = v_7.fade.w;
            bool bottom = (_e69 > (_e72 + (_e75 * 0.5)));
            float _e82 = v_7.border.x;
            float _e85 = v_7.border.y;
            float _e89 = v_7.border.w;
            float _e92 = v_7.border.z;
            corner = (bottom ? (right ? _e92 : _e89) : (right ? _e85 : _e82));
        }
        float _e95 = corner;
        float _e98 = v_7.fade.z;
        float _e101 = v_7.fade.w;
        float radius_1 = clamp(_e95, 0.0, (min(_e98, _e101) * 0.5));
        float4 _e108 = v_7.position;
        float4 _e111 = v_7.fade;
        const float2 _e112 = centred(_e108.xy, _e111);
        float4 _e114 = v_7.fade;
        float2 q_1 = (_e112 - ((_e114.zw * 0.5) - (radius_1).xx));
        float distance_ = ((length(max(q_1, (0.0).xx)) + min(max(q_1.x, q_1.y), 0.0)) - radius_1);
        float2 outside = max(q_1, (0.0).xx);
        float reach = length(outside);
        float2 gradient = ((reach > 0.0) ? (outside / (max(reach, 1e-20)).xx) : ((q_1.x > q_1.y) ? float2(1.0, 0.0) : float2(0.0, 1.0)));
        float _e155 = viewport.z;
        float2 slope = (gradient / (_e155).xx);
        float _e160 = v_7.shape.y;
        if ((_e160 < 0.0)) {
            float _e165 = v_7.shape.x;
            uint a_1 = naga_f2u32(_e165);
            float _e169 = v_7.shape.z;
            uint b_3 = naga_f2u32(_e169);
            float4 sides = (float4(float((a_1 & 4095u)), float((a_1 >> 12u)), float((b_3 & 4095u)), float((b_3 >> 12u))) / (16.0).xxxx);
            float _e189 = v_7.point_.x;
            float _e192 = v_7.fade.x;
            float _e195 = v_7.fade.z;
            bool right_1 = (_e189 > (_e192 + (_e195 * 0.5)));
            float _e202 = v_7.point_.y;
            float _e205 = v_7.fade.y;
            float _e208 = v_7.fade.w;
            bool bottom_1 = (_e202 > (_e205 + (_e208 * 0.5)));
            float across = (right_1 ? sides.y : sides.w);
            float down = (bottom_1 ? sides.z : sides.x);
            float4 _e220 = v_7.fade;
            float4 _e225 = v_7.fade;
            float4 box_2 = float4((_e220.xy + sides.wx), max(((_e225.zw - sides.wx) - sides.yz), (0.0).xx));
            float inner_radius = clamp((radius_1 - ((across + down) * 0.5)), 0.0, (min(box_2.z, box_2.w) * 0.5));
            float4 _e247 = v_7.position;
            const float2 _e249 = centred(_e247.xy, box_2);
            float2 iq = (_e249 - ((box_2.zw * 0.5) - (inner_radius).xx));
            float inside = ((length(max(iq, (0.0).xx)) + min(max(iq.x, iq.y), 0.0)) - inner_radius);
            float aa = max((slope.x + slope.y), 0.5);
            if ((box_2.z > 0.0)) {
                local_3 = (box_2.w > 0.0);
            } else {
                local_3 = false;
            }
            bool _e287 = local_3;
            float hollow = (_e287 ? clamp((0.5 - (inside / aa)), 0.0, 1.0) : 0.0);
            float _e291 = color_1.w;
            color_1.w = (_e291 * max((clamp((0.5 - (distance_ / aa)), 0.0, 1.0) - hollow), 0.0));
        } else {
            float _e304 = v_7.shape.z;
            if ((_e304 > 0.0)) {
                float _e309 = v_7.shape.z;
                float x_1 = (distance_ / (_e309 * 1.4142135));
                float x2_ = (x_1 * x_1);
                float e_1 = (sign(x_1) * sqrt((1.0 - exp(((-(x2_) * (1.2732395 + (0.147 * x2_))) / (1.0 + (0.147 * x2_)))))));
                float _e332 = color_1.w;
                color_1.w = (_e332 * (0.5 - (0.5 * e_1)));
            } else {
                float dx = slope.x;
                float dy = slope.y;
                float aa_1 = max(((radius_1 <= 0.0) ? max(dx, dy) : (dx + dy)), 0.5);
                float outer = clamp((0.5 - (distance_ / aa_1)), 0.0, 1.0);
                float _e355 = v_7.shape.y;
                float inner = clamp((0.5 - ((distance_ + _e355) / aa_1)), 0.0, 1.0);
                float _e364 = color_1.w;
                float _e368 = v_7.border.w;
                float alpha = ((_e364 * inner) + (_e368 * (outer - inner)));
                float4 _e372 = color_1;
                float _e375 = color_1.w;
                float4 _e379 = v_7.border;
                float _e383 = v_7.border.w;
                float3 rgb_2 = ((alpha > 0.0) ? ((((_e372.xyz * _e375) * inner) + ((_e379.xyz * _e383) * (outer - inner))) / (max(alpha, 1e-5)).xxx) : (0.0).xxx);
                color_1 = float4(rgb_2, alpha);
            }
        }
    }
    float _e407 = v_7.options.y;
    if ((_e407 > 0.5)) {
        tex = given;
        if (!(has_texel)) {
            float2 _e415 = v_7.uv;
            float4 _e416 = atlas.Sample(tex_sampler, _e415);
            tex = _e416;
        }
        float _e419 = v_7.options.y;
        if ((_e419 > 1.5)) {
            premultiplied_1 = true;
            float4 _e423 = color_1;
            float4 _e425 = tex;
            float _e429 = color_1.w;
            rgb = ((_e423.xyz * _e425.xyz) * _e429);
            float _e433 = color_1.w;
            float _e435 = tex.w;
            color_1.w = (_e433 * _e435);
        } else {
            float4 _e437 = color_1;
            float4 _e439 = tex;
            float _e443 = color_1.w;
            float _e445 = tex.w;
            color_1 = float4((_e437.xyz * _e439.xyz), (_e443 * _e445));
        }
    }
    float _e450 = v_7.options.x;
    if ((_e450 > 0.0)) {
        float2 _e454 = v_7.point_;
        float4 _e456 = v_7.fade;
        float4 _e460 = v_7.fade;
        float4 _e463 = v_7.fade;
        float2 _e467 = v_7.point_;
        float2 distance_1 = min((_e454 - _e456.xy), ((_e460.xy + _e463.zw) - _e467));
        float _e475 = v_7.options.x;
        float f_1 = clamp((min(distance_1.x, distance_1.y) / _e475), 0.0, 1.0);
        float _e481 = color_1.w;
        color_1.w = (_e481 * f_1);
        float _e483 = scale;
        scale = (_e483 * f_1);
    }
    float _e487 = v_7.mask.x;
    if ((_e487 > 0.5)) {
        float _e492 = v_7.mask.x;
        float4 _e494 = v_7.position;
        const float _e496 = region(_e492, _e494.xy);
        float _e498 = color_1.w;
        color_1.w = (_e498 * _e496);
        float _e500 = scale;
        scale = (_e500 * _e496);
    }
    float _e504 = v_7.mask.x;
    float4 _e505 = color_1;
    float _e508 = color_1.w;
    float _e511 = color_1.w;
    float3 _e513 = rgb;
    float _e514 = scale;
    float _e517 = color_1.w;
    bool _e519 = premultiplied_1;
    const float4 _e521 = tinted(_e504, (_e519 ? float4((_e513 * _e514), _e517) : float4((_e505.xyz * _e508), _e511)));
    return _e521;
}

float4 shade(Out input_1)
{
    const float4 _e4 = shade_texel(input_1, (0.0).xxxx, false);
    return _e4;
}

float4 basic(Basic v_8)
{
    if ((v_8.texture_ > 0.5)) {
        float4 tex_1 = atlas.Sample(tex_sampler, v_8.uv);
        float a_2 = (v_8.color.w * tex_1.w);
        return float4(((v_8.color.xyz * tex_1.xyz) * ((v_8.texture_ > 1.5) ? v_8.color.w : a_2)), a_2);
    }
    return float4((v_8.color.xyz * v_8.color.w), v_8.color.w);
}

float zgx_distance(float2 p_2, float2 a, float2 b_2)
{
    float2 pa = (p_2 - a);
    float2 ba = (b_2 - a);
    return length((pa - (ba * clamp((dot(pa, ba) / max(dot(ba, ba), 1e-12)), 0.0, 1.0))));
}

float zgx_join(float d, float2 p_3, float2 v_9, float2 d0_, float2 d1_, float h, float join, float limit)
{
    bool local_4 = (bool)0;
    bool local_5 = (bool)0;
    bool local_6 = (bool)0;
    bool local_7 = (bool)0;

    float turn = ((d0_.x * d1_.y) - (d0_.y * d1_.x));
    if (!((join > 1.5))) {
        local_4 = (abs(turn) < 1e-6);
    } else {
        local_4 = true;
    }
    bool _e24 = local_4;
    if (_e24) {
        return d;
    }
    float side = ((turn > 0.0) ? -1.0 : 1.0);
    float2 n0_ = (float2(-(d0_.y), d0_.x) * side);
    float2 n1_ = (float2(-(d1_.y), d1_.x) * side);
    float2 m = normalize((n0_ + n1_));
    float cos_half = sqrt(max(((1.0 + dot(n0_, n1_)) * 0.5), 0.0));
    float2 q_2 = (p_3 - v_9);
    if ((join < 0.5)) {
        local_5 = (cos_half > 1e-6);
    } else {
        local_5 = false;
    }
    bool _e58 = local_5;
    if (_e58) {
        local_6 = ((1.0 / cos_half) <= limit);
    } else {
        local_6 = false;
    }
    bool _e65 = local_6;
    if (_e65) {
        float2 a_3 = (v_9 + (n0_ * h));
        float2 b_4 = (v_9 + (n1_ * h));
        float2 tip = (v_9 + (m * (h / cos_half)));
        float inside_1 = max(max((dot(q_2, n0_) - h), (dot(q_2, n1_) - h)), max(-(dot(q_2, d0_)), dot(q_2, d1_)));
        const float _e83 = zgx_distance(p_3, a_3, tip);
        const float _e84 = zgx_distance(p_3, tip, b_4);
        float kite = ((inside_1 <= 0.0) ? inside_1 : min(_e83, _e84));
        return min(d, kite);
    }
    if (!((dot(q_2, d0_) < 0.0))) {
        local_7 = (dot(q_2, d1_) > 0.0);
    } else {
        local_7 = true;
    }
    bool _e100 = local_7;
    if (_e100) {
        return d;
    }
    return max(d, min((dot(q_2, m) - (h * cos_half)), ((h + 1.0) - length(q_2))));
}

float zgx_cap(float d_1, float2 p_4, float2 e, float2 out_, float h_1, float cap)
{
    float2 q_3 = (p_4 - e);
    float along = dot(q_3, out_);
    if ((cap < 0.5)) {
        return max(d_1, min(along, ((h_1 + 1.0) - length(q_3))));
    }
    if ((cap > 1.5)) {
        return d_1;
    }
    float2 box_3 = (abs(float2((along - (h_1 * 0.5)), dot(q_3, float2(-(out_.y), out_.x)))) - float2((h_1 * 0.5), h_1));
    return min(d_1, (length(max(box_3, (0.0).xx)) + min(max(box_3.x, box_3.y), 0.0)));
}

uint naga_div(uint lhs, uint rhs) {
    return lhs / (rhs == 0u ? 1u : rhs);
}

uint zgx_seek(uint first, uint count, float x)
{
    uint lo = 0u;
    uint hi = (uint)0;

    hi = count;
    uint2 loop_bound = uint2(4294967295u, 4294967295u);
    while(true) {
        if (all(loop_bound == uint2(0u, 0u))) { break; }
        loop_bound -= uint2(loop_bound.y == 0u, 1u);
        uint _e6 = lo;
        uint _e7 = hi;
        if ((_e6 >= _e7)) {
            break;
        }
        uint _e9 = lo;
        uint _e10 = hi;
        uint mid = naga_div((_e9 + _e10), 2u);
        float _e18 = asfloat(points.Load(0+(first + mid)*8));
        if ((_e18 < x)) {
            lo = (mid + 1u);
        } else {
            hi = mid;
        }
    }
    uint _e22 = lo;
    return _e22;
}

float zgx_round_box(float2 p_5, float4 box_1, float4 corners)
{
    float2 q0_ = (p_5 - (box_1.xy + (box_1.zw * 0.5)));
    float r_1 = ((q0_.y < 0.0) ? ((q0_.x > 0.0) ? corners.y : corners.x) : ((q0_.x > 0.0) ? corners.z : corners.w));
    float2 q_4 = ((abs(q0_) - (box_1.zw * 0.5)) + (r_1).xx);
    return ((length(max(q_4, (0.0).xx)) + min(max(q_4.x, q_4.y), 0.0)) - r_1);
}

float4 zgx_inset(Out v_10)
{
    float4 color_2 = (float4)0;

    float _e5 = viewport.z;
    float2 p_7 = (v_10.position.xy / (_e5).xx);
    uint first_1 = (naga_f2u32(v_10.mask.y) - 1u);
    float2 offset = asfloat(points.Load2(first_1*8));
    float sigma = asfloat(points.Load(0+(first_1 + 1u)*8));
    float spread = asfloat(points.Load(4+(first_1 + 1u)*8));
    float4 hole = float4(((v_10.fade.xy + offset) + (spread).xx), max((v_10.fade.zw - ((spread * 2.0)).xx), (0.0).xx));
    const float _e49 = zgx_round_box(p_7, hole, max((v_10.border - (spread).xxxx), (0.0).xxxx));
    float x_2 = (_e49 / (sigma * 1.4142135));
    float x2_1 = (x_2 * x_2);
    float e_2 = (sign(x_2) * sqrt((1.0 - exp(((-(x2_1) * (1.2732395 + (0.147 * x2_1))) / (1.0 + (0.147 * x2_1)))))));
    float _e73 = viewport.z;
    float aa_2 = max((1.0 / _e73), 0.5);
    const float _e80 = zgx_round_box(p_7, v_10.fade, v_10.border);
    float cover = clamp((0.5 - (_e80 / aa_2)), 0.0, 1.0);
    color_2 = ((float4((v_10.color.xyz * v_10.color.w), v_10.color.w) * (0.5 + (0.5 * e_2))) * cover);
    if ((v_10.mask.x > 0.5)) {
        float4 _e106 = color_2;
        const float _e111 = region(v_10.mask.x, v_10.position.xy);
        color_2 = (_e106 * _e111);
    }
    float4 _e115 = color_2;
    const float4 _e116 = tinted(v_10.mask.x, _e115);
    return _e116;
}

float4 zgx_polyline(Out input_2)
{
    float d_2 = 1000000000.0;
    uint k_2 = (uint)0;
    uint k_3 = (uint)0;
    float4 color_3 = (float4)0;

    if ((input_2.shape.x > 1.5)) {
        const float4 _e5 = zgx_inset(input_2);
        return _e5;
    }
    float _e10 = viewport.z;
    float2 p_8 = (input_2.position.xy / (_e10).xx);
    uint first_2 = (naga_f2u32(input_2.mask.y) - 1u);
    uint count_1 = naga_f2u32(input_2.mask.z);
    float h_2 = input_2.border.x;
    float cap_1 = input_2.border.y;
    float join_1 = input_2.border.z;
    float limit_1 = input_2.border.w;
    float reach_1 = ((h_2 * max(limit_1, 1.42)) + 1.0);
    const uint _e36 = zgx_seek(first_2, count_1, (p_8.x - reach_1));
    uint lo_1 = ((_e36 == 0u) ? 0u : (_e36 - 1u));
    const uint _e47 = zgx_seek(first_2, count_1, ((p_8.x + reach_1) + 0.001));
    uint hi_1 = min(min(_e47, (count_1 - 1u)), (lo_1 + 32u));
    k_2 = lo_1;
    uint2 loop_bound_1 = uint2(4294967295u, 4294967295u);
    bool loop_init = true;
    while(true) {
        if (all(loop_bound_1 == uint2(0u, 0u))) { break; }
        loop_bound_1 -= uint2(loop_bound_1.y == 0u, 1u);
        if (!loop_init) {
            uint _e75 = k_2;
            k_2 = (_e75 + 1u);
        }
        loop_init = false;
        uint _e57 = k_2;
        if ((_e57 < hi_1)) {
        } else {
            break;
        }
        {
            float _e59 = d_2;
            uint _e61 = k_2;
            float2 _e64 = asfloat(points.Load2((first_2 + _e61)*8));
            uint _e66 = k_2;
            float2 _e71 = asfloat(points.Load2(((first_2 + _e66) + 1u)*8));
            const float _e72 = zgx_distance(p_8, _e64, _e71);
            d_2 = min(_e59, _e72);
        }
    }
    float _e77 = d_2;
    d_2 = (_e77 - h_2);
    if ((join_1 < 1.5)) {
        k_3 = max(lo_1, 1u);
        uint2 loop_bound_2 = uint2(4294967295u, 4294967295u);
        bool loop_init_1 = true;
        while(true) {
            if (all(loop_bound_2 == uint2(0u, 0u))) { break; }
            loop_bound_2 -= uint2(loop_bound_2.y == 0u, 1u);
            if (!loop_init_1) {
                uint _e112 = k_3;
                k_3 = (_e112 + 1u);
            }
            loop_init_1 = false;
            uint _e84 = k_3;
            if ((_e84 < hi_1)) {
            } else {
                break;
            }
            {
                uint _e87 = k_3;
                float2 a_4 = asfloat(points.Load2(((first_2 + _e87) - 1u)*8));
                uint _e94 = k_3;
                float2 b_5 = asfloat(points.Load2((first_2 + _e94)*8));
                uint _e99 = k_3;
                float2 c_1 = asfloat(points.Load2(((first_2 + _e99) + 1u)*8));
                float _e105 = d_2;
                const float _e110 = zgx_join(_e105, p_8, b_5, normalize((b_5 - a_4)), normalize((c_1 - b_5)), h_2, join_1, limit_1);
                d_2 = _e110;
            }
        }
    }
    if ((cap_1 < 1.5)) {
        if ((lo_1 == 0u)) {
            float _e118 = d_2;
            float2 _e121 = asfloat(points.Load2(first_2*8));
            float2 _e124 = asfloat(points.Load2(first_2*8));
            float2 _e129 = asfloat(points.Load2((first_2 + 1u)*8));
            const float _e132 = zgx_cap(_e118, p_8, _e121, normalize((_e124 - _e129)), h_2, cap_1);
            d_2 = _e132;
        }
        if ((hi_1 == (count_1 - 1u))) {
            float2 e_3 = asfloat(points.Load2(((first_2 + count_1) - 1u)*8));
            float _e142 = d_2;
            float2 _e148 = asfloat(points.Load2(((first_2 + count_1) - 2u)*8));
            const float _e151 = zgx_cap(_e142, p_8, e_3, normalize((e_3 - _e148)), h_2, cap_1);
            d_2 = _e151;
        }
    }
    float _e154 = viewport.z;
    float aa_3 = max((1.0 / _e154), 0.5);
    color_3 = input_2.color;
    float _e162 = color_3.w;
    float _e163 = d_2;
    color_3.w = (_e162 * clamp((0.5 - (_e163 / aa_3)), 0.0, 1.0));
    if ((input_2.mask.x > 0.5)) {
        float _e176 = color_3.w;
        const float _e181 = region(input_2.mask.x, input_2.position.xy);
        color_3.w = (_e176 * _e181);
    }
    float4 _e185 = color_3;
    float _e188 = color_3.w;
    float _e191 = color_3.w;
    const float4 _e193 = tinted(input_2.mask.x, float4((_e185.xyz * _e188), _e191));
    return _e193;
}

VertexOutput_vs vs(Vertex v, uint i : SV_VertexID)
{
    float2 position = (float2)0;
    float w = 1.0;
    Out o = (Out)0;

    float2 corner_1 = float2(float((i & 1u)), float((i >> 1u)));
    float2 p_9 = (v.rect.xy + (corner_1 * v.rect.zw));
    position = p_9;
    if ((v.options.z > 0.5)) {
        position = float2((dot(v.shape.xy, p_9) + v.border.x), (dot(v.shape.zw, p_9) + v.border.y));
    }
    if ((v.options.z > 1.5)) {
        w = max((dot(v.border.zw, p_9) + 1.0), 0.0001);
    }
    float _e49 = position.x;
    float _e52 = viewport.x;
    float _e56 = w;
    float _e58 = w;
    float _e60 = position.y;
    float _e63 = viewport.y;
    float _e68 = w;
    o.position = float4((((_e49 / _e52) * 2.0) - _e56), (_e58 - ((_e60 / _e63) * 2.0)), 0.0, _e68);
    o.uv = (v.uv.xy + (corner_1 * v.uv.zw));
    o.color = v.color;
    o.point_ = p_9;
    o.fade = v.fade;
    o.options = v.options;
    o.shape = v.shape;
    o.border = v.border;
    o.mask = v.mask;
    Out _e91 = o;
    const Out out_1 = _e91;
    const VertexOutput_vs out_1_ = { out_1.uv, out_1.color, out_1.point_, out_1.fade, out_1.options, out_1.shape, out_1.border, out_1.mask, out_1.position };
    return out_1_;
}

float4 fs(FragmentInput_fs fragmentinput_fs) : SV_Target0
{
    Out v_1 = { fragmentinput_fs.position_3, fragmentinput_fs.uv_2, fragmentinput_fs.color_5, fragmentinput_fs.point_1, fragmentinput_fs.fade_1, fragmentinput_fs.options_2, fragmentinput_fs.shape_1, fragmentinput_fs.border_1, fragmentinput_fs.mask_1 };
    const float4 _e1 = shade(v_1);
    return _e1;
}

Both ConstructBoth(float4 arg0, float4 arg1) {
    Both ret = (Both)0;
    ret.retained = arg0;
    ret.drawable = arg1;
    return ret;
}

Both fs_both(FragmentInput_fs_both fragmentinput_fs_both)
{
    Out v_2 = { fragmentinput_fs_both.position_4, fragmentinput_fs_both.uv_3, fragmentinput_fs_both.color_6, fragmentinput_fs_both.point_2, fragmentinput_fs_both.fade_2, fragmentinput_fs_both.options_3, fragmentinput_fs_both.shape_2, fragmentinput_fs_both.border_2, fragmentinput_fs_both.mask_2 };
    const float4 _e1 = shade(v_2);
    const Both both = ConstructBoth(_e1, _e1);
    return both;
}

VertexOutput_vs_basic vs_basic(float4 rect : LOC0, float4 uv : LOC1, float4 color : LOC2, float4 options : LOC4, uint i_1 : SV_VertexID)
{
    Basic o_1 = (Basic)0;

    float2 corner_2 = float2(float((i_1 & 1u)), float((i_1 >> 1u)));
    float2 p_10 = (rect.xy + (corner_2 * rect.zw));
    float _e21 = viewport.x;
    float _e30 = viewport.y;
    o_1.position = float4((((p_10.x / _e21) * 2.0) - 1.0), (1.0 - ((p_10.y / _e30) * 2.0)), 0.0, 1.0);
    o_1.uv = (uv.xy + (corner_2 * uv.zw));
    o_1.color = color;
    o_1.texture_ = options.y;
    Basic _e47 = o_1;
    const Basic basic_1 = _e47;
    const VertexOutput_vs_basic basic_1_ = { basic_1.uv, basic_1.color, basic_1.texture_, basic_1.position };
    return basic_1_;
}

float4 fs_basic(FragmentInput_fs_basic fragmentinput_fs_basic) : SV_Target0
{
    Basic v_3 = { fragmentinput_fs_basic.position_6, fragmentinput_fs_basic.uv_5, fragmentinput_fs_basic.color_8, fragmentinput_fs_basic.texture_1 };
    const float4 _e1 = basic(v_3);
    return _e1;
}

Both fs_basic_both(FragmentInput_fs_basic_both fragmentinput_fs_basic_both)
{
    Basic v_4 = { fragmentinput_fs_basic_both.position_7, fragmentinput_fs_basic_both.uv_6, fragmentinput_fs_basic_both.color_9, fragmentinput_fs_basic_both.texture_2 };
    const float4 _e1 = basic(v_4);
    const Both both_1 = ConstructBoth(_e1, _e1);
    return both_1;
}

float4 fs_polyline(FragmentInput_fs_polyline fragmentinput_fs_polyline) : SV_Target0
{
    Out v_5 = { fragmentinput_fs_polyline.position_8, fragmentinput_fs_polyline.uv_7, fragmentinput_fs_polyline.color_10, fragmentinput_fs_polyline.point_3, fragmentinput_fs_polyline.fade_3, fragmentinput_fs_polyline.options_4, fragmentinput_fs_polyline.shape_3, fragmentinput_fs_polyline.border_3, fragmentinput_fs_polyline.mask_3 };
    const float4 _e1 = zgx_polyline(v_5);
    return _e1;
}

Both fs_polyline_both(FragmentInput_fs_polyline_both fragmentinput_fs_polyline_both)
{
    Out v_6 = { fragmentinput_fs_polyline_both.position_9, fragmentinput_fs_polyline_both.uv_8, fragmentinput_fs_polyline_both.color_11, fragmentinput_fs_polyline_both.point_4, fragmentinput_fs_polyline_both.fade_4, fragmentinput_fs_polyline_both.options_5, fragmentinput_fs_polyline_both.shape_4, fragmentinput_fs_polyline_both.border_4, fragmentinput_fs_polyline_both.mask_4 };
    const float4 _e1 = zgx_polyline(v_6);
    const Both both_2 = ConstructBoth(_e1, _e1);
    return both_2;
}
