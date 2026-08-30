override ATMOSPHERE_CONDITION_OUTPUT: u32 = 0u;

@fragment
fn fragment_atmosphere_conditions(input: VertexOutput) -> @location(0) vec4<f32> {
    return diagnostic_atmosphere_condition(
        initialize_playground_base(input),
        ATMOSPHERE_CONDITION_OUTPUT,
    );
}
