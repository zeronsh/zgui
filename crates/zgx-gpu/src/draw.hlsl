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

struct VertexOutput_vs {
    float2 uv_1 : LOC0;
    nointerpolation float4 color_2 : LOC1;
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
    nointerpolation float4 color_3 : LOC1;
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
    nointerpolation float4 color_4 : LOC1;
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
    nointerpolation float4 color_5 : LOC1;
    nointerpolation float texture_ : LOC2;
    float4 position_5 : SV_Position;
};

struct FragmentInput_fs_basic {
    float2 uv_5 : LOC0;
    nointerpolation float4 color_6 : LOC1;
    nointerpolation float texture_1 : LOC2;
    float4 position_6 : SV_Position;
};

struct FragmentInput_fs_basic_both {
    float2 uv_6 : LOC0;
    nointerpolation float4 color_7 : LOC1;
    nointerpolation float texture_2 : LOC2;
    float4 position_7 : SV_Position;
};

float rounded(float4 b, float4 r, float feather, float2 p)
{
    if ((b.z < 0.0)) {
        return 1.0;
    }
    float2 half_ = (b.zw * 0.5);
    float2 q = ((p - b.xy) - half_);
    float radius = ((q.y < 0.0) ? ((q.x > 0.0) ? r.y : r.x) : ((q.x > 0.0) ? r.z : r.w));
    float2 d = ((abs(q) - half_) + (radius).xx);
    float sdf = ((length(max(d, (0.0).xx)) + min(max(d.x, d.y), 0.0)) - radius);
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

float region(float index_1, float2 position_1)
{
    float k = (float)0;

    Region g_1 = ConstructRegion(asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+0)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+16)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+32)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+48)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+64)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+80)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+96)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+112)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+128)), asfloat(regions.Load4((naga_f2u32(index_1) - 1u)*160+144)));
    float _e10 = viewport.z;
    float2 p_1 = (position_1 / (_e10).xx);
    const float _e17 = rounded(g_1.round0_, g_1.radii0_, g_1.tint.y, p_1);
    const float _e22 = rounded(g_1.round1_, g_1.radii1_, g_1.tint.z, p_1);
    k = (_e17 * _e22);
    if ((g_1.fade.x > 0.0)) {
        float t_1 = clamp(((p_1.y - g_1.fade_box.y) / g_1.fade.x), 0.0, 1.0);
        float _e39 = k;
        k = (_e39 * (t_1 * t_1));
    }
    if ((g_1.fade.y > 0.0)) {
        float t_2 = clamp(((g_1.fade_box.z - p_1.x) / g_1.fade.y), 0.0, 1.0);
        float _e56 = k;
        k = (_e56 * (t_2 * t_2));
    }
    if ((g_1.fade.z > 0.0)) {
        float t_3 = clamp(((g_1.fade_box.w - p_1.y) / g_1.fade.z), 0.0, 1.0);
        float _e73 = k;
        k = (_e73 * (t_3 * t_3));
    }
    if ((g_1.fade.w > 0.0)) {
        float t_4 = clamp(((p_1.x - g_1.fade_box.x) / g_1.fade.w), 0.0, 1.0);
        float _e90 = k;
        k = (_e90 * (t_4 * t_4));
    }
    float _e93 = k;
    return _e93;
}

float4 shade_texel(Out input, float4 given, bool has_texel)
{
    Out v_5 = (Out)0;
    float4 color_1 = (float4)0;
    bool local_1 = (bool)0;
    bool local_2 = (bool)0;
    float corner = (float)0;
    bool local_3 = (bool)0;
    bool premultiplied_1 = false;
    float3 rgb = (0.0).xxx;
    float scale = 1.0;
    float4 tex = (float4)0;

    v_5 = input;
    float _e6 = v_5.options.z;
    if ((_e6 < 0.5)) {
        float4 _e11 = v_5.position;
        float _e15 = viewport.z;
        v_5.point_ = (_e11.xy / (_e15).xx);
    }
    float4 _e19 = v_5.color;
    color_1 = _e19;
    float _e23 = v_5.options.z;
    if ((_e23 < 0.5)) {
        float _e30 = v_5.shape.w;
        if (!((_e30 > 0.5))) {
            float _e38 = v_5.shape.z;
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
        float _e47 = v_5.shape.x;
        corner = _e47;
        float _e51 = v_5.options.w;
        if ((_e51 > 0.5)) {
            float _e56 = v_5.point_.x;
            float _e59 = v_5.fade.x;
            float _e62 = v_5.fade.z;
            bool right = (_e56 > (_e59 + (_e62 * 0.5)));
            float _e69 = v_5.point_.y;
            float _e72 = v_5.fade.y;
            float _e75 = v_5.fade.w;
            bool bottom = (_e69 > (_e72 + (_e75 * 0.5)));
            float _e82 = v_5.border.x;
            float _e85 = v_5.border.y;
            float _e89 = v_5.border.w;
            float _e92 = v_5.border.z;
            corner = (bottom ? (right ? _e92 : _e89) : (right ? _e85 : _e82));
        }
        float _e95 = corner;
        float _e98 = v_5.fade.z;
        float _e101 = v_5.fade.w;
        float radius_1 = clamp(_e95, 0.0, (min(_e98, _e101) * 0.5));
        float2 _e108 = v_5.point_;
        float4 _e110 = v_5.fade;
        float4 _e114 = v_5.fade;
        float4 _e121 = v_5.fade;
        float2 q_1 = (abs(((_e108 - _e110.xy) - (_e114.zw * 0.5))) - ((_e121.zw * 0.5) - (radius_1).xx));
        float distance_ = ((length(max(q_1, (0.0).xx)) + min(max(q_1.x, q_1.y), 0.0)) - radius_1);
        float _e141 = v_5.shape.y;
        if ((_e141 < 0.0)) {
            float _e146 = v_5.shape.x;
            uint a = naga_f2u32(_e146);
            float _e150 = v_5.shape.z;
            uint b_1 = naga_f2u32(_e150);
            float4 sides = (float4(float((a & 4095u)), float((a >> 12u)), float((b_1 & 4095u)), float((b_1 >> 12u))) / (16.0).xxxx);
            float _e170 = v_5.point_.x;
            float _e173 = v_5.fade.x;
            float _e176 = v_5.fade.z;
            bool right_1 = (_e170 > (_e173 + (_e176 * 0.5)));
            float _e183 = v_5.point_.y;
            float _e186 = v_5.fade.y;
            float _e189 = v_5.fade.w;
            bool bottom_1 = (_e183 > (_e186 + (_e189 * 0.5)));
            float across = (right_1 ? sides.y : sides.w);
            float down = (bottom_1 ? sides.z : sides.x);
            float4 _e201 = v_5.fade;
            float4 _e206 = v_5.fade;
            float4 box = float4((_e201.xy + sides.wx), max(((_e206.zw - sides.wx) - sides.yz), (0.0).xx));
            float inner_radius = clamp((radius_1 - ((across + down) * 0.5)), 0.0, (min(box.z, box.w) * 0.5));
            float2 _e228 = v_5.point_;
            float2 iq = (abs(((_e228 - box.xy) - (box.zw * 0.5))) - ((box.zw * 0.5) - (inner_radius).xx));
            float inside = ((length(max(iq, (0.0).xx)) + min(max(iq.x, iq.y), 0.0)) - inner_radius);
            float _e253 = abs(ddx_fine(distance_)) + abs(ddy_fine(distance_));
            float aa = max(_e253, 0.5);
            if ((box.z > 0.0)) {
                local_3 = (box.w > 0.0);
            } else {
                local_3 = false;
            }
            bool _e271 = local_3;
            float hollow = (_e271 ? clamp((0.5 - (inside / aa)), 0.0, 1.0) : 0.0);
            float _e275 = color_1.w;
            color_1.w = (_e275 * max((clamp((0.5 - (distance_ / aa)), 0.0, 1.0) - hollow), 0.0));
        } else {
            float _e288 = v_5.shape.z;
            if ((_e288 > 0.0)) {
                float _e293 = v_5.shape.z;
                float x = (distance_ / (_e293 * 1.4142135));
                float x2_ = (x * x);
                float e = (sign(x) * sqrt((1.0 - exp(((-(x2_) * (1.2732395 + (0.147 * x2_))) / (1.0 + (0.147 * x2_)))))));
                float _e316 = color_1.w;
                color_1.w = (_e316 * (0.5 - (0.5 * e)));
            } else {
                float _e322 = ddx_fine(distance_);
                float dx = abs(_e322);
                float _e324 = ddy_fine(distance_);
                float dy = abs(_e324);
                float aa_1 = max(((radius_1 <= 0.0) ? max(dx, dy) : (dx + dy)), 0.5);
                float outer = clamp((0.5 - (distance_ / aa_1)), 0.0, 1.0);
                float _e341 = v_5.shape.y;
                float inner = clamp((0.5 - ((distance_ + _e341) / aa_1)), 0.0, 1.0);
                float _e350 = color_1.w;
                float _e354 = v_5.border.w;
                float alpha = ((_e350 * inner) + (_e354 * (outer - inner)));
                float4 _e358 = color_1;
                float _e361 = color_1.w;
                float4 _e365 = v_5.border;
                float _e369 = v_5.border.w;
                float3 rgb_2 = ((alpha > 0.0) ? ((((_e358.xyz * _e361) * inner) + ((_e365.xyz * _e369) * (outer - inner))) / (max(alpha, 1e-5)).xxx) : (0.0).xxx);
                color_1 = float4(rgb_2, alpha);
            }
        }
    }
    float _e393 = v_5.options.y;
    if ((_e393 > 0.5)) {
        tex = given;
        if (!(has_texel)) {
            float2 _e401 = v_5.uv;
            float4 _e402 = atlas.Sample(tex_sampler, _e401);
            tex = _e402;
        }
        float _e405 = v_5.options.y;
        if ((_e405 > 1.5)) {
            premultiplied_1 = true;
            float4 _e409 = color_1;
            float4 _e411 = tex;
            float _e415 = color_1.w;
            rgb = ((_e409.xyz * _e411.xyz) * _e415);
            float _e419 = color_1.w;
            float _e421 = tex.w;
            color_1.w = (_e419 * _e421);
        } else {
            float4 _e423 = color_1;
            float4 _e425 = tex;
            float _e429 = color_1.w;
            float _e431 = tex.w;
            color_1 = float4((_e423.xyz * _e425.xyz), (_e429 * _e431));
        }
    }
    float _e436 = v_5.options.x;
    if ((_e436 > 0.0)) {
        float2 _e440 = v_5.point_;
        float4 _e442 = v_5.fade;
        float4 _e446 = v_5.fade;
        float4 _e449 = v_5.fade;
        float2 _e453 = v_5.point_;
        float2 distance_1 = min((_e440 - _e442.xy), ((_e446.xy + _e449.zw) - _e453));
        float _e461 = v_5.options.x;
        float f = clamp((min(distance_1.x, distance_1.y) / _e461), 0.0, 1.0);
        float _e467 = color_1.w;
        color_1.w = (_e467 * f);
        float _e469 = scale;
        scale = (_e469 * f);
    }
    float _e473 = v_5.mask.x;
    if ((_e473 > 0.5)) {
        float _e478 = v_5.mask.x;
        float4 _e480 = v_5.position;
        const float _e482 = region(_e478, _e480.xy);
        float _e484 = color_1.w;
        color_1.w = (_e484 * _e482);
        float _e486 = scale;
        scale = (_e486 * _e482);
    }
    float _e490 = v_5.mask.x;
    float4 _e491 = color_1;
    float _e494 = color_1.w;
    float _e497 = color_1.w;
    float3 _e499 = rgb;
    float _e500 = scale;
    float _e503 = color_1.w;
    bool _e505 = premultiplied_1;
    const float4 _e507 = tinted(_e490, (_e505 ? float4((_e499 * _e500), _e503) : float4((_e491.xyz * _e494), _e497)));
    return _e507;
}

float4 shade(Out input_1)
{
    const float4 _e4 = shade_texel(input_1, (0.0).xxxx, false);
    return _e4;
}

float4 basic(Basic v_6)
{
    if ((v_6.texture_ > 0.5)) {
        float4 tex_1 = atlas.Sample(tex_sampler, v_6.uv);
        float a_1 = (v_6.color.w * tex_1.w);
        return float4(((v_6.color.xyz * tex_1.xyz) * ((v_6.texture_ > 1.5) ? v_6.color.w : a_1)), a_1);
    }
    return float4((v_6.color.xyz * v_6.color.w), v_6.color.w);
}

VertexOutput_vs vs(Vertex v, uint i : SV_VertexID)
{
    float2 position = (float2)0;
    float w = 1.0;
    Out o = (Out)0;

    float2 corner_1 = float2(float((i & 1u)), float((i >> 1u)));
    float2 p_2 = (v.rect.xy + (corner_1 * v.rect.zw));
    position = p_2;
    if ((v.options.z > 0.5)) {
        position = float2((dot(v.shape.xy, p_2) + v.border.x), (dot(v.shape.zw, p_2) + v.border.y));
    }
    if ((v.options.z > 1.5)) {
        w = max((dot(v.border.zw, p_2) + 1.0), 0.0001);
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
    o.point_ = p_2;
    o.fade = v.fade;
    o.options = v.options;
    o.shape = v.shape;
    o.border = v.border;
    o.mask = v.mask;
    Out _e91 = o;
    const Out out_ = _e91;
    const VertexOutput_vs out_1 = { out_.uv, out_.color, out_.point_, out_.fade, out_.options, out_.shape, out_.border, out_.mask, out_.position };
    return out_1;
}

float4 fs(FragmentInput_fs fragmentinput_fs) : SV_Target0
{
    Out v_1 = { fragmentinput_fs.position_3, fragmentinput_fs.uv_2, fragmentinput_fs.color_3, fragmentinput_fs.point_1, fragmentinput_fs.fade_1, fragmentinput_fs.options_2, fragmentinput_fs.shape_1, fragmentinput_fs.border_1, fragmentinput_fs.mask_1 };
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
    Out v_2 = { fragmentinput_fs_both.position_4, fragmentinput_fs_both.uv_3, fragmentinput_fs_both.color_4, fragmentinput_fs_both.point_2, fragmentinput_fs_both.fade_2, fragmentinput_fs_both.options_3, fragmentinput_fs_both.shape_2, fragmentinput_fs_both.border_2, fragmentinput_fs_both.mask_2 };
    const float4 _e1 = shade(v_2);
    const Both both = ConstructBoth(_e1, _e1);
    return both;
}

VertexOutput_vs_basic vs_basic(float4 rect : LOC0, float4 uv : LOC1, float4 color : LOC2, float4 options : LOC4, uint i_1 : SV_VertexID)
{
    Basic o_1 = (Basic)0;

    float2 corner_2 = float2(float((i_1 & 1u)), float((i_1 >> 1u)));
    float2 p_3 = (rect.xy + (corner_2 * rect.zw));
    float _e21 = viewport.x;
    float _e30 = viewport.y;
    o_1.position = float4((((p_3.x / _e21) * 2.0) - 1.0), (1.0 - ((p_3.y / _e30) * 2.0)), 0.0, 1.0);
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
    Basic v_3 = { fragmentinput_fs_basic.position_6, fragmentinput_fs_basic.uv_5, fragmentinput_fs_basic.color_6, fragmentinput_fs_basic.texture_1 };
    const float4 _e1 = basic(v_3);
    return _e1;
}

Both fs_basic_both(FragmentInput_fs_basic_both fragmentinput_fs_basic_both)
{
    Basic v_4 = { fragmentinput_fs_basic_both.position_7, fragmentinput_fs_basic_both.uv_6, fragmentinput_fs_basic_both.color_7, fragmentinput_fs_basic_both.texture_2 };
    const float4 _e1 = basic(v_4);
    const Both both_1 = ConstructBoth(_e1, _e1);
    return both_1;
}
