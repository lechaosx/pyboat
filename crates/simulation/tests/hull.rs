use pyboat_simulation::{HullParameters, create_hull, hull_outline_m};
use rapier2d::prelude::{ColliderSet, RigidBodySet, Vector};

fn reference_hull() -> HullParameters {
    HullParameters {
        mass_kg: 222.0,
        length_m: 4.0,
        beam_m: 1.5,
        bow_length_m: 0.7,
    }
}

#[test]
fn matches_the_python_reference_outline() {
    assert_eq!(
        hull_outline_m(reference_hull()),
        [
            Vector::new(2.0, 0.0),
            Vector::new(1.3, -0.75),
            Vector::new(-2.0, -0.75),
            Vector::new(-2.0, 0.75),
            Vector::new(1.3, 0.75),
        ],
    );
}

#[test]
fn creates_a_dynamic_rapier_hull_with_the_configured_mass() {
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();

    let hull = create_hull(reference_hull(), &mut bodies, &mut colliders);

    assert!(bodies[hull.body].is_dynamic());
    assert_eq!(bodies[hull.body].mass(), 222.0);
    assert_eq!(colliders[hull.collider].parent(), Some(hull.body));
}
