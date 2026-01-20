// Vertex shader
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    // Taken from https://github.com/gfx-rs/wgpu/blob/trunk/examples/features/src/ray_cube_fragment/shader.wgsl
    // This is just to cover the screen
    var result: VertexOutput;
    let x = i32(vertex_index) / 2;
    let y = i32(vertex_index) & 1;
    let tc = vec2<f32>(
        f32(x) * 2.0,
        f32(y) * 2.0
    );
    result.clip_position = vec4<f32>(
        tc.x * 2.0 - 1.0,
        1.0 - tc.y * 2.0,
        0.0, 1.0
    );
    return result;
}
