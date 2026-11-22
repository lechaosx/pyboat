use game::{
    dinghy_parameters, procedural_match_environment, procedural_world_environment, sailor_trial,
};
use rapier2d_f64::prelude::Vector;
use simulation::{BoatMotion, Environment, MotionIntent, SharedSimulation, WorldGeometry};

use crate::{BoatScene, Windsock, boat_scene, player_motion_intent};

pub struct PlaygroundBoat {
    simulation: SharedSimulation,
    pub(crate) geometry: WorldGeometry,
    pub(crate) seed: u64,
    tick: u64,
    input: [f32; 4],
    intent: MotionIntent,
    force_trim: bool,
    load_aware_helm: bool,
}

impl PlaygroundBoat {
    pub fn spawn(geometry: WorldGeometry, seed: u64, tick: u64, center: Vector) -> Option<Self> {
        let parameters = dinghy_parameters();
        let position = (0..513).find_map(|sample| {
            let radius = if sample == 0 {
                0.0
            } else {
                100.0 * f64::from(sample).sqrt()
            };
            let angle = f64::from(sample) * 2.399_963_229_728_653;
            let position = center + Vector::new(angle.cos(), angle.sin()) * radius;
            (procedural_world_environment(geometry, seed, tick, position.as_vec2())
                .environment
                .water_depth_m
                > parameters.draft_m + 0.5)
                .then_some(position)
        })?;
        let wind = procedural_world_environment(geometry, seed, tick, position.as_vec2())
            .environment
            .true_wind_velocity_m_s;
        let angle_rad = wind.y.atan2(wind.x);
        let motion = BoatMotion {
            position_m: position,
            // At rest in the water, which its current and waves move.
            velocity_m_s: procedural_match_environment(geometry, seed, tick, position, angle_rad)
                .true_water_velocity_m_s,
            angle_rad,
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: 0.0,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.0,
            rudder_angular_velocity_rad_s: 0.0,
        };
        let mut simulation = SharedSimulation::new_with_world_geometry(geometry);
        simulation.add_boat(parameters, motion);
        Some(Self {
            simulation,
            geometry,
            seed,
            tick,
            input: [0.0; 4],
            intent: MotionIntent::default(),
            force_trim: false,
            load_aware_helm: false,
        })
    }

    pub fn motion(&self) -> BoatMotion {
        self.simulation.motion(0)
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn intent(&self) -> MotionIntent {
        self.intent
    }
    pub fn environment(&self) -> Environment {
        let motion = self.motion();
        procedural_match_environment(
            self.geometry,
            self.seed,
            self.tick,
            motion.position_m,
            motion.angle_rad,
        )
    }
    pub fn set_player_input(&mut self, input: [f32; 4]) {
        self.input = input;
    }
    pub fn set_sailor_options(&mut self, force_trim: bool, load_aware_helm: bool) {
        self.force_trim = force_trim;
        self.load_aware_helm = load_aware_helm;
    }
    pub fn rebase(&mut self, geometry: WorldGeometry, seed: u64, tick: u64) {
        if geometry != self.geometry {
            let motion = self.motion();
            self.simulation = SharedSimulation::new_with_world_geometry(geometry);
            self.simulation.add_boat(dinghy_parameters(), motion);
        }
        self.geometry = geometry;
        self.seed = seed;
        self.tick = tick;
    }
    pub fn step(&mut self) {
        self.intent = player_motion_intent(
            Some(self.motion().angle_rad),
            self.input[0],
            self.input[1],
            self.input[2],
            self.input[3],
        );
        let environment = self.environment();
        let targets = sailor_trial::targets(
            dinghy_parameters(),
            self.motion(),
            environment,
            self.intent,
            self.force_trim,
            self.load_aware_helm,
        );
        self.simulation
            .advance_controlled_with_environments(&[environment], &[Some(targets)]);
        self.tick += 1;
    }
    pub fn scene(&self) -> BoatScene {
        self.scene_near_camera(self.motion().position_m)
    }
    pub fn scene_near_camera(&self, camera: Vector) -> BoatScene {
        let mut motion = self.motion();
        motion.position_m = camera
            + Vector::from(
                self.geometry
                    .minimum_image_displacement_m((motion.position_m - camera).into()),
            );
        let mut scene = boat_scene(dinghy_parameters(), motion);
        scene.id = Some("playground-boat".into());
        scene.is_local = true;
        let sample = procedural_world_environment(
            self.geometry,
            self.seed,
            self.tick,
            motion.position_m.as_vec2(),
        );
        scene.apparent_wind_velocity_m_s =
            (sample.environment.true_wind_velocity_m_s - motion.velocity_m_s).as_vec2();
        scene.sea = Some(sample.sea);
        scene.windsock = Some(Windsock {
            anchor_m: scene.hull_m[0],
            apparent_wind_velocity_m_s: scene.apparent_wind_velocity_m_s,
        });
        scene
    }
}
