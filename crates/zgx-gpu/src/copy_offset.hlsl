struct Out {
    float4 position : SV_Position;
    nointerpolation int2 shift : LOC0;
};

Texture2D<float4> source : register(t0);

struct VertexOutput_vs {
    nointerpolation int2 shift_1 : LOC0;
    float4 position : SV_Position;
};

struct FragmentInput_fs {
    nointerpolation int2 shift_2 : LOC0;
    float4 position_1 : SV_Position;
};

int2 naga_f2i32(float2 value) {
    return int2(clamp(value, -2147483600.0, 2147483500.0));
}

uint2 NagaDimensions2D(Texture2D<float4> tex)
{
    uint4 ret;
    tex.GetDimensions(0, ret.x, ret.y, ret.z);
    return ret.xy;
}

VertexOutput_vs vs(uint i : SV_VertexID, float4 rect : LOC0, float4 shift : LOC1)
{
    Out o = (Out)0;

    float2 corner = float2(float((i & 1u)), float((i >> 1u)));
    float2 pixel = (rect.xy + (corner * rect.zw));
    float2 size = float2(NagaDimensions2D(source));
    o.position = float4((((pixel.x / size.x) * 2.0) - 1.0), (1.0 - ((pixel.y / size.y) * 2.0)), 0.0, 1.0);
    o.shift = naga_f2i32(shift.xy);
    Out _e39 = o;
    const Out out_ = _e39;
    const VertexOutput_vs out_1 = { out_.shift, out_.position };
    return out_1;
}

float4 fs(FragmentInput_fs fragmentinput_fs) : SV_Target0
{
    Out v = { fragmentinput_fs.position_1, fragmentinput_fs.shift_2 };
    float4 _e8 = source.Load(int3(asint(asuint(naga_f2i32(v.position.xy)) + asuint(v.shift)), int(0)));
    return _e8;
}
