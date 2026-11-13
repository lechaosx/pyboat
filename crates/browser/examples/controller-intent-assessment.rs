//! cargo run --release -p browser --features playground --example controller-intent-assessment -- OUTPUT_DIR [training|heldout|all]
#[path = "controller_intent_assessment/metrics.rs"]
mod metrics;
#[path = "controller_intent_assessment/suite.rs"]
mod suite;

use browser::controller_lab::{LabController, LabControllerKind};
use metrics::Outcomes;
use rapier2d_f64::prelude::Vector;
use simulation::{BoatMotion, DT_SECONDS, SharedSimulation};
use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
    path::Path,
    time::Instant,
};

fn body_speed(motion: BoatMotion, current: Vector) -> f64 {
    (motion.velocity_m_s - current).dot(Vector::new(motion.angle_rad.cos(), motion.angle_rad.sin()))
}
fn optional(value: Option<f64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

fn main() -> io::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|a| a == "--help") {
        println!(
            "Usage: controller-intent-assessment OUTPUT_DIR [training|heldout|all]\nWrites fixtures.csv, traces.csv and outcomes.csv. Each controller has an independent boat with identical initial state and prescribed flows. Three continuous four-second segments retain controller history. equal_state_release starts from the same imposed turning state with fresh controller memory. Signed progress follows the boat's changing longitudinal axis relative to water; world displacement is reported separately. Turning opposed to input includes physical inertia and is not proof of controller disobedience. Runtime is native single-boat wall time, excluding trace output; compare repeated release runs on an idle machine. Assessment-heldout seeds are excluded from training; reserve fresh final-test seeds for future candidate selection. No aggregate winner score."
        );
        return Ok(());
    }
    let output = args
        .first()
        .map(String::as_str)
        .unwrap_or(".parked-foil-work/controller-intent-assessment");
    let split = args.get(1).map(String::as_str).unwrap_or("all");
    if !["training", "heldout", "all"].contains(&split) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "split must be training, heldout or all",
        ));
    }
    fs::create_dir_all(output)?;
    let file = |name| File::create(Path::new(output).join(name)).map(BufWriter::new);
    let mut fixtures = file("fixtures.csv")?;
    let mut traces = file("traces.csv")?;
    let mut summaries = file("outcomes.csv")?;
    writeln!(
        fixtures,
        "case,seed,split,segment,ticks,dt_s,forward,turn,case_initial_x_m,case_initial_y_m,case_initial_vx_m_s,case_initial_vy_m_s,case_initial_heading_rad,case_initial_yaw_rad_s,case_initial_sail_rad,case_initial_sail_rate_rad_s,case_initial_rudder_rad,case_initial_rudder_rate_rad_s,base_wind_x_m_s,base_wind_y_m_s,base_current_x_m_s,base_current_y_m_s,changing_flow"
    )?;
    writeln!(
        traces,
        "case,seed,split,controller,segment,tick,forward,turn,wind_x_m_s,wind_y_m_s,current_x_m_s,current_y_m_s,x_m,y_m,heading_rad,vx_m_s,vy_m_s,yaw_rad_s,sail_rad,sail_rate_rad_s,rudder_rad,rudder_rate_rad_s,sheet_target_rad,sail_target_rad,rudder_target_rad"
    )?;
    writeln!(
        summaries,
        "case,seed,split,controller,segment,forward,turn,start_x_m,start_y_m,start_heading_rad,start_yaw_rad_s,end_yaw_rad_s,world_dx_m,world_dy_m,body_water_progress_m,intent_signed_progress_m,opposite_progress_m,body_water_lateral_m,absolute_lateral_m,turn_rad,intent_signed_turn_rad,wrong_way_turn_rad,neutral_absolute_turn_rad,first_correct_yaw_sign_sample_s,start_forward_m_s,end_forward_m_s,start_water_speed_m_s,end_water_speed_m_s,water_speed_ratio,sail_travel_rad,rudder_travel_rad,controller_us_per_tick,physics_us_per_tick"
    )?;
    for case in suite::cases()
        .into_iter()
        .filter(|case| split == "all" || split == case.split)
    {
        let initial = case.initial;
        for (segment, intent) in case.intents.iter().enumerate() {
            writeln!(
                fixtures,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                case.name,
                case.seed,
                case.split,
                segment,
                suite::SEGMENT_TICKS,
                DT_SECONDS,
                intent.forward,
                intent.turn,
                initial.position_m.x,
                initial.position_m.y,
                initial.velocity_m_s.x,
                initial.velocity_m_s.y,
                initial.angle_rad,
                initial.angular_velocity_rad_s,
                initial.sail_angle_rad,
                initial.sail_angular_velocity_rad_s,
                initial.rudder_angle_rad,
                initial.rudder_angular_velocity_rad_s,
                case.base.true_wind_velocity_m_s.x,
                case.base.true_wind_velocity_m_s.y,
                case.base.true_water_velocity_m_s.x,
                case.base.true_water_velocity_m_s.y,
                case.changing_flow
            )?;
        }
        for kind in LabControllerKind::ALL {
            let mut simulation = SharedSimulation::new();
            let parameters = game::dinghy_parameters();
            let boat = simulation.add_boat(parameters, initial);
            let mut controller = LabController::new(kind);
            for (segment, intent) in case.intents.iter().copied().enumerate() {
                let start = simulation.motion(boat);
                let first_tick = segment * suite::SEGMENT_TICKS;
                let start_current = case.environment(first_tick).true_water_velocity_m_s;
                let mut outcomes = Outcomes::default();
                let mut controller_seconds = 0.0;
                let mut physics_seconds = 0.0;
                let mut travel = [0.0; 2];
                for offset in 0..suite::SEGMENT_TICKS {
                    let tick = first_tick + offset;
                    let environment = case.environment(tick);
                    let before = simulation.motion(boat);
                    let clock = Instant::now();
                    let targets =
                        controller.targets(&simulation, boat, parameters, environment, intent);
                    controller_seconds += clock.elapsed().as_secs_f64();
                    let clock = Instant::now();
                    simulation
                        .advance_controlled_with_environments(&[environment], &[Some(targets)]);
                    physics_seconds += clock.elapsed().as_secs_f64();
                    let after = simulation.motion(boat);
                    outcomes.observe(
                        before,
                        after,
                        environment.true_water_velocity_m_s,
                        intent,
                        DT_SECONDS,
                    );
                    travel[0] += (after.sail_angle_rad - before.sail_angle_rad).abs();
                    travel[1] += (after.rudder_angle_rad - before.rudder_angle_rad).abs();
                    writeln!(
                        traces,
                        "{},{},{},{:?},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                        case.name,
                        case.seed,
                        case.split,
                        kind,
                        segment,
                        tick,
                        intent.forward,
                        intent.turn,
                        environment.true_wind_velocity_m_s.x,
                        environment.true_wind_velocity_m_s.y,
                        environment.true_water_velocity_m_s.x,
                        environment.true_water_velocity_m_s.y,
                        after.position_m.x,
                        after.position_m.y,
                        after.angle_rad,
                        after.velocity_m_s.x,
                        after.velocity_m_s.y,
                        after.angular_velocity_rad_s,
                        after.sail_angle_rad,
                        after.sail_angular_velocity_rad_s,
                        after.rudder_angle_rad,
                        after.rudder_angular_velocity_rad_s,
                        targets.sheet_target_angle_rad,
                        optional(targets.sail_target_angle_rad),
                        targets.rudder_target_angle_rad
                    )?;
                }
                let end = simulation.motion(boat);
                let end_current = case
                    .environment(first_tick + suite::SEGMENT_TICKS - 1)
                    .true_water_velocity_m_s;
                let start_speed = (start.velocity_m_s - start_current).length();
                let end_speed = (end.velocity_m_s - end_current).length();
                let displacement = end.position_m - start.position_m;
                writeln!(
                    summaries,
                    "{},{},{},{:?},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    case.name,
                    case.seed,
                    case.split,
                    kind,
                    segment,
                    intent.forward,
                    intent.turn,
                    start.position_m.x,
                    start.position_m.y,
                    start.angle_rad,
                    start.angular_velocity_rad_s,
                    end.angular_velocity_rad_s,
                    displacement.x,
                    displacement.y,
                    outcomes.forward_m,
                    optional(
                        (intent.forward != 0.0)
                            .then_some(intent.forward.signum() * outcomes.forward_m)
                    ),
                    outcomes.opposite_progress_m,
                    outcomes.lateral_m,
                    outcomes.absolute_lateral_m,
                    outcomes.turn_rad,
                    optional((intent.turn != 0.0).then_some(outcomes.signed_turn_rad)),
                    outcomes.wrong_way_turn_rad,
                    outcomes.neutral_turn_rad,
                    optional(outcomes.first_signed_response_s),
                    body_speed(start, start_current),
                    body_speed(end, end_current),
                    start_speed,
                    end_speed,
                    optional((start_speed > 0.0).then_some(end_speed / start_speed)),
                    travel[0],
                    travel[1],
                    controller_seconds * 1e6 / suite::SEGMENT_TICKS as f64,
                    physics_seconds * 1e6 / suite::SEGMENT_TICKS as f64
                )?;
            }
        }
        eprintln!("{} seed {} ({})", case.name, case.seed, case.split);
    }
    Ok(())
}
