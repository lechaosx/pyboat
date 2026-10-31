use std::collections::HashMap;

use rapier2d::prelude::Vector;

use crate::{BoatScene, windsock_extension};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClothPose {
    pub sail_angle_rad: f32,
    pub sail_load: f32,
    pub flutter_response: f32,
    pub sock_angle_rad: f32,
    pub sock_extension: f32,
}

#[derive(Clone, Copy)]
struct Mode {
    position: f64,
    velocity: f64,
}

impl Mode {
    fn new(position: f32) -> Self {
        Self {
            position: position.into(),
            velocity: 0.0,
        }
    }

    fn advance(&mut self, target: f32, frequency: f64, seconds: f64) {
        // Exact underdamped spring solution for a held target: long frames cannot
        // inject energy, and repeated renders with no elapsed time cannot move cloth.
        let damping = 0.85;
        let decay = frequency * damping;
        let oscillation = frequency * (1.0_f64 - damping * damping).sqrt();
        let offset = self.position - f64::from(target);
        let sine_coefficient = (self.velocity + decay * offset) / oscillation;
        let (sin, cos) = (oscillation * seconds).sin_cos();
        let envelope = (-decay * seconds).exp();
        self.position = f64::from(target) + envelope * (offset * cos + sine_coefficient * sin);
        self.velocity = envelope
            * (self.velocity * cos - (decay * sine_coefficient + oscillation * offset) * sin);
    }

    fn turn(&mut self, target: f32, frequency: f64, seconds: f64) {
        self.position = shortest_angle(self.position);
        let target = self.position + shortest_angle(f64::from(target) - self.position);
        self.advance(target as f32, frequency, seconds);
        self.position = shortest_angle(self.position);
    }
}

fn shortest_angle(angle: f64) -> f64 {
    (angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

fn targets(scene: &BoatScene) -> ClothPose {
    let boom = scene.sail_m[1] - scene.sail_m[0];
    let normal = Vector::new(-boom.y, boom.x).normalize();
    let wind = scene.apparent_wind_velocity_m_s;
    let across = wind.dot(normal);
    let load = across / (across * across + 4.0).sqrt();
    let speed = wind.length();
    ClothPose {
        sail_angle_rad: boom.y.atan2(boom.x) + 0.25 * load,
        sail_load: load,
        flutter_response: speed / (speed + 2.0),
        sock_angle_rad: wind.y.atan2(wind.x),
        sock_extension: windsock_extension(speed),
    }
}

pub struct ClothMotion {
    sail_angle: Mode,
    sail_load: Mode,
    response: Mode,
    sock_angle: Mode,
    sock_heading_target: f32,
    sock_extension: Mode,
}

impl ClothMotion {
    pub fn new(scene: &BoatScene) -> Self {
        let target = targets(scene);
        Self {
            sail_angle: Mode::new(target.sail_angle_rad),
            sail_load: Mode::new(target.sail_load),
            response: Mode::new(target.flutter_response),
            sock_angle: Mode::new(target.sock_angle_rad),
            sock_heading_target: target.sock_angle_rad,
            sock_extension: Mode::new(target.sock_extension),
        }
    }

    pub fn advance(&mut self, scene: &BoatScene, seconds: f64) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let target = targets(scene);
        // These are visual modal estimates, not calibrated canvas properties.
        // Pressure stiffens the sail; the loose sock responds more slowly. Both
        // retain velocity when wind or the supporting boom changes direction.
        let sail_frequency = 5.0 + 6.0 * f64::from(target.sail_load * target.sail_load);
        self.sail_angle
            .turn(target.sail_angle_rad, sail_frequency, seconds);
        self.sail_load
            .advance(target.sail_load, sail_frequency, seconds);
        self.response.advance(target.flutter_response, 6.0, seconds);
        if scene.apparent_wind_velocity_m_s.length_squared() > f32::EPSILON {
            self.sock_heading_target = target.sock_angle_rad;
        }
        self.sock_angle.turn(self.sock_heading_target, 5.0, seconds);
        self.sock_extension
            .advance(target.sock_extension, 6.0, seconds);
    }

    pub fn pose(&self) -> ClothPose {
        ClothPose {
            sail_angle_rad: self.sail_angle.position as f32,
            sail_load: (self.sail_load.position as f32).clamp(-1.0, 1.0),
            flutter_response: (self.response.position as f32).clamp(0.0, 1.0),
            sock_angle_rad: self.sock_angle.position as f32,
            sock_extension: (self.sock_extension.position as f32).clamp(0.0, 1.0),
        }
    }
}

#[derive(Default)]
pub struct ClothPresentation {
    boats: HashMap<String, ClothMotion>,
}

impl ClothPresentation {
    pub fn advance(&mut self, scenes: &[BoatScene], seconds: f64) -> Vec<ClothPose> {
        self.boats
            .retain(|id, _| scenes.iter().any(|scene| scene.id.as_ref() == Some(id)));
        scenes
            .iter()
            .map(|scene| {
                if let Some(id) = &scene.id {
                    let motion = self
                        .boats
                        .entry(id.clone())
                        .or_insert_with(|| ClothMotion::new(scene));
                    motion.advance(scene, seconds);
                    motion.pose()
                } else {
                    ClothMotion::new(scene).pose()
                }
            })
            .collect()
    }
}
