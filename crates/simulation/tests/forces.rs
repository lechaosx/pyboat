use std::f32::consts::PI;

use pyboat_simulation::{
    apparent_flow, flat_plate_drag_coefficient, flat_plate_force, flat_plate_lift_coefficient,
    hull_resistance_force, streamlined_foil_force, streamlined_lift_coefficient,
};
use rapier2d::prelude::{Rotation, Vector};

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1.0e-5 * expected.abs().max(1.0),
        "{actual} != {expected}"
    );
}

fn assert_vector_close(actual: Vector, expected: Vector) {
    assert_close(actual.x, expected.x);
    assert_close(actual.y, expected.y);
}

#[test]
fn apparent_flow_is_medium_velocity_relative_to_the_surface() {
    assert_eq!(
        apparent_flow(Vector::new(6.0, 1.0), Vector::new(2.0, -1.0)),
        Vector::new(4.0, 2.0)
    );
}

#[test]
fn flat_plate_coefficients_are_symmetric_and_include_profile_drag() {
    let angle_rad = PI / 6.0;

    assert_close(flat_plate_lift_coefficient(angle_rad), 3.0_f32.sqrt() / 2.0);
    assert_close(
        flat_plate_lift_coefficient(-angle_rad),
        -3.0_f32.sqrt() / 2.0,
    );
    assert_close(flat_plate_drag_coefficient(angle_rad, 0.02), 0.52);
    assert_close(flat_plate_drag_coefficient(-angle_rad, 0.02), 0.52);
}

#[test]
fn flat_plate_force_matches_dynamic_pressure_and_never_adds_flow_energy() {
    let density_kg_m3 = 1.225;
    let area_m2 = 8.0;
    let base_drag_coefficient = 0.02;
    let flow = Vector::new(10.0, 0.0);
    let force = flat_plate_force(
        flow,
        PI + PI / 6.0,
        density_kg_m3,
        area_m2,
        base_drag_coefficient,
    );
    let dynamic_pressure_area_n = 0.5 * density_kg_m3 * flow.length_squared() * area_m2;

    assert_vector_close(
        force,
        Vector::new(
            dynamic_pressure_area_n * 0.52,
            -dynamic_pressure_area_n * 3.0_f32.sqrt() / 2.0,
        ),
    );
    for angle_deg in -180..=180 {
        let angle_rad = (angle_deg as f32).to_radians();
        let force = flat_plate_force(
            Vector::new(3.0, -2.0),
            angle_rad,
            density_kg_m3,
            area_m2,
            base_drag_coefficient,
        );
        assert!(force.dot(Vector::new(3.0, -2.0)) >= -1.0e-4);
    }
}

#[test]
fn flat_plate_force_is_zero_without_flow_and_rotates_with_the_world() {
    assert_eq!(
        flat_plate_force(Vector::ZERO, 1.0, 1.225, 8.0, 0.02),
        Vector::ZERO
    );

    let rotation = Rotation::new(0.7);
    let flow = Vector::new(3.0, -2.0);
    let foil_angle_rad = 1.2;
    let force = flat_plate_force(flow, foil_angle_rad, 1.225, 8.0, 0.02);
    let rotated_force = flat_plate_force(
        rotation.transform_vector(flow),
        foil_angle_rad + 0.7,
        1.225,
        8.0,
        0.02,
    );

    assert_vector_close(rotated_force, rotation.transform_vector(force));
}

#[test]
fn streamlined_lift_is_linear_to_stall_and_then_falls_to_side_on() {
    let stall_angle_rad = 15.0_f32.to_radians();
    let stall_lift_coefficient = 0.8;
    let small_angle_rad = 5.0_f32.to_radians();
    let past_stall_angle_rad = 20.0_f32.to_radians();

    assert_close(
        streamlined_lift_coefficient(small_angle_rad, stall_angle_rad, stall_lift_coefficient),
        stall_lift_coefficient / 3.0,
    );
    assert!(
        streamlined_lift_coefficient(small_angle_rad, stall_angle_rad, stall_lift_coefficient,)
            > flat_plate_lift_coefficient(small_angle_rad)
    );
    assert_close(
        streamlined_lift_coefficient(stall_angle_rad, stall_angle_rad, stall_lift_coefficient),
        stall_lift_coefficient,
    );
    let past_stall = streamlined_lift_coefficient(
        past_stall_angle_rad,
        stall_angle_rad,
        stall_lift_coefficient,
    );
    assert!(past_stall > 0.0);
    assert!(past_stall < stall_lift_coefficient);
    assert_close(
        streamlined_lift_coefficient(PI / 2.0, stall_angle_rad, stall_lift_coefficient),
        0.0,
    );
}

#[test]
fn streamlined_lift_is_odd_and_repeats_for_a_reversed_foil() {
    let stall_angle_rad = 15.0_f32.to_radians();
    let stall_lift_coefficient = 0.8;
    for angle_deg in 0..=180 {
        let angle_rad = (angle_deg as f32).to_radians();
        let lift = streamlined_lift_coefficient(angle_rad, stall_angle_rad, stall_lift_coefficient);
        assert_close(
            streamlined_lift_coefficient(-angle_rad, stall_angle_rad, stall_lift_coefficient),
            -lift,
        );
        assert_close(
            streamlined_lift_coefficient(angle_rad + PI, stall_angle_rad, stall_lift_coefficient),
            lift,
        );
    }
}

#[test]
fn streamlined_foil_force_uses_the_stalled_lift_curve_and_flat_plate_drag() {
    let flow = Vector::new(2.0, 0.0);
    let density_kg_m3 = 1025.0;
    let area_m2 = 0.12;
    let angle_of_attack_rad = 5.0_f32.to_radians();
    let stall_angle_rad = 15.0_f32.to_radians();
    let stall_lift_coefficient = 0.8;
    let base_drag_coefficient = 0.02;
    let force = streamlined_foil_force(
        flow,
        PI + angle_of_attack_rad,
        density_kg_m3,
        area_m2,
        base_drag_coefficient,
        stall_angle_rad,
        stall_lift_coefficient,
    );
    let dynamic_pressure_area_n = 0.5 * density_kg_m3 * flow.length_squared() * area_m2;

    assert_vector_close(
        force,
        Vector::new(
            dynamic_pressure_area_n
                * flat_plate_drag_coefficient(angle_of_attack_rad, base_drag_coefficient),
            -dynamic_pressure_area_n * stall_lift_coefficient / 3.0,
        ),
    );
    assert!(force.dot(flow) >= 0.0);

    let rotation = Rotation::new(0.7);
    assert_vector_close(
        streamlined_foil_force(
            rotation.transform_vector(flow),
            PI + angle_of_attack_rad + 0.7,
            density_kg_m3,
            area_m2,
            base_drag_coefficient,
            stall_angle_rad,
            stall_lift_coefficient,
        ),
        rotation.transform_vector(force),
    );
}

#[test]
fn hull_resistance_is_anisotropic_dissipative_and_rotates_with_the_hull() {
    let flow = Vector::new(3.0, 4.0);
    let density_kg_m3 = 1025.0;
    let reference_area_m2 = 0.3;
    let longitudinal_coefficient = 0.08;
    let lateral_coefficient = 0.02;
    let force = hull_resistance_force(
        flow,
        0.0,
        density_kg_m3,
        reference_area_m2,
        longitudinal_coefficient,
        lateral_coefficient,
    );

    assert_vector_close(
        force,
        Vector::new(
            0.5 * density_kg_m3 * reference_area_m2 * longitudinal_coefficient * 3.0 * 3.0,
            0.5 * density_kg_m3 * reference_area_m2 * lateral_coefficient * 4.0 * 4.0,
        ),
    );
    assert!(force.dot(flow) > 0.0);
    assert_vector_close(
        hull_resistance_force(
            -flow,
            0.0,
            density_kg_m3,
            reference_area_m2,
            longitudinal_coefficient,
            lateral_coefficient,
        ),
        -force,
    );

    let rotation = Rotation::new(0.7);
    assert_vector_close(
        hull_resistance_force(
            rotation.transform_vector(flow),
            0.7,
            density_kg_m3,
            reference_area_m2,
            longitudinal_coefficient,
            lateral_coefficient,
        ),
        rotation.transform_vector(force),
    );
}
