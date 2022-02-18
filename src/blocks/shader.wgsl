struct Indices {
    data: array<f32>;
};

@group(0) @binding(0)
var<storage, read_write> v_indices: Indices;


@stage(compute) @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    v_indices.data[global_id.x] = 12.0 * v_indices.data[global_id.x];
}