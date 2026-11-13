//! cargo run --release -p browser --features playground --example train-standalone-controller -- OUTPUT_DIR [GENERATIONS]
use browser::controller_lab::{
    LabController, LabControllerKind,
    standalone::{WEIGHTS, targets_with_weights},
};
use rapier2d_f64::prelude::Vector;
use simulation::{BoatMotion, DT_SECONDS, Environment, MotionIntent, SharedSimulation};
use std::{
    fs,
    io::{self, Write},
    path::Path,
    time::Instant,
};

const TICKS: usize = 240;
const PAIRS: usize = 32;
const BATCH: usize = 24;
const WORKERS: usize = 8;
const SIGMA: f64 = 0.05;
const LEARNING_RATE: f64 = 0.03;
const ACCELERATION: f64 = 0.5;
const YAW_RATE: f64 = 0.4;

struct Random(u64);
impl Random {
    fn new(seed: u64) -> Self {
        // SplitMix64 conditions sequential fixture and initialization seeds.
        let mut state = seed.wrapping_add(0x9e3779b97f4a7c15);
        state = (state ^ (state >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        state = (state ^ (state >> 27)).wrapping_mul(0x94d049bb133111eb);
        Self(state ^ (state >> 31))
    }
    fn unit(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1_u64 << 53) as f64
    }
    fn normal(&mut self) -> f64 {
        (-2.0 * self.unit().max(1e-15).ln()).sqrt() * (std::f64::consts::TAU * self.unit()).cos()
    }
}
struct Fixture {
    seed: u64,
    initial: BoatMotion,
    intent: MotionIntent,
    environments: Vec<Environment>,
    passive_speeds: Vec<f64>,
}
fn speed(motion: BoatMotion, environment: Environment) -> f64 {
    (motion.velocity_m_s - environment.true_water_velocity_m_s)
        .dot(Vector::new(motion.angle_rad.cos(), motion.angle_rad.sin()))
}
fn fixtures(start: u64, count: usize) -> Vec<Fixture> {
    (start..start + count as u64)
        .map(|seed| {
            let mut rng = Random::new(seed);
            let sign = if rng.unit() < 0.5 { 1.0 } else { -1.0 };
            let powers = [-1.0, -0.5, -0.1, -1e-6, 0.0, 1e-6, 0.1, 0.5, 1.0];
            let turns = [-1.0, -0.5, -0.1, 0.0, 0.1, 0.5, 1.0];
            let intent = match seed % 4 {
                0 => MotionIntent {
                    forward: powers[(rng.unit() * 9.0) as usize],
                    turn: 0.0,
                },
                1 => MotionIntent {
                    forward: 0.0,
                    turn: turns[(rng.unit() * 7.0) as usize],
                },
                2 => MotionIntent::default(),
                _ => MotionIntent {
                    forward: powers[(rng.unit() * 9.0) as usize],
                    turn: turns[(rng.unit() * 7.0) as usize],
                },
            };
            let heading = rng.unit() * std::f64::consts::TAU;
            let water = Vector::new((rng.unit() - 0.5) * 0.5, (rng.unit() - 0.5) * 0.5);
            let current = Vector::new(0.2 * rng.normal(), 0.2 * rng.normal());
            let rotate = |v| rapier2d_f64::prelude::Rotation::new(heading).transform_vector(v);
            let initial_speed = if seed % 3 == 0 { 0.0 } else { rng.unit() * 3.0 };
            let initial = BoatMotion {
                position_m: Vector::ZERO,
                velocity_m_s: rotate(Vector::new(initial_speed, 0.0) + water),
                angle_rad: heading,
                angular_velocity_rad_s: if seed % 3 == 0 {
                    0.0
                } else {
                    sign * rng.unit() * 0.2
                },
                sail_angle_rad: sign * (0.1 + rng.unit() * 0.6),
                sail_angular_velocity_rad_s: sign * rng.unit() * 0.3,
                rudder_angle_rad: sign * rng.unit() * 0.2,
                rudder_angular_velocity_rad_s: 0.0,
            };
            let bearing = (rng.unit() * 8.0).floor() * std::f64::consts::FRAC_PI_4;
            let wind = Vector::new(bearing.cos(), sign * bearing.sin()) * (4.0 + rng.unit() * 5.0);
            let environments = (0..TICKS)
                .map(|tick| {
                    let seconds = tick as f64 * DT_SECONDS;
                    Environment {
                        true_wind_velocity_m_s: rotate(
                            wind * (1.0 + 0.2 * (seconds * 0.8 + seed as f64).sin()),
                        ),
                        true_water_velocity_m_s: rotate(water + current * (seconds * 0.5).sin()),
                        water_depth_m: f64::INFINITY,
                    }
                })
                .collect::<Vec<_>>();
            let mut simulation = SharedSimulation::new();
            let boat = simulation.add_boat(game::dinghy_parameters(), initial);
            let passive_speeds = environments
                .iter()
                .map(|&environment| {
                    simulation.advance_controlled_with_environments(&[environment], &[None]);
                    speed(simulation.motion(boat), environment)
                })
                .collect();
            Fixture {
                seed,
                initial,
                intent,
                environments,
                passive_speeds,
            }
        })
        .collect()
}
fn episode(
    weights: Option<&[f64; WEIGHTS]>,
    kind: LabControllerKind,
    fixture: &Fixture,
) -> (f64, f64, f64, f64) {
    let parameters = game::dinghy_parameters();
    let mut simulation = SharedSimulation::new();
    let boat = simulation.add_boat(parameters, fixture.initial);
    let mut controller = LabController::new(kind);
    let mut forward_error = 0.0;
    let mut turn_error = 0.0;
    let mut progress = 0.0;
    let mut turn = 0.0;
    // Response preferences, not physical constants: coast-relative acceleration and yaw rate.
    // The passive twin has no actuators, so its sail and rudder remain free under fluid loads.
    for tick in 0..TICKS {
        let environment = fixture.environments[tick];
        let targets = if let Some(weights) = weights {
            targets_with_weights(
                &simulation,
                boat,
                parameters,
                environment,
                fixture.intent,
                weights,
            )
        } else {
            controller.targets(&simulation, boat, parameters, environment, fixture.intent)
        };
        simulation.advance_controlled_with_environments(&[environment], &[Some(targets)]);
        let motion = simulation.motion(boat);
        let u = speed(motion, environment);
        let desired = fixture.passive_speeds[tick]
            + fixture.intent.forward * ACCELERATION * (tick + 1) as f64 * DT_SECONDS;
        forward_error += ((u - desired) / (ACCELERATION * TICKS as f64 * DT_SECONDS)).powi(2);
        turn_error +=
            ((motion.angular_velocity_rad_s - fixture.intent.turn * YAW_RATE) / YAW_RATE).powi(2);
        progress += u * DT_SECONDS;
        turn += motion.angular_velocity_rad_s * DT_SECONDS;
    }
    (
        forward_error / TICKS as f64,
        turn_error / TICKS as f64,
        progress,
        turn,
    )
}
fn score(weights: &[f64; WEIGHTS], bank: &[Fixture], indices: &[usize]) -> f64 {
    indices
        .iter()
        .map(|&i| {
            let (f, t, _, _) =
                episode(Some(weights), LabControllerKind::StandaloneNeural, &bank[i]);
            -(f + t)
        })
        .sum::<f64>()
        / indices.len() as f64
}
fn checkpoint(
    path: &Path,
    weights: &[f64; WEIGHTS],
    seed: u64,
    generation: usize,
    validation: f64,
) -> io::Result<()> {
    let mut file = fs::File::create(path)?;
    writeln!(
        file,
        "// Generated by train-standalone-controller; seed={seed}, generation={generation}, validation_score={validation:.17e}."
    )?;
    writeln!(file, "pub const TRAINED_WEIGHTS: [f64; 338] = [")?;
    for weight in weights {
        writeln!(file, "    {weight:?},")?;
    }
    writeln!(file, "];")
}
fn main() -> io::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|a| a == "--help") {
        println!(
            "Usage: train-standalone-controller OUTPUT_DIR [GENERATIONS=100]\nTrains all 338 weights of an 18→16→2 softsign policy on actual SharedSimulation ticks. Three initialization seeds, antithetic rank-shaped evolution strategies; 32 pairs, 24 four-second fixtures/generation, 8 workers. Train seeds 10000..10383, validation 20000..20047, untouched final test 30000..30095. Initial checkpoint competes with every tenth generation on validation; final test runs once after selection. Equal weights penalize coast-relative forward-speed error and yaw-rate error; forward intent asks for 0.5 m/s² relative to a freely coasting twin and turn asks for 0.4 rad/s at full input. No teacher actions, heading targets or maneuver planner. Outputs can actively push or pull the sail. Writes checkpoints, learning.csv, final.csv and metadata.txt; does not change installed weights."
        );
        return Ok(());
    }
    let output = args
        .first()
        .map(String::as_str)
        .unwrap_or(".parked-foil-work/standalone-neural");
    let generations = args
        .get(1)
        .map(|s| s.parse::<usize>().expect("generation count"))
        .unwrap_or(100);
    fs::create_dir_all(output)?;
    let root = Path::new(output);
    let clock = Instant::now();
    let training = fixtures(10000, 384);
    let validation = fixtures(20000, 48);
    let validation_indices = (0..validation.len()).collect::<Vec<_>>();
    let mut log = fs::File::create(root.join("learning.csv"))?;
    writeln!(
        log,
        "seed,generation,training_score,validation_score,best_validation,seconds"
    )?;
    let mut global = ([0.0; WEIGHTS], f64::NEG_INFINITY, 0, 0);
    for seed in [71_u64, 113, 197] {
        let mut rng = Random::new(seed);
        let mut weights = std::array::from_fn(|i| {
            rng.normal()
                * if i < 304 {
                    0.5 / 18_f64.sqrt()
                } else {
                    0.1 / 16_f64.sqrt()
                }
        });
        let initial_score = score(&weights, &validation, &validation_indices);
        let mut best = (weights, initial_score, 0);
        checkpoint(
            &root.join(format!("seed-{seed}-initial.rs")),
            &weights,
            seed,
            0,
            initial_score,
        )?;
        writeln!(
            log,
            "{seed},0,,{initial_score},{initial_score},{}",
            clock.elapsed().as_secs_f64()
        )?;
        for generation in 1..=generations {
            let batch = (0..BATCH)
                .map(|_| (rng.unit() * training.len() as f64) as usize)
                .collect::<Vec<_>>();
            let perturbations: Vec<[f64; WEIGHTS]> = (0..PAIRS)
                .map(|_| std::array::from_fn(|_| rng.normal()))
                .collect();
            let results = std::thread::scope(|scope| {
                let handles = (0..WORKERS)
                    .map(|worker| {
                        let perturbations = &perturbations;
                        let batch = &batch;
                        let training = &training;
                        let weights = &weights;
                        scope.spawn(move || {
                            (worker..PAIRS)
                                .step_by(WORKERS)
                                .map(|pair| {
                                    let plus = std::array::from_fn(|i| {
                                        weights[i] + SIGMA * perturbations[pair][i]
                                    });
                                    let minus = std::array::from_fn(|i| {
                                        weights[i] - SIGMA * perturbations[pair][i]
                                    });
                                    (
                                        pair,
                                        score(&plus, training, batch),
                                        score(&minus, training, batch),
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .collect::<Vec<_>>();
                handles
                    .into_iter()
                    .flat_map(|handle| handle.join().unwrap())
                    .collect::<Vec<_>>()
            });
            let mut ranked = results
                .iter()
                .flat_map(|&(pair, plus, minus)| [(pair, 1.0, plus), (pair, -1.0, minus)])
                .collect::<Vec<_>>();
            ranked.sort_by(|a, b| a.2.total_cmp(&b.2));
            let mean_score = ranked.iter().map(|x| x.2).sum::<f64>() / ranked.len() as f64;
            for (rank, (pair, sign, _)) in ranked.iter().enumerate() {
                let utility = rank as f64 / (2 * PAIRS - 1) as f64 - 0.5;
                for i in 0..WEIGHTS {
                    weights[i] += LEARNING_RATE * utility * sign * perturbations[*pair][i]
                        / (PAIRS as f64 * SIGMA);
                }
            }
            if generation % 10 == 0 || generation == generations {
                let held = score(&weights, &validation, &validation_indices);
                if held > best.1 {
                    best = (weights, held, generation);
                }
                writeln!(
                    log,
                    "{seed},{generation},{mean_score},{held},{},{}",
                    best.1,
                    clock.elapsed().as_secs_f64()
                )?;
                log.flush()?;
                eprintln!(
                    "seed={seed} generation={generation}/{generations} train={mean_score:.6} validation={held:.6} best={:.6} elapsed={:.1}s",
                    best.1,
                    clock.elapsed().as_secs_f64()
                );
            }
        }
        checkpoint(
            &root.join(format!("seed-{seed}-best.rs")),
            &best.0,
            seed,
            best.2,
            best.1,
        )?;
        if best.1 > global.1 {
            global = (best.0, best.1, seed, best.2);
        }
    }
    checkpoint(
        &root.join("selected.rs"),
        &global.0,
        global.2,
        global.3,
        global.1,
    )?;
    let final_test = fixtures(30000, 96);
    let mut final_log = fs::File::create(root.join("final.csv"))?;
    writeln!(
        final_log,
        "fixture,controller,forward,turn,forward_error,turn_error,progress_m,turn_rad"
    )?;
    for fixture in &final_test {
        for kind in LabControllerKind::ALL {
            let weights = (kind == LabControllerKind::StandaloneNeural).then_some(&global.0);
            let (f, t, progress, turn) = episode(weights, kind, fixture);
            writeln!(
                final_log,
                "{},{kind:?},{},{},{f},{t},{progress},{turn}",
                fixture.seed, fixture.intent.forward, fixture.intent.turn
            )?;
        }
    }
    fs::write(
        root.join("metadata.txt"),
        format!(
            "selected_seed={}\nselected_generation={}\nvalidation_score={}\ngenerations_per_seed={generations}\nparameters={WEIGHTS}\ntrainable_parameters={WEIGHTS}\ntraining_ticks={}\nwall_seconds={}\nsigma={SIGMA}\nlearning_rate={LEARNING_RATE}\nworkers={WORKERS}\n",
            global.2,
            global.3,
            global.1,
            generations * 3 * PAIRS * 2 * BATCH * TICKS,
            clock.elapsed().as_secs_f64()
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_intent_category_covers_both_initial_sail_sides() {
        let bank = fixtures(10000, 32);
        for category in 0..4 {
            for sign in [-1.0, 1.0] {
                assert!(
                    bank.iter().any(|fixture| fixture.seed % 4 == category
                        && fixture.initial.sail_angle_rad.signum() == sign),
                    "intent category {category} lacks initial sail side {sign}"
                );
            }
        }
    }
}
