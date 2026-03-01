import package::common;
import package::render;

fn channel_material_from_vertex(channel: u32) -> bool {
    return common::get_bit(channel, 0u);
}

fn channel_ambient_from_vertex(channel: u32) -> bool {
    return common::get_bit(channel, 1u);
}

fn channel_lighting_enabled(channel: u32) -> bool {
    return common::get_bit(channel, 2u);
}

fn channel_diffuse_atten(channel: u32) -> u32 {
    return extractBits(channel, 3u, 2u);
}

fn channel_position_atten(channel: u32) -> bool {
    return common::get_bit(channel, 5u);
}

fn channel_specular(channel: u32) -> bool {
    return common::get_bit(channel, 6u);
}

fn channel_light_enabled(channel: u32, index: u32) -> bool {
    return common::get_bit(channel, 7u + index);
}

// Computes the diffuse attenuation factor of a channel for a given light.
fn diffuse_atten(mode: u32, light: render::Light, vertex_pos: vec3f, vertex_normal: vec3f) -> f32 {
    switch mode {
        case 0u, default: {
            return 1f;
        }
        case 1u: {
            let vertex_to_light = light.position - vertex_pos;
            let dot_product = dot(vertex_to_light, vertex_normal);
            return dot_product / length(vertex_to_light);
        }
        case 2u: {
            let vertex_to_light = light.position - vertex_pos;
            let dot_product = dot(vertex_to_light, vertex_normal);
            return max(dot_product / length(vertex_to_light), 0f);
        }
    }
}

// Computes the position attenuation factor of a channel for a given light.
fn position_atten(specular: bool, light: render::Light, vertex_pos: vec3f, vertex_normal: vec3f) -> f32 {
    if specular {
        let l = normalize(light.position);
        let h = light.direction;
        let norm_dot_l = dot(vertex_normal, l);

        var value = 0f;
        if norm_dot_l > 0f {
            let norm_dot_h = dot(vertex_normal, h);
            value = max(norm_dot_h, 0f);
        }

        let ang_atten = max(light.cos_atten.x + value * light.cos_atten.y + value * value * light.cos_atten.z, 0f);
        let dist_atten = light.dist_atten.x + value * light.dist_atten.y + value * value * light.dist_atten.z;

        return ang_atten / dist_atten;
    } else {
        let vertex_to_light = light.position - vertex_pos;
        let vertex_to_light_dir = normalize(vertex_to_light);

        let cos = max(dot(vertex_to_light_dir, light.direction), 0f);
        let dist = length(vertex_to_light);

        let ang_atten = max(light.cos_atten.x + cos * light.cos_atten.y + cos * cos * light.cos_atten.z, 0f);
        let dist_atten = light.dist_atten.x + dist * light.dist_atten.y + dist * dist * light.dist_atten.z;

        return ang_atten / dist_atten;
    }
}

fn color_channel(vertex_pos: vec3f, vertex_normal: vec3f, vertex_color: vec3f, index: u32, config: render::Config) -> vec3f {
    let channel = config.color_channels[index];

    // get material color
    var material = config.material[index].rgb;
    if channel_material_from_vertex(channel) {
        material = vertex_color;
    }

    // if no lighting, return
    if !channel_lighting_enabled(channel) {
        return material;
    }

    // get ambient color
    var ambient = config.ambient[index].rgb;
    if channel_ambient_from_vertex(channel) {
        ambient = vertex_color;
    }

    // compute the light function
    var light_func = ambient;
    for (var light_idx = 0u; light_idx < 8u; light_idx += 1u) {
        if !channel_light_enabled(channel, light_idx) {
            continue;
        }

        let light = config.lights[light_idx];
        let diff_atten = diffuse_atten(channel_diffuse_atten(channel), light, vertex_pos, vertex_normal);

        var pos_atten = 1f;
        if channel_position_atten(channel) {
            pos_atten = position_atten(channel_specular(channel), light, vertex_pos, vertex_normal);
        }

        light_func += light.color.rgb * diff_atten * pos_atten;
    }

    return material * clamp(light_func, vec3f(0f), vec3f(1f));
}

fn alpha_channel(vertex_pos: vec3f, vertex_normal: vec3f, vertex_alpha: f32, index: u32, config: render::Config) -> f32 {
    let channel = config.alpha_channels[index];

    // get material alpha
    var material = config.material[index].a;
    if channel_material_from_vertex(channel) {
        material = vertex_alpha;
    }

    // if no lighting, return
    if !channel_lighting_enabled(channel) {
        return material;
    }

    // get ambient alpha
    var ambient = config.ambient[index].a;
    if channel_ambient_from_vertex(channel) {
        ambient = vertex_alpha;
    }

    // compute the light function
    var light_func = ambient;
    for (var light_idx = 0u; light_idx < 8u; light_idx += 1u) {
        if !channel_light_enabled(channel, light_idx) {
            continue;
        }

        let light = config.lights[light_idx];
        let diff_atten = diffuse_atten(channel_diffuse_atten(channel), light, vertex_pos, vertex_normal);

        var pos_atten: f32 = 1f;
        if channel_position_atten(channel) {
            pos_atten = position_atten(channel_specular(channel), light, vertex_pos, vertex_normal);
        }

        light_func += light.color.a * diff_atten * pos_atten;
    }

    return material * clamp(light_func, 0f, 1f);
}
