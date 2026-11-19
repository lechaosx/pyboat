struct BoatShadowFrame {
    matrices: array<mat4x4<f32>, 6>,
    enabled: array<vec4<f32>, 6>,
};
@group(0) @binding(9) var boat_shadow_maps: texture_depth_2d_array;
@group(0) @binding(10) var<uniform> boat_shadow_frame: BoatShadowFrame;
var<private> boat_shadow_receiver_normal: vec3<f32>;

fn boat_shadow_transmittance(position_m: vec3<f32>, source_index: u32) -> f32 {
    if boat_shadow_frame.enabled[source_index].x == 0.0 {
        return 1.0;
    }
    let periodic = boat_shadow_frame.enabled[source_index];
    var point = position_m;
    if periodic.w > 0.0 {
        point.x -= round((point.x - periodic.z) / periodic.y) * periodic.y;
    }
    let clip = boat_shadow_frame.matrices[source_index] * vec4<f32>(point, 1.0);
    let projected = clip.xyz / clip.w;
    if clip.w <= 0.0 || any(abs(projected.xy) > vec2<f32>(1.0)) || projected.z < 0.0 {
        return 1.0;
    }
    let size = vec2<i32>(textureDimensions(boat_shadow_maps));
    var depth_gradient = vec2<f32>(0.0);
    if dot(boat_shadow_receiver_normal, boat_shadow_receiver_normal) > 0.0 {
        let normal = boat_shadow_receiver_normal;
        let axis = select(vec3<f32>(0.0, 0.0, 1.0), vec3<f32>(0.0, 1.0, 0.0), abs(normal.z) > 0.9);
        let tangent = cross(normal, axis);
        let other = cross(normal, tangent);
        let a = boat_shadow_frame.matrices[source_index] * vec4<f32>(tangent, 0.0);
        let b = boat_shadow_frame.matrices[source_index] * vec4<f32>(other, 0.0);
        let da = (a.xyz - projected * a.w) / clip.w;
        let db = (b.xyz - projected * b.w) / clip.w;
        let determinant = da.x * db.y - da.y * db.x;
        if abs(determinant) > 1e-12 {
            depth_gradient = vec2<f32>(da.z * db.y - db.z * da.y, db.z * da.x - da.z * db.x) / determinant;
        }
    }
    let texel = (vec2<f32>(0.5, -0.5) * projected.xy + 0.5) * vec2<f32>(size) - 0.5;
    let origin = vec2<i32>(floor(texel));
    let weight = fract(texel);
    var visibility = vec4<f32>(0.0);
    for (var corner = 0u; corner < 4u; corner++) {
        let offset = vec2<i32>(i32(corner & 1u), i32(corner >> 1u));
        let p = clamp(origin + offset, vec2<i32>(0), size - 1);
        let depth = textureLoad(boat_shadow_maps, p, i32(source_index), 0);
        // Compare at each texel's centre on the receiver's own plane; a single
        // centre depth makes sloping cloth shadow itself across the PCF taps.
        let sample_offset = (vec2<f32>(p) - texel) * vec2<f32>(2.0, -2.0) / vec2<f32>(size);
        let receiver_depth = projected.z + dot(depth_gradient, sample_offset);
        // Cleared texels stay empty even for receivers past the caster far plane.
        visibility[corner] = select(0.0, 1.0, depth == 1.0 || receiver_depth <= depth);
    }
    return mix(mix(visibility.x, visibility.y, weight.x), mix(visibility.z, visibility.w, weight.x), weight.y);
}
