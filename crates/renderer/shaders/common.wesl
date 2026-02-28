const DEPTH_MAX: u32 = (1 << 24) - 1;

const PLACEHOLDER_RGB: vec3f = vec3f(1.0, 0.0, 0.8627);
const PLACEHOLDER_RGBA: vec4f = vec4f(1.0, 0.0, 0.8627, 0.5);

fn unorm_to_uint(value: f32) -> u32 {
    return u32(value * 255.0);
}

fn vec3f_to_vec3u(value: vec3f) -> vec3u {
    return vec3u(
        unorm_to_uint(value.r),
        unorm_to_uint(value.g),
        unorm_to_uint(value.b),
    );
}

fn vec4f_to_vec4u(value: vec4f) -> vec4u {
    return vec4u(
        unorm_to_uint(value.r),
        unorm_to_uint(value.g),
        unorm_to_uint(value.b),
        unorm_to_uint(value.a),
    );
}

