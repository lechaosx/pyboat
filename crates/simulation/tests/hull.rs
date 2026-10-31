use rapier2d::prelude::{ColliderSet, RigidBodySet, Vector};
use simulation::{HullParameters, create_hull, hull_outline_m};

fn reference_hull() -> HullParameters {
    HullParameters {
        mass_kg: 222.0,
        length_m: 4.0,
        beam_m: 1.5,
        bow_length_m: 0.7,
    }
}

#[test]
fn hull_outline_is_convex_symmetric_and_tapers_toward_both_ends() {
    let outline = hull_outline_m(reference_hull());
    assert_eq!(outline[0], Vector::new(2.0, 0.0));
    assert_eq!(
        outline.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
        -2.0
    );
    assert_eq!(outline.iter().map(|p| p.y.abs()).fold(0.0, f32::max), 0.75);
    for (index, point) in outline.iter().enumerate() {
        assert!(outline.contains(&Vector::new(point.x, -point.y)));
        if point.x < -1.5 || point.x > 1.5 {
            assert!(point.y.abs() < 0.75, "ends must narrow: {point:?}");
        }
        let next = outline[(index + 1) % outline.len()];
        let after = outline[(index + 2) % outline.len()];
        assert!((next - *point).perp_dot(after - next) < 0.0);
    }
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
