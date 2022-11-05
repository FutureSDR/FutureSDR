@group(0)
@binding(0)
var<storage, read_write> v_indices: array<f32>;

@compute
@workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    v_indices[global_id.x] = 12.0 * v_indices[global_id.x];
}
