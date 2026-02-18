alias Format = u32;

const FMT_R4: Format = 0x0;
const FMT_Y8: Format = 0x1;
const FMT_RA4: Format = 0x2;
const FMT_RA8: Format = 0x3;
const FMT_RGB565: Format = 0x4;
const FMT_RGB5A3: Format = 0x5;
const FMT_RGBA8: Format = 0x6;
const FMT_A8: Format = 0x7;
const FMT_R8: Format = 0x8;
const FMT_G8: Format = 0x9;
const FMT_B8: Format = 0xA;
const FMT_RG8: Format = 0xB;
const FMT_GB8: Format = 0xC;

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var output_texture: texture_storage_2d<rgba8unorm, write>;
var<push_constant> format: Format;

@compute @workgroup_size(8, 8, 1)
fn render(@builtin(global_invocation_id) global_id: vec3u) {
    let dimensions = textureDimensions(input_texture);
    if global_id.x >= dimensions.x || global_id.y >= dimensions.y {
        return;
    }
    
    let coords = global_id.xy;
    let input = textureLoad(input_texture, coords, 0);

    var output: vec4f;
    switch (format) {
        case FMT_R4: {
            output = input.rrrr;
        }
        case FMT_Y8: {
            let intensity = dot(input, vec4f(0.257, 0.504, 0.098, 0.0)) + 16.0;
            output = vec4f(intensity);
        }
        case FMT_RA4, FMT_RA8: {
            output = input.rrra;
        }
        case FMT_RGB565: {
            output = vec4f(input.rgb, 1.0);
        }
        case FMT_A8: {
            output = input.aaaa;
        }
        case FMT_R8: {
            output = input.rrrr;
        }
        case FMT_G8: {
            output = input.gggg;
        }
        case FMT_B8: {
            output = input.bbbb;
        }
        case FMT_RG8: {
            output = input.rrrg;
        }
        case FMT_GB8: {
            output = input.gggb;
        }
        case FMT_RGB5A3, FMT_RGBA8, default: {
            output = input;
        }
    }

    textureStore(output_texture, coords, output);
}
