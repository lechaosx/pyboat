use rapier2d::prelude::{
    ColliderBuilder, ColliderHandle, ColliderSet, RigidBodyBuilder, RigidBodyHandle, RigidBodySet,
    Vector,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullParameters {
    pub mass_kg: f32,
    pub length_m: f32,
    pub beam_m: f32,
    pub bow_length_m: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hull {
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
}

pub fn hull_outline_m(parameters: HullParameters) -> [Vector; 5] {
    let half_length_m = parameters.length_m / 2.0;
    let half_beam_m = parameters.beam_m / 2.0;
    let bow_base_m = half_length_m - parameters.bow_length_m;
    [
        Vector::new(half_length_m, 0.0),
        Vector::new(bow_base_m, -half_beam_m),
        Vector::new(-half_length_m, -half_beam_m),
        Vector::new(-half_length_m, half_beam_m),
        Vector::new(bow_base_m, half_beam_m),
    ]
}

pub fn create_hull(
    parameters: HullParameters,
    bodies: &mut RigidBodySet,
    colliders: &mut ColliderSet,
) -> Hull {
    let body = bodies.insert(RigidBodyBuilder::dynamic());
    let collider = ColliderBuilder::convex_polyline(hull_outline_m(parameters).to_vec())
        .unwrap()
        .mass(parameters.mass_kg)
        .build();
    let collider = colliders.insert_with_parent(collider, body, bodies);
    Hull { body, collider }
}
