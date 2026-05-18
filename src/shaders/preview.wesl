// Vertex and fragment shaders for showing the rendered texture.
// These perform the necessary transformations to show the rendered
// image at the correct location relative to the current viewport
// location in fractal space.
import shared::utils::rotation_matrix;
import shared::types::Transform;

@group(0) @binding(0) var<uniform> uniforms: Transform;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index) in_vertex_index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    var vertex: vec2<f32>;
    switch (in_vertex_index) {
        case 0u: {vertex = vec2<f32>(-1., -1.);}
        case 1u: {vertex = vec2<f32>(-1., 1.);}
        case 4u: {vertex = vec2<f32>(-1., 1.);}
        case 2u: {vertex = vec2<f32>(1., -1.);}
        case 5u: {vertex = vec2<f32>(1., -1.);}
        case 3u: {vertex = vec2<f32>(1., 1.);}
        default: {vertex = vec2<f32>(0., 0.);}
    }
    out.clip_position = vec4<f32>(
        vertex
            * uniforms.prescale
            * rotation_matrix(uniforms.angle)
            * uniforms.postscale
            + uniforms.offset,
        0.5, 1.0);
    switch (in_vertex_index) {
        case 0u: {vertex = vec2<f32>(0., 0.);}
        case 1u: {vertex = vec2<f32>(0., 1.);}
        case 4u: {vertex = vec2<f32>(0., 1.);}
        case 2u: {vertex = vec2<f32>(1., 0.);}
        case 5u: {vertex = vec2<f32>(1., 0.);}
        case 3u: {vertex = vec2<f32>(1., 1.);}
        default: {vertex = vec2<f32>(0., 0.);}
    }
    out.tex_coords = vertex;
    return out;
}

@group(1) @binding(0)
var img_texture: texture_2d<f32>;
@group(1) @binding(1)
var img_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(img_texture, img_sampler, in.tex_coords);
}
