use rapier2d::prelude::{
    ColliderBuilder, ColliderHandle, ColliderSet, ImpulseJointHandle, ImpulseJointSet,
    IntegrationParameters, MassProperties, MotorModel, PhysicsWorld, RevoluteJointBuilder,
    RigidBodyBuilder, RigidBodyHandle, RigidBodySet, Rotation, Vector,
};

pub const DT_SECONDS: f32 = 1.0 / 60.0;

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

    pub fn motion(&self) -> BoatMotion {
        read_motion(&self.boat, &self.world.bodies)
    }
}
