use rapier2d::prelude::{
    ColliderBuilder, ColliderHandle, ColliderSet, ImpulseJointHandle, ImpulseJointSet,
    IntegrationParameters, MassProperties, MotorModel, PhysicsWorld, RevoluteJointBuilder,
    RigidBodyBuilder, RigidBodyHandle, RigidBodySet, Rotation, Vector,
};

pub const DT_SECONDS: f32 = 1.0 / 60.0;
const CONTROL_SELECTION_TIE_TOLERANCE: f32 = 1.0e-6;

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatParameters {
    pub hull: HullParameters,
    pub sail_mass_kg: f32,
    pub boom_mass_kg: f32,
    pub boom_length_m: f32,
    pub mast_position_m: Vector,
    pub max_sail_angle_rad: f32,
    pub air_density_kg_m3: f32,
    pub sail_area_m2: f32,
    pub sail_center_of_effort_from_mast_m: f32,
    pub sail_base_drag_coefficient: f32,
    pub water_density_kg_m3: f32,
    pub keel_area_m2: f32,
    pub keel_position_m: Vector,
    pub keel_base_drag_coefficient: f32,
    pub keel_stall_angle_rad: f32,
    pub keel_stall_lift_coefficient: f32,
    pub hull_reference_area_m2: f32,
    pub hull_drag_coefficient: f32,
    pub hull_lateral_drag_coefficient: f32,
    pub rudder_mass_kg: f32,
    pub tiller_mass_kg: f32,
    pub rudder_draw_length_m: f32,
    pub tiller_length_m: f32,
    pub rudder_position_m: Vector,
    pub max_rudder_angle_rad: f32,
    pub rudder_area_m2: f32,
    pub rudder_base_drag_coefficient: f32,
    pub rudder_stall_angle_rad: f32,
    pub rudder_stall_lift_coefficient: f32,
    pub max_sail_actuator_torque_n_m: f32,
    pub max_rudder_actuator_torque_n_m: f32,
    pub sail_bearing_damping_n_m_s_rad: f32,
    pub rudder_bearing_damping_n_m_s_rad: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatMotion {
    pub position_m: Vector,
    pub velocity_m_s: Vector,
    pub angle_rad: f32,
    pub angular_velocity_rad_s: f32,
    pub sail_angle_rad: f32,
    pub sail_angular_velocity_rad_s: f32,
    pub rudder_angle_rad: f32,
    pub rudder_angular_velocity_rad_s: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActuatorTorques {
    pub sail_n_m: f32,
    pub rudder_n_m: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionIntent {
    pub forward: f32,
    pub turn: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AutoSheetIntent {
    pub power: f32,
    pub rudder: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceEffect {
    pub angle_rad: f32,
    pub drive_force_n: f32,
    pub yaw_moment_n_m: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlInput {
    pub boat_angular_velocity_rad_s: f32,
    pub free_sail_angle_rad: f32,
    pub sail_angle_rad: f32,
    pub apparent_wind_angle_from_boat_forward_rad: f32,
    pub sail_side_hysteresis_angle_rad: f32,
    pub max_rudder_angle_rad: f32,
    pub sail_effects: Vec<SurfaceEffect>,
    pub rudder_effects: Vec<SurfaceEffect>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceTargets {
    pub sheet_target_angle_rad: f32,
    pub rudder_target_angle_rad: f32,
    pub sail_target_angle_rad: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlSettings {
    pub angle_resolution_deg: u32,
    pub sail_side_hysteresis_angle_rad: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceControlCommand {
    AutoSheet(AutoSheetIntent),
    ComputedYawRate(MotionIntent),
    FixedYawRate {
        intent: MotionIntent,
        maximum_yaw_rate_rad_s: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlTelemetry {
    pub targets: SurfaceTargets,
    pub maximum_yaw_rate_rad_s: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Environment {
    pub true_wind_velocity_m_s: Vector,
    pub true_water_velocity_m_s: Vector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boat {
    pub hull: Hull,
    pub sail: RigidBodyHandle,
    pub rudder: RigidBodyHandle,
    pub sail_joint: ImpulseJointHandle,
    pub rudder_joint: ImpulseJointHandle,
}

pub struct Simulation {
    pub parameters: BoatParameters,
    pub boat: Boat,
    pub world: PhysicsWorld,
}

pub fn apparent_flow(medium_velocity_m_s: Vector, point_velocity_m_s: Vector) -> Vector {
    medium_velocity_m_s - point_velocity_m_s
}

pub fn flat_plate_lift_coefficient(angle_of_attack_rad: f32) -> f32 {
    (2.0 * angle_of_attack_rad).sin()
}

pub fn flat_plate_drag_coefficient(angle_of_attack_rad: f32, base_drag_coefficient: f32) -> f32 {
    base_drag_coefficient + 1.0 - (2.0 * angle_of_attack_rad).cos()
}

pub fn streamlined_lift_coefficient(
    angle_of_attack_rad: f32,
    stall_angle_rad: f32,
    stall_lift_coefficient: f32,
) -> f32 {
    let angle_of_attack_rad = (angle_of_attack_rad + std::f32::consts::FRAC_PI_2)
        .rem_euclid(std::f32::consts::PI)
        - std::f32::consts::FRAC_PI_2;
    let absolute_angle_rad = angle_of_attack_rad.abs();
    if absolute_angle_rad <= stall_angle_rad {
        return stall_lift_coefficient * angle_of_attack_rad / stall_angle_rad;
    }

    let separated_lift = stall_lift_coefficient
        * ((std::f32::consts::FRAC_PI_2 - absolute_angle_rad)
            / (std::f32::consts::FRAC_PI_2 - stall_angle_rad));
    separated_lift
        .max(flat_plate_lift_coefficient(angle_of_attack_rad).abs())
        .copysign(angle_of_attack_rad)
}

pub fn flat_plate_force(
    apparent_flow_m_s: Vector,
    foil_angle_rad: f32,
    fluid_density_kg_m3: f32,
    area_m2: f32,
    base_drag_coefficient: f32,
) -> Vector {
    let flow_angle_rad = apparent_flow_m_s.y.atan2(apparent_flow_m_s.x);
    let angle_of_attack_rad = foil_angle_rad - std::f32::consts::PI - flow_angle_rad;
    let dynamic_pressure_area_n =
        0.5 * fluid_density_kg_m3 * apparent_flow_m_s.length_squared() * area_m2;
    let lift_n = dynamic_pressure_area_n * flat_plate_lift_coefficient(angle_of_attack_rad);
    let drag_n = dynamic_pressure_area_n
        * flat_plate_drag_coefficient(angle_of_attack_rad, base_drag_coefficient);
    rotated(Vector::new(drag_n, -lift_n), flow_angle_rad)
}

pub fn streamlined_foil_force(
    apparent_flow_m_s: Vector,
    foil_angle_rad: f32,
    fluid_density_kg_m3: f32,
    area_m2: f32,
    base_drag_coefficient: f32,
    stall_angle_rad: f32,
    stall_lift_coefficient: f32,
) -> Vector {
    let flow_angle_rad = apparent_flow_m_s.y.atan2(apparent_flow_m_s.x);
    let angle_of_attack_rad = foil_angle_rad - std::f32::consts::PI - flow_angle_rad;
    let dynamic_pressure_area_n =
        0.5 * fluid_density_kg_m3 * apparent_flow_m_s.length_squared() * area_m2;
    let lift_n = dynamic_pressure_area_n
        * streamlined_lift_coefficient(
            angle_of_attack_rad,
            stall_angle_rad,
            stall_lift_coefficient,
        );
    let drag_n = dynamic_pressure_area_n
        * flat_plate_drag_coefficient(angle_of_attack_rad, base_drag_coefficient);
    rotated(Vector::new(drag_n, -lift_n), flow_angle_rad)
}

pub fn hull_resistance_force(
    apparent_water_m_s: Vector,
    hull_angle_rad: f32,
    fluid_density_kg_m3: f32,
    reference_area_m2: f32,
    longitudinal_coefficient: f32,
    lateral_coefficient: f32,
) -> Vector {
    let local_flow_m_s = rotated(apparent_water_m_s, -hull_angle_rad);
    let pressure_area = 0.5 * fluid_density_kg_m3 * reference_area_m2;
    let local_force_n = Vector::new(
        local_flow_m_s.x * local_flow_m_s.x.abs() * pressure_area * longitudinal_coefficient,
        local_flow_m_s.y * local_flow_m_s.y.abs() * pressure_area * lateral_coefficient,
    );
    rotated(local_force_n, hull_angle_rad)
}

fn wrapped_angle(angle_rad: f32) -> f32 {
    (angle_rad + std::f32::consts::PI).rem_euclid(2.0 * std::f32::consts::PI) - std::f32::consts::PI
}

fn free_sail_angle(hull_angle_rad: f32, apparent_wind_m_s: Vector, max_sail_angle_rad: f32) -> f32 {
    if apparent_wind_m_s == Vector::ZERO {
        return 0.0;
    }
    wrapped_angle(
        apparent_wind_m_s.y.atan2(apparent_wind_m_s.x) - hull_angle_rad - std::f32::consts::PI,
    )
    .clamp(-max_sail_angle_rad, max_sail_angle_rad)
}

fn sample_surface_effects(
    max_angle_rad: f32,
    angle_resolution_deg: u32,
    effect_at: impl Fn(f32) -> SurfaceEffect,
) -> Vec<SurfaceEffect> {
    let max_angle_deg = max_angle_rad.to_degrees().round() as i32;
    (-max_angle_deg..=max_angle_deg)
        .step_by(angle_resolution_deg as usize)
        .map(|angle_deg| effect_at((angle_deg as f32).to_radians()))
        .collect()
}

pub fn surface_control_input(
    parameters: BoatParameters,
    motion: BoatMotion,
    environment: Environment,
    angle_resolution_deg: u32,
    sail_side_hysteresis_angle_rad: f32,
) -> ControlInput {
    let apparent_wind_m_s = apparent_flow(environment.true_wind_velocity_m_s, motion.velocity_m_s);
    let apparent_water_m_s =
        apparent_flow(environment.true_water_velocity_m_s, motion.velocity_m_s);
    let boat_forward = rotated(Vector::new(1.0, 0.0), motion.angle_rad);
    let sail_effects = sample_surface_effects(
        parameters.max_sail_angle_rad,
        angle_resolution_deg,
        |angle_rad| {
            let center_world_m = rotated(
                parameters.mast_position_m
                    + rotated(
                        Vector::new(-parameters.sail_center_of_effort_from_mast_m, 0.0),
                        angle_rad,
                    ),
                motion.angle_rad,
            );
            let center_apparent_wind_m_s =
                apparent_wind_m_s - perpendicular(center_world_m) * motion.angular_velocity_rad_s;
            let force_n = flat_plate_force(
                center_apparent_wind_m_s,
                motion.angle_rad + angle_rad,
                parameters.air_density_kg_m3,
                parameters.sail_area_m2,
                parameters.sail_base_drag_coefficient,
            );
            SurfaceEffect {
                angle_rad,
                drive_force_n: force_n.dot(boat_forward),
                yaw_moment_n_m: center_world_m.perp_dot(force_n),
            }
        },
    );
    let rudder_effects = sample_surface_effects(
        parameters.max_rudder_angle_rad,
        angle_resolution_deg,
        |angle_rad| {
            let center_world_m = rotated(
                parameters.rudder_position_m
                    + rotated(
                        Vector::new(-parameters.rudder_draw_length_m / 2.0, 0.0),
                        angle_rad,
                    ),
                motion.angle_rad,
            );
            let center_apparent_water_m_s =
                apparent_water_m_s - perpendicular(center_world_m) * motion.angular_velocity_rad_s;
            let force_n = streamlined_foil_force(
                center_apparent_water_m_s,
                motion.angle_rad + angle_rad,
                parameters.water_density_kg_m3,
                parameters.rudder_area_m2,
                parameters.rudder_base_drag_coefficient,
                parameters.rudder_stall_angle_rad,
                parameters.rudder_stall_lift_coefficient,
            );
            SurfaceEffect {
                angle_rad,
                drive_force_n: force_n.dot(boat_forward),
                yaw_moment_n_m: center_world_m.perp_dot(force_n),
            }
        },
    );
    ControlInput {
        boat_angular_velocity_rad_s: motion.angular_velocity_rad_s,
        free_sail_angle_rad: free_sail_angle(
            motion.angle_rad,
            apparent_wind_m_s,
            parameters.max_sail_angle_rad,
        ),
        sail_angle_rad: motion.sail_angle_rad,
        apparent_wind_angle_from_boat_forward_rad: wrapped_angle(
            apparent_wind_m_s.y.atan2(apparent_wind_m_s.x) - motion.angle_rad,
        ),
        sail_side_hysteresis_angle_rad,
        max_rudder_angle_rad: parameters.max_rudder_angle_rad,
        sail_effects,
        rudder_effects,
    }
}

pub fn computed_maximum_yaw_rate_rad_s(
    parameters: BoatParameters,
    motion: BoatMotion,
    environment: Environment,
) -> f32 {
    let rudder_position_world_m = rotated(parameters.rudder_position_m, motion.angle_rad);
    let rudder_apparent_water_m_s = apparent_flow(
        environment.true_water_velocity_m_s,
        motion.velocity_m_s
            + perpendicular(rudder_position_world_m) * motion.angular_velocity_rad_s,
    );
    let steering_baseline_m = (parameters.rudder_position_m - parameters.keel_position_m).length();
    rudder_apparent_water_m_s.length() * parameters.max_rudder_angle_rad.tan() / steering_baseline_m
}

fn controlled_free_sail_angle(input: &ControlInput) -> f32 {
    if input.apparent_wind_angle_from_boat_forward_rad.abs() < std::f32::consts::FRAC_PI_2
        && input.apparent_wind_angle_from_boat_forward_rad.abs()
            < input.sail_side_hysteresis_angle_rad
        && input.sail_angle_rad * input.free_sail_angle_rad < 0.0
    {
        input
            .free_sail_angle_rad
            .abs()
            .copysign(input.sail_angle_rad)
    } else {
        input.free_sail_angle_rad
    }
}

fn trimmed_sail_effect(input: &ControlInput, free_sail_angle_rad: f32) -> SurfaceEffect {
    input
        .sail_effects
        .iter()
        .copied()
        .filter(|effect| {
            if free_sail_angle_rad < 0.0 {
                free_sail_angle_rad <= effect.angle_rad && effect.angle_rad <= 0.0
            } else if free_sail_angle_rad > 0.0 {
                0.0 <= effect.angle_rad && effect.angle_rad <= free_sail_angle_rad
            } else {
                effect.angle_rad == 0.0
            }
        })
        .reduce(|selected, effect| {
            if effect.drive_force_n > selected.drive_force_n
                || (effect.drive_force_n == selected.drive_force_n
                    && effect.angle_rad.abs() < selected.angle_rad.abs())
            {
                effect
            } else {
                selected
            }
        })
        .unwrap()
}

fn nearest_sail_effect(input: &ControlInput, target_angle_rad: f32) -> SurfaceEffect {
    input
        .sail_effects
        .iter()
        .copied()
        .reduce(|selected, effect| {
            let selected_error = (selected.angle_rad - target_angle_rad).abs();
            let effect_error = (effect.angle_rad - target_angle_rad).abs();
            if effect_error + CONTROL_SELECTION_TIE_TOLERANCE < selected_error {
                effect
            } else {
                selected
            }
        })
        .unwrap()
}

fn sail_target(
    input: &ControlInput,
    power: f32,
    allow_sternway: bool,
    gradual_sternway: bool,
) -> (f32, SurfaceEffect, Option<f32>) {
    if power < 0.0 && allow_sternway {
        let backed = input
            .sail_effects
            .iter()
            .copied()
            .reduce(|selected, effect| {
                if effect.drive_force_n < selected.drive_force_n
                    || (effect.drive_force_n == selected.drive_force_n
                        && effect.angle_rad.abs() < selected.angle_rad.abs())
                {
                    effect
                } else {
                    selected
                }
            })
            .unwrap();
        if !gradual_sternway {
            return (backed.angle_rad, backed, Some(backed.angle_rad));
        }
        let backing_power = (-power).clamp(0.0, 1.0);
        let target_angle_rad = input.free_sail_angle_rad
            + backing_power * (backed.angle_rad - input.free_sail_angle_rad);
        return (
            target_angle_rad,
            nearest_sail_effect(input, target_angle_rad),
            Some(target_angle_rad),
        );
    }

    let power = power.clamp(0.0, 1.0);
    let free_sail_angle_rad = controlled_free_sail_angle(input);
    let trimmed = trimmed_sail_effect(input, free_sail_angle_rad);
    let target_angle_rad = free_sail_angle_rad + power * (trimmed.angle_rad - free_sail_angle_rad);
    (
        target_angle_rad,
        nearest_sail_effect(input, target_angle_rad),
        None,
    )
}

pub fn auto_sheet_control(input: &ControlInput, intent: AutoSheetIntent) -> SurfaceTargets {
    let (sheet_target_angle_rad, _, sail_target_angle_rad) =
        sail_target(input, intent.power, false, false);
    SurfaceTargets {
        sheet_target_angle_rad,
        rudder_target_angle_rad: intent.rudder.clamp(-1.0, 1.0) * input.max_rudder_angle_rad,
        sail_target_angle_rad,
    }
}

pub fn yaw_rate_move_control(
    input: &ControlInput,
    intent: MotionIntent,
    maximum_yaw_rate_rad_s: f32,
) -> SurfaceTargets {
    let (sheet_target_angle_rad, sail_effect, sail_target_angle_rad) =
        sail_target(input, intent.forward.clamp(-1.0, 1.0), true, true);
    let mut rate_error_fraction = intent.turn.clamp(-1.0, 1.0);
    if input.boat_angular_velocity_rad_s != 0.0 {
        rate_error_fraction -= input.boat_angular_velocity_rad_s / maximum_yaw_rate_rad_s;
    }
    rate_error_fraction = rate_error_fraction.clamp(-1.0, 1.0);
    let minimum_moment_n_m = input
        .rudder_effects
        .iter()
        .map(|rudder| sail_effect.yaw_moment_n_m + rudder.yaw_moment_n_m)
        .fold(f32::INFINITY, f32::min);
    let maximum_moment_n_m = input
        .rudder_effects
        .iter()
        .map(|rudder| sail_effect.yaw_moment_n_m + rudder.yaw_moment_n_m)
        .fold(f32::NEG_INFINITY, f32::max);
    let target_moment_n_m = if rate_error_fraction < 0.0 {
        -rate_error_fraction * minimum_moment_n_m.min(0.0)
    } else {
        rate_error_fraction * maximum_moment_n_m.max(0.0)
    };
    let rudder_effect = input
        .rudder_effects
        .iter()
        .copied()
        .reduce(|selected, effect| {
            let selected_error =
                (sail_effect.yaw_moment_n_m + selected.yaw_moment_n_m - target_moment_n_m).abs();
            let effect_error =
                (sail_effect.yaw_moment_n_m + effect.yaw_moment_n_m - target_moment_n_m).abs();
            if effect_error + CONTROL_SELECTION_TIE_TOLERANCE < selected_error
                || ((effect_error - selected_error).abs() <= CONTROL_SELECTION_TIE_TOLERANCE
                    && effect.angle_rad.abs() < selected.angle_rad.abs())
            {
                effect
            } else {
                selected
            }
        })
        .unwrap();
    SurfaceTargets {
        sheet_target_angle_rad,
        rudder_target_angle_rad: rudder_effect.angle_rad,
        sail_target_angle_rad,
    }
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

fn polygon_moment_of_inertia(mass_kg: f32, points: &[Vector]) -> f32 {
    let mut weighted_radius = 0.0;
    let mut twice_area = 0.0;
    for index in 0..points.len() {
        let first = points[index];
        let second = points[(index + 1) % points.len()];
        let cross = first.perp_dot(second);
        twice_area += cross;
        weighted_radius +=
            cross * (first.length_squared() + first.dot(second) + second.length_squared());
    }
    mass_kg * weighted_radius / (6.0 * twice_area)
}

pub fn create_hull(
    parameters: HullParameters,
    bodies: &mut RigidBodySet,
    colliders: &mut ColliderSet,
) -> Hull {
    let outline = hull_outline_m(parameters);
    let mass_properties = MassProperties::new(
        Vector::ZERO,
        parameters.mass_kg,
        polygon_moment_of_inertia(parameters.mass_kg, &outline),
    );
    let body =
        bodies.insert(RigidBodyBuilder::dynamic().additional_mass_properties(mass_properties));
    let collider = ColliderBuilder::convex_polyline(outline.to_vec())
        .unwrap()
        .density(0.0)
        .build();
    let collider = colliders.insert_with_parent(collider, body, bodies);
    bodies[body].recompute_mass_properties_from_colliders(colliders);
    Hull { body, collider }
}

fn sail_center_from_mast_m(parameters: BoatParameters) -> Vector {
    Vector::new(-parameters.boom_length_m / 2.0, 0.0)
}

fn sail_hinge_moment_of_inertia_kg_m2(parameters: BoatParameters) -> f32 {
    (parameters.sail_mass_kg + parameters.boom_mass_kg) * parameters.boom_length_m.powi(2) / 3.0
}

fn sail_mass_properties(parameters: BoatParameters) -> MassProperties {
    let mass_kg = parameters.sail_mass_kg + parameters.boom_mass_kg;
    let hinge_inertia = sail_hinge_moment_of_inertia_kg_m2(parameters);
    let center = sail_center_from_mast_m(parameters);
    MassProperties::new(
        Vector::ZERO,
        mass_kg,
        hinge_inertia - mass_kg * center.length_squared(),
    )
}

fn rudder_center_from_hinge_m(parameters: BoatParameters) -> Vector {
    let mass_kg = parameters.rudder_mass_kg + parameters.tiller_mass_kg;
    Vector::new(
        (-parameters.rudder_mass_kg * parameters.rudder_draw_length_m / 2.0
            + parameters.tiller_mass_kg * parameters.tiller_length_m / 2.0)
            / mass_kg,
        0.0,
    )
}

fn rudder_hinge_moment_of_inertia_kg_m2(parameters: BoatParameters) -> f32 {
    parameters.rudder_mass_kg * parameters.rudder_draw_length_m.powi(2) / 3.0
        + parameters.tiller_mass_kg * parameters.tiller_length_m.powi(2) / 3.0
}

fn rudder_mass_properties(parameters: BoatParameters) -> MassProperties {
    let mass_kg = parameters.rudder_mass_kg + parameters.tiller_mass_kg;
    let hinge_inertia = rudder_hinge_moment_of_inertia_kg_m2(parameters);
    let center = rudder_center_from_hinge_m(parameters);
    MassProperties::new(
        Vector::ZERO,
        mass_kg,
        hinge_inertia - mass_kg * center.length_squared(),
    )
}

fn damped_control_torque(
    angle_rad: f32,
    angular_velocity_rad_s: f32,
    moment_of_inertia_kg_m2: f32,
    maximum_torque_n_m: f32,
    maximum_angle_rad: f32,
    target_angle_rad: f32,
) -> f32 {
    let stiffness_n_m_rad = maximum_torque_n_m / (maximum_angle_rad / 10.0);
    let critical_damping_n_m_s_rad = 2.0 * (stiffness_n_m_rad * moment_of_inertia_kg_m2).sqrt();
    (stiffness_n_m_rad * (target_angle_rad - angle_rad)
        - critical_damping_n_m_s_rad * angular_velocity_rad_s)
        .clamp(-maximum_torque_n_m, maximum_torque_n_m)
}

pub fn target_control_torques(
    parameters: BoatParameters,
    motion: BoatMotion,
    target_sail_angle_rad: f32,
    target_rudder_angle_rad: f32,
) -> ActuatorTorques {
    ActuatorTorques {
        sail_n_m: damped_control_torque(
            motion.sail_angle_rad,
            motion.sail_angular_velocity_rad_s,
            sail_hinge_moment_of_inertia_kg_m2(parameters),
            parameters.max_sail_actuator_torque_n_m,
            parameters.max_sail_angle_rad,
            target_sail_angle_rad,
        ),
        rudder_n_m: damped_control_torque(
            motion.rudder_angle_rad,
            motion.rudder_angular_velocity_rad_s,
            rudder_hinge_moment_of_inertia_kg_m2(parameters),
            parameters.max_rudder_actuator_torque_n_m,
            parameters.max_rudder_angle_rad,
            target_rudder_angle_rad,
        ),
    }
}

pub fn sheet_control_torques(
    parameters: BoatParameters,
    motion: BoatMotion,
    target_sail_angle_rad: f32,
    target_rudder_angle_rad: f32,
) -> ActuatorTorques {
    let torques = target_control_torques(
        parameters,
        motion,
        target_sail_angle_rad,
        target_rudder_angle_rad,
    );
    let outward_direction = if motion.sail_angle_rad == 0.0 {
        target_sail_angle_rad
    } else {
        motion.sail_angle_rad
    };
    if torques.sail_n_m * outward_direction > 0.0 {
        ActuatorTorques {
            sail_n_m: 0.0,
            ..torques
        }
    } else {
        torques
    }
}

pub fn surface_target_control_torques(
    parameters: BoatParameters,
    motion: BoatMotion,
    targets: SurfaceTargets,
) -> ActuatorTorques {
    match targets.sail_target_angle_rad {
        Some(target_sail_angle_rad) => target_control_torques(
            parameters,
            motion,
            target_sail_angle_rad,
            targets.rudder_target_angle_rad,
        ),
        None => sheet_control_torques(
            parameters,
            motion,
            targets.sheet_target_angle_rad,
            targets.rudder_target_angle_rad,
        ),
    }
}

pub fn create_boat(
    parameters: BoatParameters,
    motion: BoatMotion,
    bodies: &mut RigidBodySet,
    colliders: &mut ColliderSet,
    joints: &mut ImpulseJointSet,
) -> Boat {
    let hull = create_hull(parameters.hull, bodies, colliders);
    let sail = bodies.insert(
        RigidBodyBuilder::dynamic().additional_mass_properties(sail_mass_properties(parameters)),
    );
    let rudder = bodies.insert(
        RigidBodyBuilder::dynamic().additional_mass_properties(rudder_mass_properties(parameters)),
    );
    bodies[sail].recompute_mass_properties_from_colliders(colliders);
    bodies[rudder].recompute_mass_properties_from_colliders(colliders);
    let sail_joint = joints.insert(
        hull.body,
        sail,
        RevoluteJointBuilder::new()
            .local_anchor1(parameters.mast_position_m)
            .local_anchor2(-sail_center_from_mast_m(parameters))
            .limits([
                -parameters.max_sail_angle_rad,
                parameters.max_sail_angle_rad,
            ])
            .motor_model(MotorModel::ForceBased)
            .motor_velocity(0.0, parameters.sail_bearing_damping_n_m_s_rad),
        true,
    );
    let rudder_joint = joints.insert(
        hull.body,
        rudder,
        RevoluteJointBuilder::new()
            .local_anchor1(parameters.rudder_position_m)
            .local_anchor2(-rudder_center_from_hinge_m(parameters))
            .limits([
                -parameters.max_rudder_angle_rad,
                parameters.max_rudder_angle_rad,
            ])
            .motor_model(MotorModel::ForceBased)
            .motor_velocity(0.0, parameters.rudder_bearing_damping_n_m_s_rad),
        true,
    );
    let boat = Boat {
        hull,
        sail,
        rudder,
        sail_joint,
        rudder_joint,
    };
    restore_motion(&boat, parameters, motion, bodies);
    boat
}

fn rotated(vector: Vector, angle_rad: f32) -> Vector {
    Rotation::new(angle_rad).transform_vector(vector)
}

fn perpendicular(vector: Vector) -> Vector {
    Vector::new(-vector.y, vector.x)
}

fn restore_surface_motion(
    body: RigidBodyHandle,
    hinge_position_m: Vector,
    center_from_hinge_m: Vector,
    relative_angle_rad: f32,
    relative_angular_velocity_rad_s: f32,
    motion: BoatMotion,
    bodies: &mut RigidBodySet,
) {
    let hinge_offset_m = rotated(hinge_position_m, motion.angle_rad);
    let hinge_position_m = motion.position_m + hinge_offset_m;
    let hinge_velocity_m_s =
        motion.velocity_m_s + perpendicular(hinge_offset_m) * motion.angular_velocity_rad_s;
    let angle_rad = motion.angle_rad + relative_angle_rad;
    let angular_velocity_rad_s = motion.angular_velocity_rad_s + relative_angular_velocity_rad_s;
    let center_offset_m = rotated(center_from_hinge_m, angle_rad);
    let surface = &mut bodies[body];
    surface.set_translation(hinge_position_m + center_offset_m, true);
    surface.set_rotation(Rotation::new(angle_rad), true);
    surface.set_linvel(
        hinge_velocity_m_s + perpendicular(center_offset_m) * angular_velocity_rad_s,
        true,
    );
    surface.set_angvel(angular_velocity_rad_s, true);
}

pub fn restore_motion(
    boat: &Boat,
    parameters: BoatParameters,
    motion: BoatMotion,
    bodies: &mut RigidBodySet,
) {
    let hull = &mut bodies[boat.hull.body];
    hull.set_translation(motion.position_m, true);
    hull.set_rotation(Rotation::new(motion.angle_rad), true);
    hull.set_linvel(motion.velocity_m_s, true);
    hull.set_angvel(motion.angular_velocity_rad_s, true);
    restore_surface_motion(
        boat.sail,
        parameters.mast_position_m,
        sail_center_from_mast_m(parameters),
        motion.sail_angle_rad,
        motion.sail_angular_velocity_rad_s,
        motion,
        bodies,
    );
    restore_surface_motion(
        boat.rudder,
        parameters.rudder_position_m,
        rudder_center_from_hinge_m(parameters),
        motion.rudder_angle_rad,
        motion.rudder_angular_velocity_rad_s,
        motion,
        bodies,
    );
}

fn relative_angle(angle_rad: f32, hull_angle_rad: f32) -> f32 {
    (angle_rad - hull_angle_rad + std::f32::consts::PI).rem_euclid(2.0 * std::f32::consts::PI)
        - std::f32::consts::PI
}

pub fn read_motion(boat: &Boat, bodies: &RigidBodySet) -> BoatMotion {
    let hull = &bodies[boat.hull.body];
    let sail = &bodies[boat.sail];
    let rudder = &bodies[boat.rudder];
    BoatMotion {
        position_m: hull.translation(),
        velocity_m_s: hull.linvel(),
        angle_rad: hull.rotation().angle(),
        angular_velocity_rad_s: hull.angvel(),
        sail_angle_rad: relative_angle(sail.rotation().angle(), hull.rotation().angle()),
        sail_angular_velocity_rad_s: sail.angvel() - hull.angvel(),
        rudder_angle_rad: relative_angle(rudder.rotation().angle(), hull.rotation().angle()),
        rudder_angular_velocity_rad_s: rudder.angvel() - hull.angvel(),
    }
}

impl Simulation {
    pub fn new(parameters: BoatParameters, motion: BoatMotion) -> Self {
        let mut world = PhysicsWorld {
            gravity: Vector::ZERO,
            integration_parameters: IntegrationParameters {
                dt: DT_SECONDS,
                num_solver_iterations: 8,
                warmstart_joints: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let boat = create_boat(
            parameters,
            motion,
            &mut world.bodies,
            &mut world.colliders,
            &mut world.impulse_joints,
        );
        Self {
            parameters,
            boat,
            world,
        }
    }

    pub fn advance(&mut self, environment: Environment, actuator_torques: ActuatorTorques) {
        for body in [self.boat.hull.body, self.boat.sail, self.boat.rudder] {
            self.world.bodies[body].reset_forces(false);
            self.world.bodies[body].reset_torques(false);
        }

        let sail = &self.world.bodies[self.boat.sail];
        let sail_angle_rad = sail.rotation().angle();
        let center_of_effort_m = sail.translation()
            + rotated(
                Vector::new(
                    self.parameters.boom_length_m / 2.0
                        - self.parameters.sail_center_of_effort_from_mast_m,
                    0.0,
                ),
                sail_angle_rad,
            );
        let apparent_wind_m_s = apparent_flow(
            environment.true_wind_velocity_m_s,
            sail.velocity_at_point(center_of_effort_m),
        );
        let sail_force_n = flat_plate_force(
            apparent_wind_m_s,
            sail_angle_rad,
            self.parameters.air_density_kg_m3,
            self.parameters.sail_area_m2,
            self.parameters.sail_base_drag_coefficient,
        );
        self.world.bodies[self.boat.sail].add_force_at_point(
            sail_force_n,
            center_of_effort_m,
            true,
        );

        let rudder = &self.world.bodies[self.boat.rudder];
        let rudder_angle_rad = rudder.rotation().angle();
        let rudder_center_of_effort_m = rudder.translation()
            + rotated(
                Vector::new(-self.parameters.rudder_draw_length_m / 2.0, 0.0)
                    - rudder_center_from_hinge_m(self.parameters),
                rudder_angle_rad,
            );
        let apparent_rudder_water_m_s = apparent_flow(
            environment.true_water_velocity_m_s,
            rudder.velocity_at_point(rudder_center_of_effort_m),
        );
        let rudder_force_n = streamlined_foil_force(
            apparent_rudder_water_m_s,
            rudder_angle_rad,
            self.parameters.water_density_kg_m3,
            self.parameters.rudder_area_m2,
            self.parameters.rudder_base_drag_coefficient,
            self.parameters.rudder_stall_angle_rad,
            self.parameters.rudder_stall_lift_coefficient,
        );
        self.world.bodies[self.boat.rudder].add_force_at_point(
            rudder_force_n,
            rudder_center_of_effort_m,
            true,
        );

        let hull = &self.world.bodies[self.boat.hull.body];
        let hull_angle_rad = hull.rotation().angle();
        let keel_center_of_effort_m =
            hull.translation() + rotated(self.parameters.keel_position_m, hull_angle_rad);
        let apparent_keel_water_m_s = apparent_flow(
            environment.true_water_velocity_m_s,
            hull.velocity_at_point(keel_center_of_effort_m),
        );
        let keel_force_n = streamlined_foil_force(
            apparent_keel_water_m_s,
            hull_angle_rad,
            self.parameters.water_density_kg_m3,
            self.parameters.keel_area_m2,
            self.parameters.keel_base_drag_coefficient,
            self.parameters.keel_stall_angle_rad,
            self.parameters.keel_stall_lift_coefficient,
        );
        self.world.bodies[self.boat.hull.body].add_force_at_point(
            keel_force_n,
            keel_center_of_effort_m,
            true,
        );

        for local_position_m in [
            Vector::new(-self.parameters.hull.length_m / 2.0, 0.0),
            Vector::new(self.parameters.hull.length_m / 2.0, 0.0),
        ] {
            let hull = &self.world.bodies[self.boat.hull.body];
            let world_position_m = hull.translation() + rotated(local_position_m, hull_angle_rad);
            let apparent_water_m_s = apparent_flow(
                environment.true_water_velocity_m_s,
                hull.velocity_at_point(world_position_m),
            );
            let force_n = hull_resistance_force(
                apparent_water_m_s,
                hull_angle_rad,
                self.parameters.water_density_kg_m3,
                self.parameters.hull_reference_area_m2 / 2.0,
                self.parameters.hull_drag_coefficient,
                self.parameters.hull_lateral_drag_coefficient,
            );
            self.world.bodies[self.boat.hull.body].add_force_at_point(
                force_n,
                world_position_m,
                true,
            );
        }

        self.world.bodies[self.boat.sail].add_torque(actuator_torques.sail_n_m, true);
        self.world.bodies[self.boat.rudder].add_torque(actuator_torques.rudder_n_m, true);
        self.world.bodies[self.boat.hull.body].add_torque(
            -actuator_torques.sail_n_m - actuator_torques.rudder_n_m,
            true,
        );
        self.world.step();
    }

    pub fn advance_controlled(
        &mut self,
        environment: Environment,
        command: SurfaceControlCommand,
        settings: ControlSettings,
    ) -> ControlTelemetry {
        let motion = self.motion();
        let input = surface_control_input(
            self.parameters,
            motion,
            environment,
            settings.angle_resolution_deg,
            settings.sail_side_hysteresis_angle_rad,
        );
        let (targets, maximum_yaw_rate_rad_s) = match command {
            SurfaceControlCommand::AutoSheet(intent) => (auto_sheet_control(&input, intent), None),
            SurfaceControlCommand::ComputedYawRate(intent) => {
                let maximum_yaw_rate_rad_s =
                    computed_maximum_yaw_rate_rad_s(self.parameters, motion, environment);
                (
                    yaw_rate_move_control(&input, intent, maximum_yaw_rate_rad_s),
                    Some(maximum_yaw_rate_rad_s),
                )
            }
            SurfaceControlCommand::FixedYawRate {
                intent,
                maximum_yaw_rate_rad_s,
            } => (
                yaw_rate_move_control(&input, intent, maximum_yaw_rate_rad_s),
                Some(maximum_yaw_rate_rad_s),
            ),
        };
        let actuator_torques = surface_target_control_torques(self.parameters, motion, targets);
        self.advance(environment, actuator_torques);
        ControlTelemetry {
            targets,
            maximum_yaw_rate_rad_s,
        }
    }

    pub fn motion(&self) -> BoatMotion {
        read_motion(&self.boat, &self.world.bodies)
    }
}
