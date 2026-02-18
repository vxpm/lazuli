alias Format = u32;

const FMT_Z4: Format = 0x0;
const FMT_Z8: Format = 0x1;
const FMT_Z16C: Format = 0x3;
const FMT_Z24X8: Format = 0x6;
const FMT_Z8H: Format = 0x8;
const FMT_Z8M: Format = 0x9;
const FMT_Z8L: Format = 0xA;
const FMT_Z16A: Format = 0xB;
const FMT_Z16B: Format = 0xC;

const DEPTH_MAX: f32 = 16777215.0;

@group(0) @binding(0) var depth_texture: texture_2d<f32>;
@group(0) @binding(1) var color_texture: texture_storage_2d<rgba8unorm, write>;
var<push_constant> format: Format;

@compute @workgroup_size(8, 8, 1)
fn render(@builtin(global_invocation_id) global_id: vec3u) {
    let dimensions = textureDimensions(depth_texture);
    if global_id.x >= dimensions.x || global_id.y >= dimensions.y {
        return;
    }
    
    let coords = global_id.xy;
    let depth = u32(textureLoad(depth_texture, coords, 0).r * DEPTH_MAX);
    
    // swizzle to bgra because r should be most significant byte, not least
    let bytes = unpack4xU8(depth).bgra;

    var color: vec4u;
    switch (format) {
        case FMT_Z8: {
            // I8, I = R?
            color = bytes.rrrr;
        }
        case FMT_Z8H: {
            color = bytes.rrrr;   
        }
        case FMT_Z8M: {
            color = bytes.gggg;
        }
        case FMT_Z8L: {
            color = bytes.bbbb;
        }
        case FMT_Z16A, FMT_Z16C: {
            // IA8, I = R, A = G
            color = bytes.rrrg;
        }
        case FMT_Z16B: {
            // IA8, I = G, A = R
            color = bytes.gggr;
        }
        case FMT_Z24X8: {
            color = vec4u(bytes.rgb, 255);
        }
        default: {
            color = 255 * vec4u(coords, 0, 1) / vec4u(dimensions, 1, 1);
        }
    }

    let conv = vec4f(color) / 255.0;
    textureStore(color_texture, coords, conv);
}
