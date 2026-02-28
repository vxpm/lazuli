import package::common;

struct Params {
    color: vec4f,
    a: f32,
    b_mag: u32,
    b_shift: u32,
    c: f32,
}

fn orthographic_distance(params: Params, depth: f32) -> f32 {
    return clamp(params.a * depth - params.c, 0f, 1f);
}

fn perspective_distance(params: Params, depth: f32) -> f32 {
    let depth_max = f32(common::DEPTH_MAX);

    // dennormalize
    let depth_denorm = depth * depth_max;
    let a_denorm = params.a * depth_max;
    
    // compute
    let divisor = f32(params.b_mag - (u32(depth_denorm) >> params.b_shift));
    return clamp(a_denorm / divisor - params.c, 0f, 1f);
}

fn exponential(distance: f32) -> f32 {
    return 1f - pow(2f, -8f * distance);
}

fn exponential_squared(distance: f32) -> f32 {
    return 1f - pow(2f, -8f * pow(distance, 2f));
}

fn inverse_exponential(distance: f32) -> f32 {
    return pow(2f, -8f * (1f - distance));
}

fn inverse_exponential_squared(distance: f32) -> f32 {
    return pow(2f, -8f * pow(1f - distance, 2f));
}
