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
    pub rudder_mass_kg: f32,
    pub tiller_mass_kg: f32,
    pub rudder_draw_length_m: f32,
    pub tiller_length_m: f32,
    pub rudder_position_m: Vector,
    pub max_rudder_angle_rad: f32,
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

fn sail_mass_properties(parameters: BoatParameters) -> MassProperties {
    let mass_kg = parameters.sail_mass_kg + parameters.boom_mass_kg;
    let hinge_inertia = mass_kg * parameters.boom_length_m.powi(2) / 3.0;
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

fn rudder_mass_properties(parameters: BoatParameters) -> MassProperties {
    let mass_kg = parameters.rudder_mass_kg + parameters.tiller_mass_kg;
    let hinge_inertia = parameters.rudder_mass_kg * parameters.rudder_draw_length_m.powi(2) / 3.0
        + parameters.tiller_mass_kg * parameters.tiller_length_m.powi(2) / 3.0;
    let center = rudder_center_from_hinge_m(parameters);
    MassProperties::new(
        Vector::ZERO,
        mass_kg,
        hinge_inertia - mass_kg * center.length_squared(),
    )
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

    pub fn advance(&mut self, actuator_torques: ActuatorTorques) {
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
