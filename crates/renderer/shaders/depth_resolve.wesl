struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var depth_texture: texture_depth_multisampled_2d;
var<immediate> uvs: vec4f;

var<private> POSITIONS: array<vec2f, 4> = array<vec2f, 4>(
    vec2f(-1.0, 1.0),
    vec2f(-1.0, -1.0),
    vec2f(1.0, 1.0),
    vec2f(1.0, -1.0),
);

@vertex
fn vs_main(
    @builtin(vertex_index) index: u32,
) -> VertexOutput {
    let top_left = uvs.xy;
    let bottom_right = uvs.zw;

    let uvs = array<vec2f, 4>(
        top_left,
        vec2f(top_left.x, bottom_right.y),
        vec2f(bottom_right.x, top_left.y),
        bottom_right
    );

    return VertexOutput(vec4f(POSITIONS[index], 0.0, 1.0), uvs[index]);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) f32 {
    // compute the coordinates of the texture texel
    let dimensions = textureDimensions(depth_texture);
    let x = u32(floor(f32(dimensions.x) * in.uv.x));
    let y = u32(floor(f32(dimensions.y) * in.uv.y));

    // compute mean of samples
    let samples = textureNumSamples(depth_texture);
    var acc = 0f;
    var min_sample = 1f;
    var max_sample = 0f;
    for (var i: u32 = 0; i < samples; i += 1) {
        let sample = 1.0 - textureLoad(depth_texture, vec2u(x, y), i);
        acc += sample;
        min_sample = min(min_sample, sample);
        max_sample = max(max_sample, sample);
    }

    let threshold = min_sample * 0.05;
    let distance = max_sample - min_sample;
    if distance >= threshold {
        return min_sample;
    } else {
        return acc / f32(samples);
    }
}
