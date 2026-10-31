use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    error::Error,
    fmt,
    ops::RangeInclusive,
};

use rapier2d::prelude::{
    ColliderBuilder, ColliderHandle, ColliderSet, ImpulseJointHandle, ImpulseJointSet,
    IntegrationParameters, MassProperties, MotorModel, PhysicsWorld, RevoluteJointBuilder,
    RigidBodyBuilder, RigidBodyHandle, RigidBodySet, RigidBodyType, Rotation, Vector,
};
use serde::{Deserialize, Deserializer, Serialize, de};

pub const TICKS_PER_SECOND: u64 = 60;
pub const DT_SECONDS: f32 = 1.0 / TICKS_PER_SECOND as f32;
// Opposing hull extrema at this speed remain below half the minimum supported beam per 1/32 tick.
pub const MAXIMUM_SHARED_HULL_POINT_SPEED_M_S: f32 = 700.0;
const CONTROL_SELECTION_TIE_TOLERANCE: f32 = 1.0e-6;
const GRAVITY_ACCELERATION_M_S2: f32 = 9.81;
const STATIC_GROUND_SPEED_THRESHOLD_M_S: f32 = 0.02;
const CONTACT_CHART_HYSTERESIS_M: f32 = 0.5;
const SPATIAL_CELL_M: f32 = 16.0;
const MAXIMUM_COLLISION_SUBSTEPS: usize = 32;
const MINIMUM_SHARED_HULL_BEAM_M: f32 = 1.5;

const DEFAULT_WORLD_SIDE_M: f32 = 64_000.0;
const MINIMUM_WORLD_SIDE_M: f32 = 8_000.0;
const MAXIMUM_WORLD_SIDE_M: f32 = 256_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct WorldGeometry {
    width_m: f32,
    height_m: f32,
}

impl WorldGeometry {
    pub fn new(width_m: f32, height_m: f32) -> Result<Self, WorldGeometryError> {
        if !valid_world_dimension(width_m) || !valid_world_dimension(height_m) {
            return Err(WorldGeometryError);
        }
        Ok(Self { width_m, height_m })
    }

    pub fn width_m(self) -> f32 {
        self.width_m
    }

    pub fn height_m(self) -> f32 {
        self.height_m
    }

    pub fn wrap_position_m(self, position_m: [f32; 2]) -> [f32; 2] {
        [wrap_component(position_m[0], self.width_m), position_m[1]]
    }

    pub fn minimum_image_displacement_m(self, displacement_m: [f32; 2]) -> [f32; 2] {
        [
            minimum_image_component(displacement_m[0], self.width_m),
            displacement_m[1],
        ]
    }

    pub fn normalized_world_coordinates(self, position_m: [f32; 2]) -> [f32; 2] {
        let wrapped_m = self.wrap_position_m(position_m);
        [
            normalized_component(wrapped_m[0], self.width_m),
            (wrapped_m[1] / self.height_m + 0.5).clamp(0.0, 1.0),
        ]
    }

    pub fn latitude_degrees(self, y_m: f32) -> f32 {
        let degrees = self.latitude_phase_degrees(y_m);
        if degrees > 90.0 {
            180.0 - degrees
        } else if degrees < -90.0 {
            -180.0 - degrees
        } else {
            degrees
        }
    }

    pub fn latitude_gradient_degrees_per_m(self, y_m: f32) -> f32 {
        let direction = if self.latitude_phase_degrees(y_m).abs() > 90.0 {
            -1.0
        } else {
            1.0
        };
        direction * 180.0 / self.height_m
    }

    pub fn latitude_phase_degrees(self, y_m: f32) -> f32 {
        let period_m = 2.0 * self.height_m;
        let cycle_y_m = y_m.rem_euclid(period_m);
        let degrees = 180.0 * cycle_y_m / self.height_m;
        if degrees >= 180.0 {
            degrees - 360.0
        } else {
            degrees
        }
    }

    pub fn local_solar_phase(self, global_day_phase: f32, x_m: f32) -> f32 {
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
            width_m: f32,
            height_m: f32,
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

fn valid_world_dimension(dimension_m: f32) -> bool {
    let half_m = dimension_m / 2.0;
    dimension_m.is_finite()
        && (MINIMUM_WORLD_SIDE_M..=MAXIMUM_WORLD_SIDE_M).contains(&dimension_m)
        && half_m * 2.0 == dimension_m
}

fn minimum_image_component(displacement_m: f32, period_m: f32) -> f32 {
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

fn wrap_component(value: f32, period: f32) -> f32 {
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

fn normalized_component(wrapped: f32, period: f32) -> f32 {
    const UNIT_UPPER_BOUND: f32 = f32::from_bits(1.0_f32.to_bits() - 1);
    (wrapped / period + 0.5).min(UNIT_UPPER_BOUND)
}

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
    pub draft_m: f32,
    pub ground_static_friction_coefficient: f32,
    pub ground_kinetic_friction_coefficient: f32,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub true_wind_velocity_m_s: Vector,
    pub true_water_velocity_m_s: Vector,
    pub water_depth_m: f32,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            true_wind_velocity_m_s: Vector::ZERO,
            true_water_velocity_m_s: Vector::ZERO,
            water_depth_m: f32::INFINITY,
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
    pub impulse_magnitude_n_s: f32,
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
    cell_width_m: f32,
    cells: HashMap<(i32, i32), Vec<usize>>,
}

impl CylindricalSpatialIndex {
    fn new(world_geometry: WorldGeometry, positions_m: &[Vector]) -> Self {
        Self::new_with_extents(world_geometry, positions_m, &vec![0.0; positions_m.len()])
    }

    fn new_with_extents(
        world_geometry: WorldGeometry,
        positions_m: &[Vector],
        extents_m: &[f32],
    ) -> Self {
        assert_eq!(positions_m.len(), extents_m.len());
        let columns = (world_geometry.width_m() / SPATIAL_CELL_M).floor() as i32;
        let cell_width_m = world_geometry.width_m() / columns as f32;
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

    fn query(&self, center_m: Vector, radius_m: f32) -> Vec<usize> {
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

fn linear_cell_indices(center_m: f32, extent_m: f32, cell_size_m: f32) -> RangeInclusive<i32> {
    let (minimum, maximum) = linear_cell_bounds(center_m, extent_m, cell_size_m);
    minimum..=maximum
}

fn linear_cell_bounds(center_m: f32, extent_m: f32, cell_size_m: f32) -> (i32, i32) {
    (
        ((center_m - extent_m) / cell_size_m).floor() as i32,
        ((center_m + extent_m) / cell_size_m).floor() as i32,
    )
}

fn wrapped_cell_indices(center_m: f32, extent_m: f32, cell_size_m: f32, count: i32) -> Vec<i32> {
    if extent_m * 2.0 >= cell_size_m * count as f32 {
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
    search_radii_m: &[f32],
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

fn collision_substep_policy(swept_distance_m: f32, collision_resolution_m: f32) -> usize {
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

pub fn grounding_friction_force(
    point_velocity_m_s: Vector,
    water_depth_m: f32,
    draft_m: f32,
    supported_mass_kg: f32,
    kinetic_friction_coefficient: f32,
) -> Vector {
    let support_fraction = ((draft_m - water_depth_m) / draft_m).clamp(0.0, 1.0);
    let maximum_force_n = kinetic_friction_coefficient
        * supported_mass_kg
        * GRAVITY_ACCELERATION_M_S2
        * support_fraction;
    let stopping_force_n = -point_velocity_m_s * supported_mass_kg / DT_SECONDS;
    if stopping_force_n.length() <= maximum_force_n {
        return stopping_force_n;
    }
    if point_velocity_m_s == Vector::ZERO {
        Vector::ZERO
    } else {
        -point_velocity_m_s.normalize() * maximum_force_n
    }
}

fn total_mass_kg(parameters: BoatParameters) -> f32 {
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
    water_depth_m: f32,
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
        let required_force_n = -external_force_n - hull.linvel() * total_mass_kg / DT_SECONDS;
        let required_torque_n_m = -external_torque_n_m - hull.angvel() * inertia_kg_m2 / DT_SECONDS;
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
        let friction_force_n = grounding_friction_force(
            velocity_m_s,
            water_depth_m,
            parameters.draft_m,
            total_mass_kg / 2.0,
            parameters.ground_kinetic_friction_coefficient,
        );
        world.bodies[boat.hull.body].add_force_at_point(friction_force_n, position_m, true);
    }
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
    let collider = ColliderBuilder::convex_hull(&outline)
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
    let sail = &world.bodies[boat.sail];
    let sail_angle_rad = sail.rotation().angle();
    let center_of_effort_m = sail.translation()
        + rotated(
            Vector::new(
                parameters.boom_length_m / 2.0 - parameters.sail_center_of_effort_from_mast_m,
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
        parameters.air_density_kg_m3,
        parameters.sail_area_m2,
        parameters.sail_base_drag_coefficient,
    );
    world.bodies[boat.sail].add_force_at_point(sail_force_n, center_of_effort_m, true);

    let rudder = &world.bodies[boat.rudder];
    let rudder_angle_rad = rudder.rotation().angle();
    let rudder_center_of_effort_m = rudder.translation()
        + rotated(
            Vector::new(-parameters.rudder_draw_length_m / 2.0, 0.0)
                - rudder_center_from_hinge_m(parameters),
            rudder_angle_rad,
        );
    let apparent_rudder_water_m_s = apparent_flow(
        environment.true_water_velocity_m_s,
        rudder.velocity_at_point(rudder_center_of_effort_m),
    );
    let rudder_force_n = streamlined_foil_force(
        apparent_rudder_water_m_s,
        rudder_angle_rad,
        parameters.water_density_kg_m3,
        parameters.rudder_area_m2,
        parameters.rudder_base_drag_coefficient,
        parameters.rudder_stall_angle_rad,
        parameters.rudder_stall_lift_coefficient,
    );
    world.bodies[boat.rudder].add_force_at_point(rudder_force_n, rudder_center_of_effort_m, true);

    let hull = &world.bodies[boat.hull.body];
    let hull_angle_rad = hull.rotation().angle();
    let keel_center_of_effort_m =
        hull.translation() + rotated(parameters.keel_position_m, hull_angle_rad);
    let apparent_keel_water_m_s = apparent_flow(
        environment.true_water_velocity_m_s,
        hull.velocity_at_point(keel_center_of_effort_m),
    );
    let keel_force_n = streamlined_foil_force(
        apparent_keel_water_m_s,
        hull_angle_rad,
        parameters.water_density_kg_m3,
        parameters.keel_area_m2,
        parameters.keel_base_drag_coefficient,
        parameters.keel_stall_angle_rad,
        parameters.keel_stall_lift_coefficient,
    );
    world.bodies[boat.hull.body].add_force_at_point(keel_force_n, keel_center_of_effort_m, true);

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
    apply_grounding_friction(world, parameters, boat, environment.water_depth_m);
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
        apply_boat_forces(
            &mut self.world,
            self.parameters,
            self.boat,
            environment,
            actuator_torques,
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

    pub fn boats_within_radius(&self, center_m: Vector, radius_m: f32) -> Vec<usize> {
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
        assert_eq!(actuator_torques.len(), self.boats.len());
        let substeps = self.prepare_contact_charts();
        for (boat, actuator_torques) in self.boats.iter().zip(actuator_torques) {
            apply_boat_forces(
                &mut self.world,
                boat.parameters,
                boat.boat,
                environment,
                *actuator_torques,
            );
        }
        self.step_and_collect_contacts(substeps)
    }

    pub fn advance_with_environments(
        &mut self,
        environments: &[Environment],
        actuator_torques: &[ActuatorTorques],
    ) -> Vec<HullContact> {
        assert_eq!(environments.len(), self.boats.len());
        assert_eq!(actuator_torques.len(), self.boats.len());
        let substeps = self.prepare_contact_charts();
        for ((boat, environment), actuator_torques) in
            self.boats.iter().zip(environments).zip(actuator_torques)
        {
            apply_boat_forces(
                &mut self.world,
                boat.parameters,
                boat.boat,
                *environment,
                *actuator_torques,
            );
        }
        self.step_and_collect_contacts(substeps)
    }

    pub fn motion(&self, boat: usize) -> BoatMotion {
        let mut motion = read_motion(&self.boats[boat].boat, &self.world.bodies);
        motion.position_m = Vector::from(
            self.world_geometry
                .wrap_position_m(motion.position_m.into()),
        );
        motion
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

    fn step_and_collect_contacts(&mut self, substeps: usize) -> Vec<HullContact> {
        self.world.integration_parameters.dt = DT_SECONDS / substeps as f32;
        let mut contacts_by_pair = HashMap::<(usize, usize), HullContact>::new();
        for _ in 0..substeps {
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
                    first_contact_local_m: first_contact_local_m / contact_count as f32,
                    second_contact_local_m: second_contact_local_m / contact_count as f32,
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
                    -3_000.0 + (index % 100) as f32 * 40.0,
                    -3_000.0 + (index / 100) as f32 * 40.0,
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
                    -3_000.0 + (index % 100) as f32 * 40.0,
                    -3_000.0 + (index / 100) as f32 * 40.0,
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

        assert_eq!(index.query(Vector::ZERO, f32::MAX), vec![0, 1]);
    }

    #[test]
    fn collision_substeps_cover_maximum_supported_opposing_motion() {
        let swept_distance_m = 2.0 * MAXIMUM_SHARED_HULL_POINT_SPEED_M_S * DT_SECONDS;
        assert_eq!(collision_substep_policy(swept_distance_m, 0.75), 32);
        assert!(swept_distance_m / (MAXIMUM_COLLISION_SUBSTEPS as f32) < 0.75);
    }
}
