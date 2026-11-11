use game::{dinghy_parameters, procedural_match_environment};
use rapier2d_f64::prelude::Vector;
use simulation::{BoatMotion, Environment, MotionIntent, SharedSimulation, WorldGeometry};

use crate::{
    BoatScene, Windsock, boat_scene,
    controller_lab::{LabController, LabControllerKind},
    player_motion_intent,
};

pub struct PlaygroundBoat {
    simulation: SharedSimulation,
    controller: LabController,
    pub(crate) geometry: WorldGeometry,
    pub(crate) seed: u64,
    tick: u64,
    input: [f32; 4],
    intent: MotionIntent,
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
            (procedural_match_environment(geometry, seed, tick, position).water_depth_m
                > parameters.draft_m + 0.5)
                .then_some(position)
        })?;
        let environment = procedural_match_environment(geometry, seed, tick, position);
        let wind = environment.true_wind_velocity_m_s;
        let motion = BoatMotion {
            position_m: position,
            velocity_m_s: Vector::ZERO,
            angle_rad: wind.y.atan2(wind.x),
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
            controller: LabController::new(LabControllerKind::Current),
            geometry,
            seed,
            tick,
            input: [0.0; 4],
            intent: MotionIntent::default(),
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
    pub fn kind(&self) -> LabControllerKind {
        self.controller.kind()
    }
    pub fn environment(&self) -> Environment {
        procedural_match_environment(
            self.geometry,
            self.seed,
            self.tick,
            self.motion().position_m,
        )
    }
    pub fn set_player_input(&mut self, input: [f32; 4]) {
        self.input = input;
    }
    pub fn select(&mut self, kind: LabControllerKind) {
        self.controller.select(kind);
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
        self.controller.reset();
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
        let targets = self.controller.targets(
            &self.simulation,
            0,
            dinghy_parameters(),
            environment,
            self.intent,
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
        scene.apparent_wind_velocity_m_s =
            (self.environment().true_wind_velocity_m_s - motion.velocity_m_s).as_vec2();
        scene.windsock = Some(Windsock {
            anchor_m: scene.hull_m[0],
            apparent_wind_velocity_m_s: scene.apparent_wind_velocity_m_s,
        });
        scene
    }
}
