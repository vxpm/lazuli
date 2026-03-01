import package::common;
import package::render;

// Computes the diffuse attenuation factor of a channel for a given light.
fn diffuse_atten(mode: u32, light: render::Light, vertex_pos: vec3f, vertex_normal: vec3f) -> f32 {
    switch mode {
        case 0, default: {
            return 1f;
        }
        case 1: {
            let vertex_to_light = light.position - vertex_pos;
            let dot_product = dot(vertex_to_light, vertex_normal);
            return dot_product / length(vertex_to_light);
        }
        case 2: {
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
        if norm_dot_l > 0 {
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
    if channel.material_from_vertex != 0 {
        material = vertex_color;
    }

    // if no lighting, return
    if channel.lighting_enabled == 0 {
        return material;
    }

    // get ambient color
    var ambient = config.ambient[index].rgb;
    if channel.ambient_from_vertex != 0 {
        ambient = vertex_color;
    }

    // compute the light function
    var light_func = ambient;
    for (var light_idx = 0; light_idx < 8; light_idx += 1) {
        if channel.light_mask[light_idx] == 0 {
            continue;
        }

        let light = config.lights[light_idx];
        let diff_atten = diffuse_atten(channel.diffuse_atten, light, vertex_pos, vertex_normal);

        var pos_atten = 1f;
        if channel.atten != 0 {
            pos_atten = position_atten(channel.specular != 0, light, vertex_pos, vertex_normal);
        }

        light_func += light.color.rgb * diff_atten * pos_atten;
    }

    return material * clamp(light_func, vec3f(0f), vec3f(1f));
}

fn alpha_channel(vertex_pos: vec3f, vertex_normal: vec3f, vertex_alpha: f32, index: u32, config: render::Config) -> f32 {
    let channel = config.alpha_channels[index];

    // get material alpha
    var material = config.material[index].a;
    if channel.material_from_vertex != 0 {
        material = vertex_alpha;
    }

    // if no lighting, return
    if channel.lighting_enabled == 0 {
        return material;
    }

    // get ambient alpha
    var ambient = config.ambient[index].a;
    if channel.ambient_from_vertex != 0 {
        ambient = vertex_alpha;
    }

    // compute the light function
    var light_func = ambient;
    for (var light_idx = 0; light_idx < 8; light_idx += 1) {
        if channel.light_mask[light_idx] == 0 {
            continue;
        }

        let light = config.lights[light_idx];
        let diff_atten = diffuse_atten(channel.diffuse_atten, light, vertex_pos, vertex_normal);

        var pos_atten: f32 = 1f;
        if channel.atten != 0 {
            pos_atten = position_atten(channel.specular != 0, light, vertex_pos, vertex_normal);
        }

        light_func += light.color.a * diff_atten * pos_atten;
    }

    return material * clamp(light_func, 0f, 1f);
}
