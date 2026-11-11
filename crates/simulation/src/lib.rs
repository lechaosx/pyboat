use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    error::Error,
    fmt,
    ops::RangeInclusive,
};

use rapier2d::prelude::{
    ColliderBuilder, ColliderHandle, ColliderSet, GenericJoint, ImpulseJointHandle,
    ImpulseJointSet, IntegrationParameters, JointAxesMask, JointAxis, MassProperties, MotorModel,
    PhysicsWorld, RevoluteJointBuilder, RigidBody, RigidBodyBuilder, RigidBodyHandle, RigidBodySet,
    RigidBodyType, Rotation, Vector,
};
use serde::{Deserialize, Deserializer, Serialize, de};

pub const TICKS_PER_SECOND: u64 = 60;
pub const DT_SECONDS: f64 = (1.0_f32 / TICKS_PER_SECOND as f32) as f64;
// Opposing hull extrema at this speed remain below half the minimum supported beam per 1/32 tick.
pub const MAXIMUM_SHARED_HULL_POINT_SPEED_M_S: f64 = 700.0;
const CONTROL_SELECTION_TIE_TOLERANCE: f64 = 1.0e-6_f32 as f64;
const GRAVITY_ACCELERATION_M_S2: f64 = 9.81_f32 as f64;
const STATIC_GROUND_SPEED_THRESHOLD_M_S: f64 = 0.02_f32 as f64;
const CONTACT_CHART_HYSTERESIS_M: f64 = 0.5;
const SPATIAL_CELL_M: f64 = 16.0;
const MAXIMUM_COLLISION_SUBSTEPS: usize = 32;
const MINIMUM_SHARED_HULL_BEAM_M: f64 = 1.5;

const DEFAULT_WORLD_SIDE_M: f64 = 64_000.0;
const MINIMUM_WORLD_SIDE_M: f64 = 8_000.0;
const MAXIMUM_WORLD_SIDE_M: f64 = 256_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct WorldGeometry {
    width_m: f64,
    height_m: f64,
}

impl WorldGeometry {
    pub fn new(width_m: f64, height_m: f64) -> Result<Self, WorldGeometryError> {
        if !valid_world_dimension(width_m) || !valid_world_dimension(height_m) {
            return Err(WorldGeometryError);
        }
        Ok(Self { width_m, height_m })
    }

    pub fn width_m(self) -> f64 {
        self.width_m
    }

    pub fn height_m(self) -> f64 {
        self.height_m
    }

    pub fn wrap_position_m(self, position_m: [f64; 2]) -> [f64; 2] {
        [wrap_component(position_m[0], self.width_m), position_m[1]]
    }

    pub fn minimum_image_displacement_m(self, displacement_m: [f64; 2]) -> [f64; 2] {
        [
            minimum_image_component(displacement_m[0], self.width_m),
            displacement_m[1],
        ]
    }

    pub fn normalized_world_coordinates(self, position_m: [f64; 2]) -> [f64; 2] {
        let wrapped_m = self.wrap_position_m(position_m);
        [
            normalized_component(wrapped_m[0], self.width_m),
            (wrapped_m[1] / self.height_m + 0.5).clamp(0.0, 1.0),
        ]
    }

    pub fn latitude_degrees(self, y_m: f64) -> f64 {
        let degrees = self.latitude_phase_degrees(y_m);
        if degrees > 90.0 {
            180.0 - degrees
        } else if degrees < -90.0 {
            -180.0 - degrees
        } else {
            degrees
        }
    }

    pub fn latitude_gradient_degrees_per_m(self, y_m: f64) -> f64 {
        let direction = if self.latitude_phase_degrees(y_m).abs() > 90.0 {
            -1.0
        } else {
            1.0
        };
        direction * 180.0 / self.height_m
    }

    pub fn latitude_phase_degrees(self, y_m: f64) -> f64 {
        let period_m = 2.0 * self.height_m;
        let cycle_y_m = y_m.rem_euclid(period_m);
        let degrees = 180.0 * cycle_y_m / self.height_m;
        if degrees >= 180.0 {
            degrees - 360.0
        } else {
            degrees
        }
    }

    pub fn local_solar_phase(self, global_day_phase: f64, x_m: f64) -> f64 {
        (global_day_phase.rem_euclid(1.0) + x_m.rem_euclid(self.width_m) / self.width_m)
            .rem_euclid(1.0)
    }
}

impl Default for WorldGeometry {
    fn default() -> Self {
        Self {
            width_m: DEFAULT_WORLD_SIDE_M,
            height_m: DEFAULT_WORLD_SIDE_M,
        }
    }
}

impl<'de> Deserialize<'de> for WorldGeometry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct SerializedWorldGeometry {
            width_m: f64,
            height_m: f64,
        }

        let geometry = SerializedWorldGeometry::deserialize(deserializer)?;
        Self::new(geometry.width_m, geometry.height_m).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldGeometryError;

impl fmt::Display for WorldGeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .write_str("world dimensions must be finite values from 8000 through 256000 metres")
    }
}

impl Error for WorldGeometryError {}

fn valid_world_dimension(dimension_m: f64) -> bool {
    let half_m = dimension_m / 2.0;
    dimension_m.is_finite()
        && (MINIMUM_WORLD_SIDE_M..=MAXIMUM_WORLD_SIDE_M).contains(&dimension_m)
        && half_m * 2.0 == dimension_m
}

fn minimum_image_component(displacement_m: f64, period_m: f64) -> f64 {
    let half_period_m = period_m / 2.0;
    if displacement_m > -half_period_m && displacement_m <= half_period_m {
        return displacement_m;
    }
    let wrapped_m = displacement_m.rem_euclid(period_m);
    if wrapped_m > half_period_m {
        wrapped_m - period_m
    } else {
        wrapped_m
    }
}

fn wrap_component(value: f64, period: f64) -> f64 {
    let half_period = period / 2.0;
    if value >= -half_period && value < half_period {
        return value;
    }
    let wrapped = value.rem_euclid(period);
    if wrapped >= half_period {
        wrapped - period
    } else {
        wrapped
    }
}

fn normalized_component(wrapped: f64, period: f64) -> f64 {
    const UNIT_UPPER_BOUND: f64 = f64::from_bits(1.0_f64.to_bits() - 1);
    (wrapped / period + 0.5).min(UNIT_UPPER_BOUND)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullParameters {
    pub mass_kg: f64,
    pub length_m: f64,
    pub beam_m: f64,
    pub bow_length_m: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hull {
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatParameters {
    pub hull: HullParameters,
    pub sail_mass_kg: f64,
    pub boom_mass_kg: f64,
    pub boom_length_m: f64,
    pub mast_position_m: Vector,
    pub max_sail_angle_rad: f64,
    pub air_density_kg_m3: f64,
    pub sail_area_m2: f64,
    pub sail_base_drag_coefficient: f64,
    pub water_density_kg_m3: f64,
    pub keel_area_m2: f64,
    pub keel_position_m: Vector,
    pub keel_base_drag_coefficient: f64,
    pub keel_stall_angle_rad: f64,
    pub keel_stall_lift_coefficient: f64,
    pub hull_reference_area_m2: f64,
    pub hull_drag_coefficient: f64,
    pub hull_lateral_drag_coefficient: f64,
    pub rudder_mass_kg: f64,
    pub tiller_mass_kg: f64,
    pub rudder_draw_length_m: f64,
    pub tiller_length_m: f64,
    pub rudder_position_m: Vector,
    pub max_rudder_angle_rad: f64,
    pub rudder_area_m2: f64,
    pub rudder_base_drag_coefficient: f64,
    pub max_sail_actuator_torque_n_m: f64,
    pub max_tiller_hand_force_n: f64,
    pub sail_bearing_damping_n_m_s_rad: f64,
    pub rudder_bearing_damping_n_m_s_rad: f64,
    pub draft_m: f64,
    pub ground_static_friction_coefficient: f64,
    pub ground_kinetic_friction_coefficient: f64,
}

impl BoatParameters {
    pub fn max_rudder_actuator_torque_n_m(self) -> f64 {
        // An ideal sailor pushes perpendicular to the tiller at its tip.
        self.max_tiller_hand_force_n * self.tiller_length_m
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatMotion {
    pub position_m: Vector,
    pub velocity_m_s: Vector,
    pub angle_rad: f64,
    pub angular_velocity_rad_s: f64,
    pub sail_angle_rad: f64,
    pub sail_angular_velocity_rad_s: f64,
    pub rudder_angle_rad: f64,
    pub rudder_angular_velocity_rad_s: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActuatorTorques {
    pub sail_n_m: f64,
    pub rudder_n_m: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MotionIntent {
    pub forward: f64,
    pub turn: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AutoSheetIntent {
    pub power: f64,
    pub rudder: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceEffect {
    pub angle_rad: f64,
    pub drive_force_n: f64,
    pub yaw_moment_n_m: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ControlInput {
    pub boat_angular_velocity_rad_s: f64,
    pub free_sail_angle_rad: f64,
    pub sail_angle_rad: f64,
    pub apparent_wind_angle_from_boat_forward_rad: f64,
    pub sail_side_hysteresis_angle_rad: f64,
    pub max_rudder_angle_rad: f64,
    pub sail_effects: Vec<SurfaceEffect>,
    pub rudder_effects: Vec<SurfaceEffect>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BoatControlDynamics {
    pub locked_inertia_kg_m2: f64,
    pub total_mass_kg: f64,
    pub sail_hinge_inertia_kg_m2: f64,
    pub sail_yaw_coupling_kg_m2: f64,
    pub sail_relative_inertia_kg_m2: f64,
    pub sail_rudder_coupling_kg_m2: f64,
    pub center_of_mass_from_hull_m: Vector,
    pub rudder_yaw_coupling_kg_m2: f64,
    pub rudder_relative_inertia_kg_m2: f64,
    pub rudder_hinge_inertia_kg_m2: f64,
    pub fluid_hinge_torques_n_m: [f64; 2],
    pub fluid_generalized_torques_n_m: [f64; 2],
    pub foil_forward_force_n: f64,
    pub fluid_forward_force_n: f64,
    pub fluid_yaw_moments_n_m: [f64; 2],
    pub passive_yaw_moment_n_m: f64,
    pub rudder_motor_stiffness_n_m_rad: f64,
    pub rudder_motor_damping_n_m_s_rad: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceTargets {
    pub sheet_target_angle_rad: f64,
    pub rudder_target_angle_rad: f64,
    pub sail_target_angle_rad: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlSettings {
    pub angle_resolution_deg: u32,
    pub sail_side_hysteresis_angle_rad: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceControlCommand {
    AutoSheet(AutoSheetIntent),
    ComputedYawRate(MotionIntent),
    FixedYawRate {
        intent: MotionIntent,
        maximum_yaw_rate_rad_s: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlTelemetry {
    pub targets: SurfaceTargets,
    pub maximum_yaw_rate_rad_s: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub true_wind_velocity_m_s: Vector,
    pub true_water_velocity_m_s: Vector,
    pub water_depth_m: f64,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            true_wind_velocity_m_s: Vector::ZERO,
            true_water_velocity_m_s: Vector::ZERO,
            water_depth_m: f64::INFINITY,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boat {
    pub hull: Hull,
    pub sail: RigidBodyHandle,
    pub rudder: RigidBodyHandle,
    pub sail_joint: ImpulseJointHandle,
    pub rudder_joint: ImpulseJointHandle,
    pub rudder_actuator: ImpulseJointHandle,
}

pub struct Simulation {
    pub parameters: BoatParameters,
    pub boat: Boat,
    pub world: PhysicsWorld,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullContact {
    pub first_boat: usize,
    pub second_boat: usize,
    pub impulse_magnitude_n_s: f64,
    pub first_contact_local_m: Vector,
    pub second_contact_local_m: Vector,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SharedBoat {
    parameters: BoatParameters,
    boat: Boat,
}

struct CylindricalSpatialIndex {
    world_geometry: WorldGeometry,
    columns: i32,
    cell_width_m: f64,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl CylindricalSpatialIndex {
    fn new(world_geometry: WorldGeometry, positions_m: &[Vector]) -> Self {
        Self::new_with_extents(world_geometry, positions_m, &vec![0.0; positions_m.len()])
    }

    fn new_with_extents(
        world_geometry: WorldGeometry,
        positions_m: &[Vector],
        extents_m: &[f64],
    ) -> Self {
        assert_eq!(positions_m.len(), extents_m.len());
        let columns = (world_geometry.width_m() / SPATIAL_CELL_M).floor() as i32;
        let cell_width_m = world_geometry.width_m() / columns as f64;
        let mut cells = HashMap::<_, Vec<_>>::new();
        for (index, (position_m, extent_m)) in positions_m
            .iter()
            .copied()
            .zip(extents_m.iter().copied())
            .enumerate()
        {
            let wrapped_m = Vector::from(world_geometry.wrap_position_m(position_m.into()));
            let center_x_m = wrapped_m.x + world_geometry.width_m() / 2.0;
            for column in wrapped_cell_indices(center_x_m, extent_m, cell_width_m, columns) {
                for row in linear_cell_indices(wrapped_m.y, extent_m, SPATIAL_CELL_M) {
                    cells.entry((column, row)).or_default().push(index);
                }
            }
        }
        Self {
            world_geometry,
            columns,
            cell_width_m,
            cells,
        }
    }

    fn query(&self, center_m: Vector, radius_m: f64) -> Vec<usize> {
        let wrapped_m = Vector::from(self.world_geometry.wrap_position_m(center_m.into()));
        let center_x_m = wrapped_m.x + self.world_geometry.width_m() / 2.0;
        let columns = wrapped_cell_indices(center_x_m, radius_m, self.cell_width_m, self.columns);
        let (minimum_row, maximum_row) = linear_cell_bounds(wrapped_m.y, radius_m, SPATIAL_CELL_M);
        let row_count = i64::from(maximum_row) - i64::from(minimum_row) + 1;
        let mut indices = BTreeSet::new();
        if row_count.saturating_mul(columns.len() as i64) > self.cells.len() as i64 {
            for (&(column, row), cell_indices) in &self.cells {
                if columns.binary_search(&column).is_ok()
                    && (minimum_row..=maximum_row).contains(&row)
                {
                    indices.extend(cell_indices.iter().copied());
                }
            }
        } else {
            for column in columns {
                for row in minimum_row..=maximum_row {
                    if let Some(cell_indices) = self.cells.get(&(column, row)) {
                        indices.extend(cell_indices.iter().copied());
                    }
                }
            }
        }
        indices.into_iter().collect()
    }
}

fn linear_cell_indices(center_m: f64, extent_m: f64, cell_size_m: f64) -> RangeInclusive<i32> {
    let (minimum, maximum) = linear_cell_bounds(center_m, extent_m, cell_size_m);
    minimum..=maximum
}

fn linear_cell_bounds(center_m: f64, extent_m: f64, cell_size_m: f64) -> (i32, i32) {
    (
        ((center_m - extent_m) / cell_size_m).floor() as i32,
        ((center_m + extent_m) / cell_size_m).floor() as i32,
    )
}

fn wrapped_cell_indices(center_m: f64, extent_m: f64, cell_size_m: f64, count: i32) -> Vec<i32> {
    if extent_m * 2.0 >= cell_size_m * count as f64 {
        return (0..count).collect();
    }
    let minimum = ((center_m - extent_m) / cell_size_m).floor() as i32;
    let maximum = ((center_m + extent_m) / cell_size_m).floor() as i32;
    (minimum..=maximum)
        .map(|cell| cell.rem_euclid(count))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

struct CandidateSearch {
    pairs: Vec<(usize, usize)>,
    #[cfg(test)]
    exact_checks: usize,
}

fn cylindrical_candidate_pairs(
    world_geometry: WorldGeometry,
    positions_m: &[Vector],
    search_radii_m: &[f64],
) -> CandidateSearch {
    let index =
        CylindricalSpatialIndex::new_with_extents(world_geometry, positions_m, search_radii_m);
    let mut pairs = Vec::new();
    #[cfg(test)]
    let mut exact_checks = 0;
    for first in 0..positions_m.len() {
        for second in index.query(positions_m[first], search_radii_m[first]) {
            if second <= first {
                continue;
            }
            #[cfg(test)]
            {
                exact_checks += 1;
            }
            let displacement_m =
                Vector::from(index.world_geometry.minimum_image_displacement_m(
                    (positions_m[second] - positions_m[first]).into(),
                ));
            let pair_radius_m = search_radii_m[first] + search_radii_m[second];
            if displacement_m.length_squared() <= pair_radius_m * pair_radius_m {
                pairs.push((first, second));
            }
        }
    }
    CandidateSearch {
        pairs,
        #[cfg(test)]
        exact_checks,
    }
}

fn collision_substep_policy(swept_distance_m: f64, collision_resolution_m: f64) -> usize {
    let required_substeps = (swept_distance_m / collision_resolution_m).ceil().max(1.0) as usize;
    required_substeps.min(MAXIMUM_COLLISION_SUBSTEPS)
}

fn supported_shared_motion(parameters: BoatParameters, mut motion: BoatMotion) -> BoatMotion {
    if !motion.velocity_m_s.is_finite() || !motion.angular_velocity_rad_s.is_finite() {
        motion.velocity_m_s = Vector::ZERO;
        motion.angular_velocity_rad_s = 0.0;
        return motion;
    }
    let hull = parameters.hull;
    let hull_radius_m = (hull.length_m / 2.0).hypot(hull.beam_m / 2.0);
    let point_speed_m_s =
        motion.velocity_m_s.length() + motion.angular_velocity_rad_s.abs() * hull_radius_m;
    if point_speed_m_s > MAXIMUM_SHARED_HULL_POINT_SPEED_M_S {
        let scale = MAXIMUM_SHARED_HULL_POINT_SPEED_M_S / point_speed_m_s;
        motion.velocity_m_s *= scale;
        motion.angular_velocity_rad_s *= scale;
    }
    motion
}

pub struct SharedSimulation {
    boats: Vec<SharedBoat>,
    boat_indices_by_collider: HashMap<ColliderHandle, usize>,
    world_geometry: WorldGeometry,
    spatial_index: CylindricalSpatialIndex,
    pub world: PhysicsWorld,
}

impl Default for SharedSimulation {
    fn default() -> Self {
        Self::new()
    }
}

pub fn apparent_flow(medium_velocity_m_s: Vector, point_velocity_m_s: Vector) -> Vector {
    medium_velocity_m_s - point_velocity_m_s
}

// Portable transcendental calls keep native and WebAssembly replay arithmetic aligned.
pub fn flat_plate_lift_coefficient(angle_of_attack_rad: f64) -> f64 {
    libm::sin(2.0 * angle_of_attack_rad)
}

pub fn flat_plate_drag_coefficient(angle_of_attack_rad: f64, base_drag_coefficient: f64) -> f64 {
    base_drag_coefficient + 1.0 - libm::cos(2.0 * angle_of_attack_rad)
}

pub fn streamlined_lift_coefficient(
    angle_of_attack_rad: f64,
    stall_angle_rad: f64,
    stall_lift_coefficient: f64,
) -> f64 {
    let angle_of_attack_rad = (angle_of_attack_rad + std::f64::consts::FRAC_PI_2)
        .rem_euclid(std::f64::consts::PI)
        - std::f64::consts::FRAC_PI_2;
    let absolute_angle_rad = angle_of_attack_rad.abs();
    if absolute_angle_rad <= stall_angle_rad {
        return stall_lift_coefficient * angle_of_attack_rad / stall_angle_rad;
    }

    let separated_lift = stall_lift_coefficient
        * ((std::f64::consts::FRAC_PI_2 - absolute_angle_rad)
            / (std::f64::consts::FRAC_PI_2 - stall_angle_rad));
    separated_lift
        .max(flat_plate_lift_coefficient(angle_of_attack_rad).abs())
        .copysign(angle_of_attack_rad)
}

pub fn flat_plate_force(
    apparent_flow_m_s: Vector,
    foil_angle_rad: f64,
    fluid_density_kg_m3: f64,
    area_m2: f64,
    base_drag_coefficient: f64,
) -> Vector {
    let normal = rotated(Vector::Y, foil_angle_rad);
    0.5 * fluid_density_kg_m3
        * area_m2
        * apparent_flow_m_s.length()
        * (base_drag_coefficient * apparent_flow_m_s + 2.0 * apparent_flow_m_s.dot(normal) * normal)
}

pub fn streamlined_foil_force(
    apparent_flow_m_s: Vector,
    foil_angle_rad: f64,
    fluid_density_kg_m3: f64,
    area_m2: f64,
    base_drag_coefficient: f64,
    stall_angle_rad: f64,
    stall_lift_coefficient: f64,
) -> Vector {
    let flow_angle_rad = libm::atan2(apparent_flow_m_s.y, apparent_flow_m_s.x);
    let angle_of_attack_rad = foil_angle_rad - std::f64::consts::PI - flow_angle_rad;
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

pub fn rudder_foil_force(
    apparent_flow_m_s: Vector,
    foil_angle_rad: f64,
    fluid_density_kg_m3: f64,
    sample_area_m2: f64,
    total_area_m2: f64,
    chord_m: f64,
    base_drag_coefficient: f64,
) -> Vector {
    // MMG/Fujii normal-pressure law uses the whole rectangular blade's AR.
    // Transom-hung free-tip geometry uses uncorrected AR. Symmetric broadside/
    // astern continuation is a steady proxy; section, ventilation and transient
    // effects require calibration.
    // https://doi.org/10.1007/s00773-014-0293-y, Eq. 38.
    let aspect_ratio = total_area_m2 / chord_m.powi(2);
    let normal_slope = 6.13 * aspect_ratio / (aspect_ratio + 2.25);
    let normal = rotated(Vector::Y, foil_angle_rad);
    0.5 * fluid_density_kg_m3
        * sample_area_m2
        * apparent_flow_m_s.length()
        * (base_drag_coefficient * apparent_flow_m_s
            + normal_slope * apparent_flow_m_s.dot(normal) * normal)
}

pub fn hull_resistance_force(
    apparent_water_m_s: Vector,
    hull_angle_rad: f64,
    fluid_density_kg_m3: f64,
    reference_area_m2: f64,
    longitudinal_coefficient: f64,
    lateral_coefficient: f64,
) -> Vector {
    let local_flow_m_s = rotated(apparent_water_m_s, -hull_angle_rad);
    let pressure_area = 0.5 * fluid_density_kg_m3 * reference_area_m2;
    let local_force_n = Vector::new(
        local_flow_m_s.x * local_flow_m_s.x.abs() * pressure_area * longitudinal_coefficient,
        local_flow_m_s.y * local_flow_m_s.y.abs() * pressure_area * lateral_coefficient,
    );
    rotated(local_force_n, hull_angle_rad)
}

pub fn grounding_friction_force(
    point_velocity_m_s: Vector,
    water_depth_m: f64,
    draft_m: f64,
    supported_mass_kg: f64,
    kinetic_friction_coefficient: f64,
) -> Vector {
    grounding_friction_force_at_step(
        point_velocity_m_s,
        water_depth_m,
        draft_m,
        supported_mass_kg,
        kinetic_friction_coefficient,
        DT_SECONDS,
    )
}

fn grounding_friction_force_at_step(
    point_velocity_m_s: Vector,
    water_depth_m: f64,
    draft_m: f64,
    supported_mass_kg: f64,
    kinetic_friction_coefficient: f64,
    step_seconds: f64,
) -> Vector {
    let support_fraction = ((draft_m - water_depth_m) / draft_m).clamp(0.0, 1.0);
    let maximum_force_n = kinetic_friction_coefficient
        * supported_mass_kg
        * GRAVITY_ACCELERATION_M_S2
        * support_fraction;
    let stopping_force_n = -point_velocity_m_s * supported_mass_kg / step_seconds;
    if stopping_force_n.length() <= maximum_force_n {
        return stopping_force_n;
    }
    if point_velocity_m_s == Vector::ZERO {
        Vector::ZERO
    } else {
        -point_velocity_m_s.normalize() * maximum_force_n
    }
}

fn total_mass_kg(parameters: BoatParameters) -> f64 {
    parameters.hull.mass_kg
        + parameters.sail_mass_kg
        + parameters.boom_mass_kg
        + parameters.rudder_mass_kg
        + parameters.tiller_mass_kg
}

fn apply_grounding_friction(
    world: &mut PhysicsWorld,
    parameters: BoatParameters,
    boat: Boat,
    water_depth_m: f64,
    step_seconds: f64,
) {
    let support_fraction =
        ((parameters.draft_m - water_depth_m) / parameters.draft_m).clamp(0.0, 1.0);
    if support_fraction == 0.0 {
        world.bodies[boat.hull.body].set_body_type(RigidBodyType::Dynamic, true);
        return;
    }

    let hull = &world.bodies[boat.hull.body];
    let hull_center_m = hull.center_of_mass();
    let hull_angle_rad = hull.rotation().angle();
    let half_length_m = parameters.hull.length_m / 2.0;
    let forward = rotated(Vector::new(1.0, 0.0), hull_angle_rad);
    let bow_position_m = hull_center_m + forward * half_length_m;
    let stern_position_m = hull_center_m - forward * half_length_m;
    let bow_velocity_m_s = hull.velocity_at_point(bow_position_m);
    let stern_velocity_m_s = hull.velocity_at_point(stern_position_m);
    let total_mass_kg = total_mass_kg(parameters);

    if bow_velocity_m_s.length() <= STATIC_GROUND_SPEED_THRESHOLD_M_S
        && stern_velocity_m_s.length() <= STATIC_GROUND_SPEED_THRESHOLD_M_S
    {
        let (external_force_n, external_torque_n_m, inertia_kg_m2) =
            [boat.hull.body, boat.sail, boat.rudder].iter().fold(
                (Vector::ZERO, 0.0, 0.0),
                |(force_n, torque_n_m, inertia_kg_m2), body_handle| {
                    let body = &world.bodies[*body_handle];
                    let body_force_n = body.user_force();
                    let offset_m = body.center_of_mass() - hull_center_m;
                    (
                        force_n + body_force_n,
                        torque_n_m + body.user_torque() + offset_m.perp_dot(body_force_n),
                        inertia_kg_m2
                            + body.mass_properties().local_mprops.principal_inertia()
                            + body.mass() * offset_m.length_squared(),
                    )
                },
            );
        let required_force_n = -external_force_n - hull.linvel() * total_mass_kg / step_seconds;
        let required_torque_n_m =
            -external_torque_n_m - hull.angvel() * inertia_kg_m2 / step_seconds;
        let static_force_limit_n = parameters.ground_static_friction_coefficient
            * total_mass_kg
            * GRAVITY_ACCELERATION_M_S2
            * support_fraction;
        let static_torque_limit_n_m = static_force_limit_n * half_length_m;
        // One elliptical budget couples translation and yaw; testing independent
        // limits would allow an unrealistically large combined grounding wrench.
        let static_friction_usage = (required_force_n.length() / static_force_limit_n).powi(2)
            + (required_torque_n_m / static_torque_limit_n_m).powi(2);
        if static_friction_usage <= 1.0 {
            world.bodies[boat.hull.body].set_linvel(Vector::ZERO, true);
            world.bodies[boat.hull.body].set_angvel(0.0, true);
            world.bodies[boat.hull.body].set_body_type(RigidBodyType::Fixed, true);
            return;
        }
    }

    world.bodies[boat.hull.body].set_body_type(RigidBodyType::Dynamic, true);
    for (position_m, velocity_m_s) in [
        (bow_position_m, bow_velocity_m_s),
        (stern_position_m, stern_velocity_m_s),
    ] {
        let friction_force_n = grounding_friction_force_at_step(
            velocity_m_s,
            water_depth_m,
            parameters.draft_m,
            total_mass_kg / 2.0,
            parameters.ground_kinetic_friction_coefficient,
            step_seconds,
        );
        world.bodies[boat.hull.body].add_force_at_point(friction_force_n, position_m, true);
    }
}

fn wrapped_angle(angle_rad: f64) -> f64 {
    (angle_rad + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI) - std::f64::consts::PI
}

fn free_sail_angle(hull_angle_rad: f64, apparent_wind_m_s: Vector, max_sail_angle_rad: f64) -> f64 {
    if apparent_wind_m_s == Vector::ZERO {
        return 0.0;
    }
    wrapped_angle(
        libm::atan2(apparent_wind_m_s.y, apparent_wind_m_s.x)
            - hull_angle_rad
            - std::f64::consts::PI,
    )
    .clamp(-max_sail_angle_rad, max_sail_angle_rad)
}

fn sample_surface_effects(
    max_angle_rad: f64,
    angle_resolution_deg: u32,
    effect_at: impl Fn(f64) -> SurfaceEffect,
) -> Vec<SurfaceEffect> {
    let max_angle_deg = max_angle_rad.to_degrees().round() as i32;
    (-max_angle_deg..=max_angle_deg)
        .step_by(angle_resolution_deg as usize)
        .map(|angle_deg| effect_at((angle_deg as f64).to_radians()))
        .collect()
}

pub fn rudder_foil_samples(parameters: BoatParameters) -> [(Vector, f64); 2] {
    // Two-point Gauss integrates the cubic resisting moment of a rotating plate.
    let offset = 1.0 / (2.0 * 3.0_f64.sqrt());
    [0.5 - offset, 0.5 + offset].map(|fraction| {
        (
            Vector::new(-parameters.rudder_draw_length_m * fraction, 0.0),
            parameters.rudder_area_m2 / 2.0,
        )
    })
}

pub fn sail_foil_samples(parameters: BoatParameters) -> [(Vector, f64); 2] {
    // The triangular span decreases linearly from mast to clew. Gauss-Jacobi
    // weights integrate its area moments through degree three, including drag torque.
    let root = 6.0_f64.sqrt();
    [
        ((4.0 - root) / 10.0, (9.0 + root) / 18.0),
        ((4.0 + root) / 10.0, (9.0 - root) / 18.0),
    ]
    .map(|(fraction, weight)| {
        (
            Vector::new(-parameters.boom_length_m * fraction, 0.0),
            parameters.sail_area_m2 * weight,
        )
    })
}

pub fn keel_foil_samples(parameters: BoatParameters) -> [(Vector, f64); 4] {
    // Draft approximates the rectangular board's effective span, giving chord A/span.
    let half_chord = parameters.keel_area_m2 / parameters.draft_m / 2.0;
    let offset = 1.0 / (2.0 * 3.0_f64.sqrt());
    // Split at the centre so pure yaw's |x|³ moment is integrated on both sides.
    [-0.5 - offset, -0.5 + offset, 0.5 - offset, 0.5 + offset].map(|fraction| {
        (
            parameters.keel_position_m + Vector::new(half_chord * fraction, 0.0),
            parameters.keel_area_m2 / 4.0,
        )
    })
}

pub fn surface_control_input(
    parameters: BoatParameters,
    motion: BoatMotion,
    environment: Environment,
    angle_resolution_deg: u32,
    sail_side_hysteresis_angle_rad: f64,
) -> ControlInput {
    surface_control_input_about_point(
        parameters,
        motion,
        environment,
        angle_resolution_deg,
        sail_side_hysteresis_angle_rad,
        Vector::ZERO,
    )
}

pub fn surface_control_input_about_point(
    parameters: BoatParameters,
    motion: BoatMotion,
    environment: Environment,
    angle_resolution_deg: u32,
    sail_side_hysteresis_angle_rad: f64,
    reference_from_hull_m: Vector,
) -> ControlInput {
    let reference_world_m = rotated(reference_from_hull_m, motion.angle_rad);
    let apparent_wind_m_s = apparent_flow(environment.true_wind_velocity_m_s, motion.velocity_m_s);
    let apparent_water_m_s =
        apparent_flow(environment.true_water_velocity_m_s, motion.velocity_m_s);
    let boat_forward = rotated(Vector::new(1.0, 0.0), motion.angle_rad);
    // Candidate trim angles describe steady relative positions. Applying the
    // current sweep velocity to every candidate makes targets chase their own drag.
    let sail_effects = sample_surface_effects(
        parameters.max_sail_angle_rad,
        angle_resolution_deg,
        |angle_rad| {
            let (force_n, yaw_moment_n_m) = sail_foil_samples(parameters).into_iter().fold(
                (Vector::ZERO, 0.0),
                |(force, torque), (offset, area)| {
                    let surface_offset = rotated(offset, motion.angle_rad + angle_rad);
                    let point =
                        rotated(parameters.mast_position_m, motion.angle_rad) + surface_offset;
                    let flow =
                        apparent_wind_m_s - perpendicular(point) * motion.angular_velocity_rad_s;
                    let sample_force = flat_plate_force(
                        flow,
                        motion.angle_rad + angle_rad,
                        parameters.air_density_kg_m3,
                        area,
                        parameters.sail_base_drag_coefficient,
                    );
                    (
                        force + sample_force,
                        torque
                            + if reference_from_hull_m == Vector::ZERO {
                                point.perp_dot(sample_force)
                            } else {
                                (point - reference_world_m).perp_dot(sample_force)
                            },
                    )
                },
            );
            SurfaceEffect {
                angle_rad,
                drive_force_n: force_n.dot(boat_forward),
                yaw_moment_n_m,
            }
        },
    );
    let rudder_effects = sample_surface_effects(
        parameters.max_rudder_angle_rad,
        angle_resolution_deg,
        |angle_rad| {
            let (force_n, yaw_moment_n_m) = rudder_foil_samples(parameters).into_iter().fold(
                (Vector::ZERO, 0.0),
                |(force, torque), (offset, area)| {
                    let surface_offset = rotated(offset, motion.angle_rad + angle_rad);
                    let point =
                        rotated(parameters.rudder_position_m, motion.angle_rad) + surface_offset;
                    let flow =
                        apparent_water_m_s - perpendicular(point) * motion.angular_velocity_rad_s;
                    let sample_force = rudder_foil_force(
                        flow,
                        motion.angle_rad + angle_rad,
                        parameters.water_density_kg_m3,
                        area,
                        parameters.rudder_area_m2,
                        parameters.rudder_draw_length_m,
                        parameters.rudder_base_drag_coefficient,
                    );
                    (
                        force + sample_force,
                        torque
                            + if reference_from_hull_m == Vector::ZERO {
                                point.perp_dot(sample_force)
                            } else {
                                (point - reference_world_m).perp_dot(sample_force)
                            },
                    )
                },
            );
            SurfaceEffect {
                angle_rad,
                drive_force_n: force_n.dot(boat_forward),
                yaw_moment_n_m,
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
            libm::atan2(apparent_wind_m_s.y, apparent_wind_m_s.x) - motion.angle_rad,
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
) -> f64 {
    let rudder_position_world_m = rotated(parameters.rudder_position_m, motion.angle_rad);
    let rudder_apparent_water_m_s = apparent_flow(
        environment.true_water_velocity_m_s,
        motion.velocity_m_s
            + perpendicular(rudder_position_world_m) * motion.angular_velocity_rad_s,
    );
    let steering_baseline_m = (parameters.rudder_position_m - parameters.keel_position_m).length();
    rudder_apparent_water_m_s.length() * parameters.max_rudder_angle_rad.tan() / steering_baseline_m
}

fn controlled_free_sail_angle(input: &ControlInput) -> f64 {
    if input.apparent_wind_angle_from_boat_forward_rad.abs() < std::f64::consts::FRAC_PI_2
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

fn trimmed_sail_effect(input: &ControlInput, free_sail_angle_rad: f64) -> SurfaceEffect {
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

fn nearest_sail_effect(input: &ControlInput, target_angle_rad: f64) -> SurfaceEffect {
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
    power: f64,
    allow_sternway: bool,
    gradual_sternway: bool,
) -> (f64, SurfaceEffect, Option<f64>) {
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

pub fn continuous_yaw_rate_move_control(
    input: &ControlInput,
    intent: MotionIntent,
    maximum_yaw_rate_rad_s: f64,
) -> SurfaceTargets {
    yaw_rate_move_control_with_selection(input, intent, maximum_yaw_rate_rad_s, true)
}

pub fn yaw_rate_move_control(
    input: &ControlInput,
    intent: MotionIntent,
    maximum_yaw_rate_rad_s: f64,
) -> SurfaceTargets {
    yaw_rate_move_control_with_selection(input, intent, maximum_yaw_rate_rad_s, false)
}

fn yaw_rate_move_control_with_selection(
    input: &ControlInput,
    intent: MotionIntent,
    maximum_yaw_rate_rad_s: f64,
    continuous: bool,
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
        .fold(f64::INFINITY, f64::min);
    let maximum_moment_n_m = input
        .rudder_effects
        .iter()
        .map(|rudder| sail_effect.yaw_moment_n_m + rudder.yaw_moment_n_m)
        .fold(f64::NEG_INFINITY, f64::max);
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
    let mut rudder_target_angle_rad = rudder_effect.angle_rad;
    if continuous {
        let requested_rudder_moment = target_moment_n_m - sail_effect.yaw_moment_n_m;
        let mut selected_error = (rudder_effect.yaw_moment_n_m - requested_rudder_moment).abs();
        for segment in input.rudder_effects.windows(2) {
            let [left, right] = [segment[0], segment[1]];
            let delta = right.yaw_moment_n_m - left.yaw_moment_n_m;
            let angle = if delta == 0.0 {
                0.0_f64.clamp(left.angle_rad, right.angle_rad)
            } else {
                let fraction = (requested_rudder_moment - left.yaw_moment_n_m) / delta;
                if !(0.0..=1.0).contains(&fraction) {
                    continue;
                }
                left.angle_rad + fraction * (right.angle_rad - left.angle_rad)
            };
            let error = if delta == 0.0 {
                (left.yaw_moment_n_m - requested_rudder_moment).abs()
            } else {
                0.0
            };
            if error + CONTROL_SELECTION_TIE_TOLERANCE < selected_error
                || ((error - selected_error).abs() <= CONTROL_SELECTION_TIE_TOLERANCE
                    && angle.abs() < rudder_target_angle_rad.abs())
            {
                selected_error = error;
                rudder_target_angle_rad = angle;
            }
        }
    }
    SurfaceTargets {
        sheet_target_angle_rad,
        rudder_target_angle_rad,
        sail_target_angle_rad,
    }
}

pub fn hull_outline_m(parameters: HullParameters) -> [Vector; 15] {
    let half_length_m = parameters.length_m / 2.0;
    let half_beam_m = parameters.beam_m / 2.0;
    let bow_base_m = half_length_m - parameters.bow_length_m;
    [
        Vector::new(half_length_m, 0.0),
        Vector::new(
            half_length_m - 0.35 * parameters.bow_length_m,
            -0.20 * half_beam_m,
        ),
        Vector::new(bow_base_m, -0.52 * half_beam_m),
        Vector::new(0.435 * bow_base_m, -0.87 * half_beam_m),
        Vector::new(-0.107 * half_length_m, -half_beam_m),
        Vector::new(-0.504 * half_length_m, -0.91 * half_beam_m),
        Vector::new(-0.826 * half_length_m, -0.68 * half_beam_m),
        Vector::new(-half_length_m, -0.52 * half_beam_m),
        Vector::new(-half_length_m, 0.52 * half_beam_m),
        Vector::new(-0.826 * half_length_m, 0.68 * half_beam_m),
        Vector::new(-0.504 * half_length_m, 0.91 * half_beam_m),
        Vector::new(-0.107 * half_length_m, half_beam_m),
        Vector::new(0.435 * bow_base_m, 0.87 * half_beam_m),
        Vector::new(bow_base_m, 0.52 * half_beam_m),
        Vector::new(
            half_length_m - 0.35 * parameters.bow_length_m,
            0.20 * half_beam_m,
        ),
    ]
}

fn polygon_moment_of_inertia(mass_kg: f64, points: &[Vector]) -> f64 {
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

fn foil_spin_inertia_kg_m2(chord_m: f64, area_m2: f64, density_kg_m3: f64) -> f64 {
    if area_m2 == 0.0 || density_kg_m3 == 0.0 {
        return 0.0;
    }
    // Thin-ellipsoid potential flow, with an equal-area ellipse of this chord:
    // https://mujoco.readthedocs.io/en/stable/computation/fluid.html#added-mass
    // Compromise: retain only centroid-spin energy ½Jω², omitting normal added
    // mass and translation/rotation coupling. Elliptical edges approximate the
    // rectangular rudder/keel and triangular sail; this is not measured boat data.
    let a = chord_m / 2.0;
    let c = area_m2 / (std::f64::consts::PI * a);
    let h = std::f64::consts::FRAC_PI_2 / 128.0;
    let mut kx = 0.0;
    let mut kz = 0.0;
    for i in 0..=128 {
        let angle = h * f64::from(i);
        let q = libm::cos(angle);
        let s = libm::sin(angle);
        let d = c * c * q * q + a * a * s * s;
        let weight = if i == 0 || i == 128 {
            1.0
        } else if i % 2 == 0 {
            2.0
        } else {
            4.0
        };
        kx += weight * 2.0 * c / a * q * q / d.sqrt();
        kz += weight * 2.0 * c * a * q * q / (d * d.sqrt());
    }
    kx *= h / 3.0;
    kz *= h / 3.0;
    8.0 * std::f64::consts::PI * density_kg_m3 * a.powi(3) * c / (15.0 * (2.0 * kx + kz))
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
    let collider = ColliderBuilder::convex_hull(&outline)
        .unwrap()
        .density(0.0)
        .build();
    let collider = colliders.insert_with_parent(collider, body, bodies);
    bodies[body].recompute_mass_properties_from_colliders(colliders);
    Hull { body, collider }
}

fn sail_center_from_mast_m(parameters: BoatParameters) -> Vector {
    Vector::new(
        -parameters.boom_length_m * (parameters.sail_mass_kg / 3.0 + parameters.boom_mass_kg / 2.0)
            / (parameters.sail_mass_kg + parameters.boom_mass_kg),
        0.0,
    )
}

fn sail_hinge_moment_of_inertia_kg_m2(parameters: BoatParameters) -> f64 {
    (parameters.sail_mass_kg / 6.0 + parameters.boom_mass_kg / 3.0)
        * parameters.boom_length_m.powi(2)
}

fn sail_mass_properties(parameters: BoatParameters) -> MassProperties {
    let mass_kg = parameters.sail_mass_kg + parameters.boom_mass_kg;
    let hinge_inertia = sail_hinge_moment_of_inertia_kg_m2(parameters);
    let center = sail_center_from_mast_m(parameters);
    MassProperties::new(
        Vector::ZERO,
        mass_kg,
        hinge_inertia - mass_kg * center.length_squared()
            + foil_spin_inertia_kg_m2(
                parameters.boom_length_m,
                parameters.sail_area_m2,
                parameters.air_density_kg_m3,
            ),
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

fn rudder_hinge_moment_of_inertia_kg_m2(parameters: BoatParameters) -> f64 {
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
        hinge_inertia - mass_kg * center.length_squared()
            + foil_spin_inertia_kg_m2(
                parameters.rudder_draw_length_m,
                parameters.rudder_area_m2,
                parameters.water_density_kg_m3,
            ),
    )
}

fn damped_control_torque(
    angle_rad: f64,
    angular_velocity_rad_s: f64,
    moment_of_inertia_kg_m2: f64,
    maximum_torque_n_m: f64,
    maximum_angle_rad: f64,
    target_angle_rad: f64,
) -> f64 {
    let stiffness_n_m_rad = maximum_torque_n_m / (maximum_angle_rad / 10.0);
    let critical_damping_n_m_s_rad = 2.0 * (stiffness_n_m_rad * moment_of_inertia_kg_m2).sqrt();
    // The torque is held for a full tick. Solve feedback at its end using
    // ω₁ = ω₀ + τΔt/I and θ₁ = θ₀ + ω₁Δt; explicit critical damping
    // can inject energy when the hinge response is faster than the tick.
    let numerator = stiffness_n_m_rad
        * (target_angle_rad - angle_rad - DT_SECONDS * angular_velocity_rad_s)
        - critical_damping_n_m_s_rad * angular_velocity_rad_s;
    let denominator = 1.0
        + (critical_damping_n_m_s_rad * DT_SECONDS + stiffness_n_m_rad * DT_SECONDS.powi(2))
            / moment_of_inertia_kg_m2;
    (numerator / denominator).clamp(-maximum_torque_n_m, maximum_torque_n_m)
}

fn sail_target_control_torque(
    parameters: BoatParameters,
    motion: BoatMotion,
    targets: SurfaceTargets,
) -> f64 {
    let target = targets
        .sail_target_angle_rad
        .unwrap_or(targets.sheet_target_angle_rad);
    let torque = damped_control_torque(
        motion.sail_angle_rad,
        motion.sail_angular_velocity_rad_s,
        sail_hinge_moment_of_inertia_kg_m2(parameters),
        parameters.max_sail_actuator_torque_n_m,
        parameters.max_sail_angle_rad,
        target,
    );
    let outward_direction = if motion.sail_angle_rad == 0.0 {
        target
    } else {
        motion.sail_angle_rad
    };
    if targets.sail_target_angle_rad.is_none() && torque * outward_direction > 0.0 {
        0.0
    } else {
        torque
    }
}

fn set_rudder_target(
    world: &mut PhysicsWorld,
    boat: Boat,
    parameters: BoatParameters,
    target: Option<f64>,
) {
    let actuator = &mut world
        .impulse_joints
        .get_mut(boat.rudder_actuator, true)
        .unwrap()
        .data;
    let maximum_torque = parameters.max_rudder_actuator_torque_n_m();
    let stiffness = maximum_torque / (parameters.max_rudder_angle_rad / 10.0);
    let damping = 2.0 * (stiffness * rudder_hinge_moment_of_inertia_kg_m2(parameters)).sqrt();
    actuator.set_motor_position(JointAxis::AngX, target.unwrap_or(0.0), stiffness, damping);
    actuator.set_motor_max_force(
        JointAxis::AngX,
        if target.is_some() {
            maximum_torque
        } else {
            0.0
        },
    );
}

pub fn create_boat(
    parameters: BoatParameters,
    motion: BoatMotion,
    bodies: &mut RigidBodySet,
    colliders: &mut ColliderSet,
    joints: &mut ImpulseJointSet,
) -> Boat {
    let hull = create_hull(parameters.hull, bodies, colliders);
    let hull_body = &mut bodies[hull.body];
    hull_body.set_additional_mass_properties(
        MassProperties::new(
            Vector::ZERO,
            parameters.hull.mass_kg,
            hull_body.mass_properties().local_mprops.principal_inertia()
                + foil_spin_inertia_kg_m2(
                    parameters.keel_area_m2 / parameters.draft_m,
                    parameters.keel_area_m2,
                    parameters.water_density_kg_m3,
                ),
        ),
        true,
    );
    hull_body.recompute_mass_properties_from_colliders(colliders);
    let sail = bodies.insert(
        RigidBodyBuilder::dynamic().additional_mass_properties(sail_mass_properties(parameters)),
    );
    let rudder = bodies.insert(
        RigidBodyBuilder::dynamic().additional_mass_properties(rudder_mass_properties(parameters)),
    );
    bodies[sail].recompute_mass_properties_from_colliders(colliders);
    bodies[rudder].recompute_mass_properties_from_colliders(colliders);
    // Force-based relative damping dissipates hinge motion without resisting
    // common rotation or changing assembly angular momentum.
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
    for (joint, damping) in [
        (sail_joint, parameters.sail_bearing_damping_n_m_s_rad),
        (rudder_joint, parameters.rudder_bearing_damping_n_m_s_rad),
    ] {
        if damping == 0.0 {
            joints.get_mut(joint, true).unwrap().data.motor_axes = JointAxesMask::empty();
        }
    }
    // Keep uncapped passive bearing friction independent of bounded sailor effort;
    // this joint supplies only angular actuation, without duplicate hinge constraints.
    let mut actuator = GenericJoint::new(JointAxesMask::empty());
    actuator.set_motor_model(JointAxis::AngX, MotorModel::ForceBased);
    actuator.set_motor_max_force(JointAxis::AngX, 0.0);
    let rudder_actuator = joints.insert(hull.body, rudder, actuator, true);
    let boat = Boat {
        hull,
        sail,
        rudder,
        sail_joint,
        rudder_joint,
        rudder_actuator,
    };
    restore_motion(&boat, parameters, motion, bodies);
    boat
}

fn rotated(vector: Vector, angle_rad: f64) -> Vector {
    Rotation::new(angle_rad).transform_vector(vector)
}

fn perpendicular(vector: Vector) -> Vector {
    Vector::new(-vector.y, vector.x)
}

fn restore_surface_motion(
    body: RigidBodyHandle,
    hinge_position_m: Vector,
    center_from_hinge_m: Vector,
    relative_angle_rad: f64,
    relative_angular_velocity_rad_s: f64,
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

fn relative_angle(angle_rad: f64, hull_angle_rad: f64) -> f64 {
    (angle_rad - hull_angle_rad + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI)
        - std::f64::consts::PI
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

fn physics_world() -> PhysicsWorld {
    PhysicsWorld {
        gravity: Vector::ZERO,
        integration_parameters: IntegrationParameters {
            dt: DT_SECONDS,
            num_solver_iterations: 8,
            warmstart_joints: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn foil_force_slope_bound(base_drag: f64, lift: f64, lift_slope: f64) -> f64 {
    // For F = ½ρAv² Rθ(D,-L), radial and angular Jacobian columns are
    // bounded by 2a and a+b. Their Frobenius norm bounds the force derivative.
    let a = (base_drag + 2.0).hypot(lift);
    let b = 2.0_f64.hypot(lift_slope);
    (2.0 * a).hypot(a + b)
}

fn streamlined_force_slope_bound(base_drag: f64, stall_angle: f64, stall_lift: f64) -> f64 {
    let slope = (stall_lift / stall_angle)
        .max(stall_lift / (std::f64::consts::FRAC_PI_2 - stall_angle))
        .max(2.0);
    foil_force_slope_bound(base_drag, stall_lift.max(1.0), slope)
}

fn fluid_response_rate(
    body: &RigidBody,
    samples: impl IntoIterator<Item = (Vector, f64)>,
    fluid_velocity: Vector,
    density: f64,
    coefficient_slope: f64,
) -> f64 {
    let inertia = body.mass_properties().local_mprops.principal_inertia();
    samples
        .into_iter()
        .map(|(offset, area)| {
            let point = body.translation() + rotated(offset, body.rotation().angle());
            let speed = apparent_flow(fluid_velocity, body.velocity_at_point(point)).length();
            let force_slope = 0.5 * density * area * speed * coefficient_slope;
            // Free-body point mobility bounds its constrained translation and rotation.
            force_slope * (body.mass().recip() + offset.length_squared() / inertia)
        })
        .sum()
}

fn fluid_maximum_step_seconds(
    world: &PhysicsWorld,
    parameters: BoatParameters,
    boat: Boat,
    environment: Environment,
) -> f64 {
    let sail_rate = fluid_response_rate(
        &world.bodies[boat.sail],
        sail_foil_samples(parameters)
            .map(|(point, area)| (point - sail_center_from_mast_m(parameters), area)),
        environment.true_wind_velocity_m_s,
        parameters.air_density_kg_m3,
        foil_force_slope_bound(parameters.sail_base_drag_coefficient, 1.0, 2.0),
    );
    let rudder_aspect_ratio = parameters.rudder_area_m2 / parameters.rudder_draw_length_m.powi(2);
    let rudder_normal_slope = 6.13 * rudder_aspect_ratio / (rudder_aspect_ratio + 2.25);
    let rudder_rate = fluid_response_rate(
        &world.bodies[boat.rudder],
        rudder_foil_samples(parameters)
            .map(|(point, area)| (point - rudder_center_from_hinge_m(parameters), area)),
        environment.true_water_velocity_m_s,
        parameters.water_density_kg_m3,
        // F = ½ρA|u|Bu, ||B|| = Cd0 + k, hence ||dF/du|| ≤ ρA|u|(Cd0 + k).
        2.0 * (parameters.rudder_base_drag_coefficient + rudder_normal_slope),
    );
    let keel_rate = fluid_response_rate(
        &world.bodies[boat.hull.body],
        keel_foil_samples(parameters),
        environment.true_water_velocity_m_s,
        parameters.water_density_kg_m3,
        streamlined_force_slope_bound(
            parameters.keel_base_drag_coefficient,
            parameters.keel_stall_angle_rad,
            parameters.keel_stall_lift_coefficient,
        ),
    );
    let hull_rate = fluid_response_rate(
        &world.bodies[boat.hull.body],
        [-0.5, 0.5].map(|fraction| {
            (
                Vector::new(parameters.hull.length_m * fraction, 0.0),
                parameters.hull_reference_area_m2 / 2.0,
            )
        }),
        environment.true_water_velocity_m_s,
        parameters.water_density_kg_m3,
        2.0 * parameters
            .hull_drag_coefficient
            .max(parameters.hull_lateral_drag_coefficient),
    );
    // Re-evaluate every substep as fluid velocities and articulated motion change.
    DT_SECONDS.min(
        sail_rate
            .max(rudder_rate)
            .max(keel_rate + hull_rate)
            .recip(),
    )
}

fn apply_boat_forces(
    world: &mut PhysicsWorld,
    parameters: BoatParameters,
    boat: Boat,
    environment: Environment,
    actuator_torques: ActuatorTorques,
) {
    for body in [boat.hull.body, boat.sail, boat.rudder] {
        world.bodies[body].reset_forces(false);
        world.bodies[body].reset_torques(false);
    }
    let sail_center = sail_center_from_mast_m(parameters);
    for (offset, area) in sail_foil_samples(parameters) {
        let sail = &world.bodies[boat.sail];
        let angle = sail.rotation().angle();
        let point = sail.translation() + rotated(offset - sail_center, angle);
        let force = flat_plate_force(
            apparent_flow(
                environment.true_wind_velocity_m_s,
                sail.velocity_at_point(point),
            ),
            angle,
            parameters.air_density_kg_m3,
            area,
            parameters.sail_base_drag_coefficient,
        );
        world.bodies[boat.sail].add_force_at_point(force, point, true);
    }
    let rudder_center = rudder_center_from_hinge_m(parameters);
    for (offset, area) in rudder_foil_samples(parameters) {
        let rudder = &world.bodies[boat.rudder];
        let angle = rudder.rotation().angle();
        let point = rudder.translation() + rotated(offset - rudder_center, angle);
        let force = rudder_foil_force(
            apparent_flow(
                environment.true_water_velocity_m_s,
                rudder.velocity_at_point(point),
            ),
            angle,
            parameters.water_density_kg_m3,
            area,
            parameters.rudder_area_m2,
            parameters.rudder_draw_length_m,
            parameters.rudder_base_drag_coefficient,
        );
        world.bodies[boat.rudder].add_force_at_point(force, point, true);
    }
    let hull_angle_rad = world.bodies[boat.hull.body].rotation().angle();
    for (offset, area) in keel_foil_samples(parameters) {
        let hull = &world.bodies[boat.hull.body];
        let point = hull.translation() + rotated(offset, hull_angle_rad);
        let force = streamlined_foil_force(
            apparent_flow(
                environment.true_water_velocity_m_s,
                hull.velocity_at_point(point),
            ),
            hull_angle_rad,
            parameters.water_density_kg_m3,
            area,
            parameters.keel_base_drag_coefficient,
            parameters.keel_stall_angle_rad,
            parameters.keel_stall_lift_coefficient,
        );
        world.bodies[boat.hull.body].add_force_at_point(force, point, true);
    }

    for local_position_m in [
        Vector::new(-parameters.hull.length_m / 2.0, 0.0),
        Vector::new(parameters.hull.length_m / 2.0, 0.0),
    ] {
        let hull = &world.bodies[boat.hull.body];
        let world_position_m = hull.translation() + rotated(local_position_m, hull_angle_rad);
        let apparent_water_m_s = apparent_flow(
            environment.true_water_velocity_m_s,
            hull.velocity_at_point(world_position_m),
        );
        let force_n = hull_resistance_force(
            apparent_water_m_s,
            hull_angle_rad,
            parameters.water_density_kg_m3,
            parameters.hull_reference_area_m2 / 2.0,
            parameters.hull_drag_coefficient,
            parameters.hull_lateral_drag_coefficient,
        );
        world.bodies[boat.hull.body].add_force_at_point(force_n, world_position_m, true);
    }

    world.bodies[boat.sail].add_torque(actuator_torques.sail_n_m, true);
    world.bodies[boat.rudder].add_torque(actuator_torques.rudder_n_m, true);
    world.bodies[boat.hull.body].add_torque(
        -actuator_torques.sail_n_m - actuator_torques.rudder_n_m,
        true,
    );
    apply_grounding_friction(
        world,
        parameters,
        boat,
        environment.water_depth_m,
        world.integration_parameters.dt,
    );
}

impl Simulation {
    pub fn new(parameters: BoatParameters, motion: BoatMotion) -> Self {
        let mut world = physics_world();
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
        set_rudder_target(&mut self.world, self.boat, self.parameters, None);
        self.advance_actuators(environment, actuator_torques);
    }

    pub fn advance_targets(&mut self, environment: Environment, targets: SurfaceTargets) {
        let sail_n_m = sail_target_control_torque(self.parameters, self.motion(), targets);
        set_rudder_target(
            &mut self.world,
            self.boat,
            self.parameters,
            Some(targets.rudder_target_angle_rad),
        );
        self.advance_actuators(
            environment,
            ActuatorTorques {
                sail_n_m,
                rudder_n_m: 0.0,
            },
        );
    }

    fn advance_actuators(&mut self, environment: Environment, actuator_torques: ActuatorTorques) {
        let mut remaining_seconds = DT_SECONDS;
        while remaining_seconds > 0.0 {
            let step_seconds = remaining_seconds.min(fluid_maximum_step_seconds(
                &self.world,
                self.parameters,
                self.boat,
                environment,
            ));
            self.world.integration_parameters.dt = step_seconds;
            apply_boat_forces(
                &mut self.world,
                self.parameters,
                self.boat,
                environment,
                actuator_torques,
            );
            self.world.step();
            remaining_seconds = (remaining_seconds - step_seconds).max(0.0);
        }
        self.world.integration_parameters.dt = DT_SECONDS;
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
        self.advance_targets(environment, targets);
        ControlTelemetry {
            targets,
            maximum_yaw_rate_rad_s,
        }
    }

    pub fn motion(&self) -> BoatMotion {
        read_motion(&self.boat, &self.world.bodies)
    }
}

impl SharedSimulation {
    pub fn new() -> Self {
        Self::new_with_world_geometry(WorldGeometry::default())
    }

    pub fn new_with_world_geometry(world_geometry: WorldGeometry) -> Self {
        Self {
            boats: Vec::new(),
            boat_indices_by_collider: HashMap::new(),
            world_geometry,
            spatial_index: CylindricalSpatialIndex::new(world_geometry, &[]),
            world: physics_world(),
        }
    }

    pub fn add_boat(&mut self, parameters: BoatParameters, mut motion: BoatMotion) -> usize {
        assert!(
            parameters.hull.beam_m.is_finite()
                && parameters.hull.beam_m >= MINIMUM_SHARED_HULL_BEAM_M,
            "shared simulation hull beam must be at least {MINIMUM_SHARED_HULL_BEAM_M} metres"
        );
        motion.position_m = Vector::from(
            self.world_geometry
                .wrap_position_m(motion.position_m.into()),
        );
        motion = supported_shared_motion(parameters, motion);
        let boat = create_boat(
            parameters,
            motion,
            &mut self.world.bodies,
            &mut self.world.colliders,
            &mut self.world.impulse_joints,
        );
        let index = self.boats.len();
        self.boat_indices_by_collider
            .insert(boat.hull.collider, index);
        self.boats.push(SharedBoat { parameters, boat });
        self.refresh_spatial_index();
        index
    }

    pub fn remove_boat(&mut self, boat_index: usize) {
        let boat = self.boats.remove(boat_index).boat;
        self.boat_indices_by_collider.remove(&boat.hull.collider);
        for (index, shifted_boat) in self.boats.iter().enumerate().skip(boat_index) {
            self.boat_indices_by_collider
                .insert(shifted_boat.boat.hull.collider, index);
        }
        for body in [boat.sail, boat.rudder, boat.hull.body] {
            self.world.bodies.remove(
                body,
                &mut self.world.islands,
                &mut self.world.colliders,
                &mut self.world.impulse_joints,
                &mut self.world.multibody_joints,
                &mut self.world.soft_bodies,
                true,
            );
        }
        self.refresh_spatial_index();
    }

    pub fn boats_within_radius(&self, center_m: Vector, radius_m: f64) -> Vec<usize> {
        let radius_squared_m2 = radius_m * radius_m;
        self.spatial_index
            .query(center_m, radius_m)
            .into_iter()
            .filter(|boat| {
                let displacement_m = self.world_geometry.minimum_image_displacement_m(
                    (self.motion(*boat).position_m - center_m).into(),
                );
                Vector::from(displacement_m).length_squared() <= radius_squared_m2
            })
            .collect::<Vec<_>>()
    }

    pub fn advance(
        &mut self,
        environment: Environment,
        actuator_torques: &[ActuatorTorques],
    ) -> Vec<HullContact> {
        self.advance_with_environments(&vec![environment; self.boats.len()], actuator_torques)
    }

    pub fn advance_with_environments(
        &mut self,
        environments: &[Environment],
        actuator_torques: &[ActuatorTorques],
    ) -> Vec<HullContact> {
        assert_eq!(environments.len(), self.boats.len());
        assert_eq!(actuator_torques.len(), self.boats.len());
        for boat in &self.boats {
            set_rudder_target(&mut self.world, boat.boat, boat.parameters, None);
        }
        let substeps = self.prepare_contact_charts();
        self.step_and_collect_contacts(substeps, environments, actuator_torques)
    }

    pub fn advance_controlled_with_environments(
        &mut self,
        environments: &[Environment],
        targets: &[Option<SurfaceTargets>],
    ) -> Vec<HullContact> {
        assert_eq!(environments.len(), self.boats.len());
        assert_eq!(targets.len(), self.boats.len());
        let mut torques = Vec::with_capacity(self.boats.len());
        for (boat, targets) in self.boats.iter().zip(targets) {
            let sail_n_m = targets.map_or(0.0, |targets| {
                sail_target_control_torque(
                    boat.parameters,
                    read_motion(&boat.boat, &self.world.bodies),
                    targets,
                )
            });
            set_rudder_target(
                &mut self.world,
                boat.boat,
                boat.parameters,
                targets.map(|t| t.rudder_target_angle_rad),
            );
            torques.push(ActuatorTorques {
                sail_n_m,
                rudder_n_m: 0.0,
            });
        }
        let substeps = self.prepare_contact_charts();
        self.step_and_collect_contacts(substeps, environments, &torques)
    }

    pub fn motion(&self, boat: usize) -> BoatMotion {
        let mut motion = read_motion(&self.boats[boat].boat, &self.world.bodies);
        motion.position_m = Vector::from(
            self.world_geometry
                .wrap_position_m(motion.position_m.into()),
        );
        motion
    }

    pub fn control_dynamics(
        &self,
        boat_index: usize,
        environment: Environment,
    ) -> BoatControlDynamics {
        let entry = &self.boats[boat_index];
        let parameters = entry.parameters;
        let boat = entry.boat;
        let hull = &self.world.bodies[boat.hull.body];
        let origin = hull.translation();
        let bodies =
            [boat.hull.body, boat.sail, boat.rudder].map(|handle| &self.world.bodies[handle]);
        let mass: f64 = bodies.iter().map(|body| body.mass()).sum();
        let center = bodies.iter().fold(Vector::ZERO, |sum, body| {
            sum + (body.center_of_mass() - origin) * body.mass()
        }) / mass
            + origin;
        let mut state = BoatControlDynamics {
            total_mass_kg: mass,
            center_of_mass_from_hull_m: hull.rotation().inverse().transform_vector(center - origin),
            locked_inertia_kg_m2: bodies
                .iter()
                .map(|body| {
                    body.mass_properties().local_mprops.principal_inertia()
                        + body.mass() * (body.center_of_mass() - center).length_squared()
                })
                .sum(),
            ..BoatControlDynamics::default()
        };
        let mut total_force = Vector::ZERO;
        let forward = rotated(Vector::X, hull.rotation().angle());
        for (index, handle, hinge_local, body_center, samples) in [
            (
                0,
                boat.sail,
                parameters.mast_position_m,
                sail_center_from_mast_m(parameters),
                sail_foil_samples(parameters),
            ),
            (
                1,
                boat.rudder,
                parameters.rudder_position_m,
                rudder_center_from_hinge_m(parameters),
                rudder_foil_samples(parameters),
            ),
        ] {
            let body = &self.world.bodies[handle];
            let angle = body.rotation().angle();
            let hinge = origin + rotated(hinge_local, hull.rotation().angle());
            for (offset, area) in samples {
                let point = body.translation() + rotated(offset - body_center, angle);
                let fluid = if index == 0 {
                    environment.true_wind_velocity_m_s
                } else {
                    environment.true_water_velocity_m_s
                };
                let flow = apparent_flow(fluid, body.velocity_at_point(point));
                let force = if index == 0 {
                    flat_plate_force(
                        flow,
                        angle,
                        parameters.air_density_kg_m3,
                        area,
                        parameters.sail_base_drag_coefficient,
                    )
                } else {
                    rudder_foil_force(
                        flow,
                        angle,
                        parameters.water_density_kg_m3,
                        area,
                        parameters.rudder_area_m2,
                        parameters.rudder_draw_length_m,
                        parameters.rudder_base_drag_coefficient,
                    )
                };
                total_force += force;
                state.foil_forward_force_n += force.dot(forward);
                state.fluid_hinge_torques_n_m[index] += (point - hinge).perp_dot(force);
                state.fluid_yaw_moments_n_m[index] += (point - center).perp_dot(force);
            }
            if index == 0 {
                let arm = body.center_of_mass() - hinge;
                let inertia = body.mass_properties().local_mprops.principal_inertia();
                state.sail_hinge_inertia_kg_m2 = inertia + body.mass() * arm.length_squared();
                state.sail_yaw_coupling_kg_m2 =
                    inertia + body.mass() * (body.center_of_mass() - center).dot(arm);
                state.sail_relative_inertia_kg_m2 =
                    inertia + body.mass() * (1.0 - body.mass() / mass) * arm.length_squared();
                let rudder = &self.world.bodies[boat.rudder];
                let rudder_hinge =
                    origin + rotated(parameters.rudder_position_m, hull.rotation().angle());
                state.sail_rudder_coupling_kg_m2 = -body.mass() * rudder.mass() / mass
                    * arm.dot(rudder.center_of_mass() - rudder_hinge);
            }
            if index == 1 {
                let arm = body.center_of_mass() - hinge;
                let inertia = body.mass_properties().local_mprops.principal_inertia();
                state.rudder_yaw_coupling_kg_m2 =
                    inertia + body.mass() * (body.center_of_mass() - center).dot(arm);
                state.rudder_relative_inertia_kg_m2 =
                    inertia + body.mass() * (1.0 - body.mass() / mass) * arm.length_squared();
                state.rudder_hinge_inertia_kg_m2 =
                    body.mass_properties().local_mprops.principal_inertia()
                        + body.mass() * (body.center_of_mass() - hinge).length_squared();
            }
        }
        let angle = hull.rotation().angle();
        for (offset, area) in keel_foil_samples(parameters) {
            let point = origin + rotated(offset, angle);
            let force = streamlined_foil_force(
                apparent_flow(
                    environment.true_water_velocity_m_s,
                    hull.velocity_at_point(point),
                ),
                angle,
                parameters.water_density_kg_m3,
                area,
                parameters.keel_base_drag_coefficient,
                parameters.keel_stall_angle_rad,
                parameters.keel_stall_lift_coefficient,
            );
            total_force += force;
            state.passive_yaw_moment_n_m += (point - center).perp_dot(force);
        }
        for sign in [-1.0, 1.0] {
            let point = origin
                + rotated(
                    Vector::new(sign * parameters.hull.length_m / 2.0, 0.0),
                    angle,
                );
            let force = hull_resistance_force(
                apparent_flow(
                    environment.true_water_velocity_m_s,
                    hull.velocity_at_point(point),
                ),
                angle,
                parameters.water_density_kg_m3,
                parameters.hull_reference_area_m2 / 2.0,
                parameters.hull_drag_coefficient,
                parameters.hull_lateral_drag_coefficient,
            );
            total_force += force;
            state.passive_yaw_moment_n_m += (point - center).perp_dot(force);
        }
        state.fluid_forward_force_n = total_force.dot(forward);
        for (index, handle, local_hinge) in [
            (0, boat.sail, parameters.mast_position_m),
            (1, boat.rudder, parameters.rudder_position_m),
        ] {
            let body = &self.world.bodies[handle];
            let hinge = origin + rotated(local_hinge, hull.rotation().angle());
            state.fluid_generalized_torques_n_m[index] = state.fluid_hinge_torques_n_m[index]
                - body.mass() / mass * (body.center_of_mass() - hinge).perp_dot(total_force);
        }
        state.rudder_motor_stiffness_n_m_rad =
            parameters.max_rudder_actuator_torque_n_m() / (parameters.max_rudder_angle_rad / 10.0);
        state.rudder_motor_damping_n_m_s_rad = 2.0
            * (state.rudder_motor_stiffness_n_m_rad
                * rudder_hinge_moment_of_inertia_kg_m2(parameters))
            .sqrt();
        state
    }

    fn refresh_spatial_index(&mut self) {
        let positions_m = self
            .boats
            .iter()
            .map(|boat| {
                let position_m = self.world.bodies[boat.boat.hull.body].translation();
                Vector::from(self.world_geometry.wrap_position_m(position_m.into()))
            })
            .collect::<Vec<_>>();
        self.spatial_index = CylindricalSpatialIndex::new(self.world_geometry, &positions_m);
    }

    fn enforce_supported_motion(&mut self) {
        for boat in &self.boats {
            let motion = read_motion(&boat.boat, &self.world.bodies);
            let supported = supported_shared_motion(boat.parameters, motion);
            if supported != motion {
                restore_motion(
                    &boat.boat,
                    boat.parameters,
                    supported,
                    &mut self.world.bodies,
                );
            }
        }
    }

    fn prepare_contact_charts(&mut self) -> usize {
        self.enforce_supported_motion();
        let motions = self
            .boats
            .iter()
            .map(|boat| read_motion(&boat.boat, &self.world.bodies))
            .collect::<Vec<_>>();
        let canonical_positions = motions
            .iter()
            .map(|motion| {
                Vector::from(
                    self.world_geometry
                        .wrap_position_m(motion.position_m.into()),
                )
            })
            .collect::<Vec<_>>();
        let hull_radii_m = self
            .boats
            .iter()
            .map(|boat| {
                let hull = boat.parameters.hull;
                (hull.length_m / 2.0).hypot(hull.beam_m / 2.0)
            })
            .collect::<Vec<_>>();
        let search_radii_m = motions
            .iter()
            .zip(&hull_radii_m)
            .map(|(motion, radius_m)| {
                radius_m
                    + (motion.velocity_m_s.length()
                        + motion.angular_velocity_rad_s.abs() * radius_m)
                        * DT_SECONDS
                    + CONTACT_CHART_HYSTERESIS_M / 2.0
            })
            .collect::<Vec<_>>();
        let candidate_search =
            cylindrical_candidate_pairs(self.world_geometry, &canonical_positions, &search_radii_m);
        let mut neighbors = vec![Vec::new(); self.boats.len()];
        let mut substeps = 1;
        for (first, second) in candidate_search.pairs {
            let first_radius_m = hull_radii_m[first];
            let second_radius_m = hull_radii_m[second];
            let swept_distance_m = ((motions[second].velocity_m_s - motions[first].velocity_m_s)
                .length()
                + motions[first].angular_velocity_rad_s.abs() * first_radius_m
                + motions[second].angular_velocity_rad_s.abs() * second_radius_m)
                * DT_SECONDS;
            let displacement_m = Vector::from(self.world_geometry.minimum_image_displacement_m(
                (canonical_positions[second] - canonical_positions[first]).into(),
            ));
            let candidate_distance_m =
                first_radius_m + second_radius_m + swept_distance_m + CONTACT_CHART_HYSTERESIS_M;
            if displacement_m.length_squared() > candidate_distance_m * candidate_distance_m {
                continue;
            }
            neighbors[first].push(second);
            neighbors[second].push(first);
            let collision_resolution_m = self.boats[first]
                .parameters
                .hull
                .beam_m
                .min(self.boats[second].parameters.hull.beam_m)
                / 2.0;
            substeps = substeps.max(collision_substep_policy(
                swept_distance_m,
                collision_resolution_m,
            ));
        }

        let mut chart_positions = vec![Vector::ZERO; self.boats.len()];
        let mut visited = vec![false; self.boats.len()];
        for root in 0..self.boats.len() {
            if visited[root] {
                continue;
            }
            visited[root] = true;
            chart_positions[root] = canonical_positions[root];
            let mut queue = VecDeque::from([root]);
            while let Some(first) = queue.pop_front() {
                for &second in &neighbors[first] {
                    if visited[second] {
                        continue;
                    }
                    let displacement_m = self.world_geometry.minimum_image_displacement_m(
                        (canonical_positions[second] - canonical_positions[first]).into(),
                    );
                    chart_positions[second] = chart_positions[first] + Vector::from(displacement_m);
                    visited[second] = true;
                    queue.push_back(second);
                }
            }
        }

        for (boat, target_position_m) in self.boats.iter().zip(chart_positions) {
            let current_position_m = self.world.bodies[boat.boat.hull.body].translation();
            let translation_m = target_position_m - current_position_m;
            if translation_m == Vector::ZERO {
                continue;
            }
            // A chart change must preserve every joint anchor in the assembly.
            for body in [boat.boat.hull.body, boat.boat.sail, boat.boat.rudder] {
                let position_m = self.world.bodies[body].translation() + translation_m;
                self.world.bodies[body].set_translation(position_m, true);
            }
        }
        substeps
    }

    fn step_and_collect_contacts(
        &mut self,
        substeps: usize,
        environments: &[Environment],
        actuator_torques: &[ActuatorTorques],
    ) -> Vec<HullContact> {
        let collision_maximum_step_seconds = DT_SECONDS / substeps as f64;
        let mut remaining_seconds = DT_SECONDS;
        let mut contacts_by_pair = HashMap::<(usize, usize), HullContact>::new();
        while remaining_seconds > 0.0 {
            let mut step_seconds = remaining_seconds.min(collision_maximum_step_seconds);
            for (boat, environment) in self.boats.iter().zip(environments) {
                step_seconds = step_seconds.min(fluid_maximum_step_seconds(
                    &self.world,
                    boat.parameters,
                    boat.boat,
                    *environment,
                ));
            }
            self.world.integration_parameters.dt = step_seconds;
            for ((boat, environment), torques) in
                self.boats.iter().zip(environments).zip(actuator_torques)
            {
                apply_boat_forces(
                    &mut self.world,
                    boat.parameters,
                    boat.boat,
                    *environment,
                    *torques,
                );
            }
            self.world.step();
            for mut contact in self.hull_contacts() {
                if contact.first_boat > contact.second_boat {
                    std::mem::swap(&mut contact.first_boat, &mut contact.second_boat);
                    std::mem::swap(
                        &mut contact.first_contact_local_m,
                        &mut contact.second_contact_local_m,
                    );
                }
                contacts_by_pair
                    .entry((contact.first_boat, contact.second_boat))
                    .and_modify(|aggregate| {
                        aggregate.impulse_magnitude_n_s += contact.impulse_magnitude_n_s;
                    })
                    .or_insert(contact);
            }
            self.enforce_supported_motion();
            remaining_seconds = (remaining_seconds - step_seconds).max(0.0);
        }
        self.world.integration_parameters.dt = DT_SECONDS;
        self.refresh_spatial_index();
        let mut contacts = contacts_by_pair.into_values().collect::<Vec<_>>();
        contacts.sort_unstable_by_key(|contact| (contact.first_boat, contact.second_boat));
        contacts
    }

    fn hull_contacts(&self) -> Vec<HullContact> {
        let mut contacts = self
            .world
            .contact_pairs()
            .filter(|pair| pair.has_any_active_contact())
            .filter_map(|pair| {
                let first_boat = *self.boat_indices_by_collider.get(&pair.collider1)?;
                let second_boat = *self.boat_indices_by_collider.get(&pair.collider2)?;
                let (first_contact_local_m, second_contact_local_m, contact_count) = pair
                    .manifolds()
                    .iter()
                    .flat_map(|manifold| &manifold.points)
                    .fold(
                        (Vector::ZERO, Vector::ZERO, 0_u32),
                        |(first, second, count), contact| {
                            (
                                first + contact.local_p1,
                                second + contact.local_p2,
                                count + 1,
                            )
                        },
                    );
                if contact_count == 0 {
                    return None;
                }
                Some(HullContact {
                    first_boat,
                    second_boat,
                    impulse_magnitude_n_s: pair.total_impulse_magnitude(),
                    first_contact_local_m: first_contact_local_m / contact_count as f64,
                    second_contact_local_m: second_contact_local_m / contact_count as f64,
                })
            })
            .collect::<Vec<_>>();
        contacts.sort_unstable_by_key(|contact| {
            (
                contact.first_boat.min(contact.second_boat),
                contact.first_boat.max(contact.second_boat),
            )
        });
        contacts
    }
}

#[cfg(test)]
mod cylinder_spatial_tests {
    use super::*;

    #[test]
    fn wrapped_candidate_search_excludes_remote_quadratic_pairs() {
        let world_geometry = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
        let mut positions_m = (0..1_000)
            .map(|index| {
                Vector::new(
                    -3_000.0 + (index % 100) as f64 * 40.0,
                    -3_000.0 + (index / 100) as f64 * 40.0,
                )
            })
            .collect::<Vec<_>>();
        positions_m.extend([
            Vector::new(3_999.0, 1_000.0),
            Vector::new(-3_999.0, 1_000.0),
        ]);
        let search = cylindrical_candidate_pairs(
            world_geometry,
            &positions_m,
            &vec![3.0; positions_m.len()],
        );

        assert_eq!(search.pairs, vec![(1_000, 1_001)]);
        assert!(search.exact_checks < positions_m.len() * 4);
    }

    #[test]
    fn one_large_swept_extent_does_not_expand_every_ordinary_query() {
        let world_geometry = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
        let mut positions_m = (0..1_000)
            .map(|index| {
                Vector::new(
                    -3_000.0 + (index % 100) as f64 * 40.0,
                    -3_000.0 + (index / 100) as f64 * 40.0,
                )
            })
            .collect::<Vec<_>>();
        positions_m.push(Vector::new(0.0, -2_800.0));
        let mut search_radii_m = vec![3.0; positions_m.len()];
        search_radii_m[1_000] = 1_000.0;
        let search = cylindrical_candidate_pairs(world_geometry, &positions_m, &search_radii_m);

        assert!(search.pairs.contains(&(575, 1_000)));
        assert!(!search.pairs.contains(&(0, 1_000)));
        assert!(search.exact_checks < positions_m.len() * 4);
    }

    #[test]
    fn broad_radius_query_scans_the_finite_occupied_index() {
        let world_geometry = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
        let index = CylindricalSpatialIndex::new(
            world_geometry,
            &[
                Vector::new(-3_000.0, -3_000.0),
                Vector::new(3_000.0, 3_000.0),
            ],
        );

        assert_eq!(index.query(Vector::ZERO, f64::MAX), vec![0, 1]);
    }

    #[test]
    fn collision_substeps_cover_maximum_supported_opposing_motion() {
        let swept_distance_m = 2.0 * MAXIMUM_SHARED_HULL_POINT_SPEED_M_S * DT_SECONDS;
        assert_eq!(collision_substep_policy(swept_distance_m, 0.75), 32);
        assert!(swept_distance_m / (MAXIMUM_COLLISION_SUBSTEPS as f64) < 0.75);
    }
}
