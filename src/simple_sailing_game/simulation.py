import math
from collections.abc import Callable
from dataclasses import dataclass, replace

import pymunk

from . import boat
from .controllers import arcade, assisted as controller, improved_arcade, manual
from .physics import coupled as articulated_model
from .physics import dynamic as lumped_model


DT_SECONDS = 1 / 60
INITIAL_BOAT_ANGLE_RAD = math.radians(135)
CONTROL_ANGLE_RESOLUTION_DEG = 1
SAILOR_MAX_SHEET_FORCE_N = 250
SAILOR_MAX_TILLER_FORCE_N = 60
MAXIMUM_YAW_RATE_RAD_S = math.radians(35)

GUIDED_GRADIENT_CONTROL = "gradient with helper"
GLOBAL_OPTIMUM_CONTROL = "global optimum"
RAW_GRADIENT_CONTROL = "gradient without helper"
MANUAL_CONTROL = "manual"
AUTO_SHEET_CONTROL = "auto sheet"
ARCADE_CONTROL = "arcade"
ARCADE_STERNWAY_CONTROL = "arcade sternway"
ARCADE_MOVE_CONTROL = "arcade move intent"
IMPROVED_ARCADE_CONTROL = "improved arcade move intent"
LUMPED_PHYSICS = "lumped"
ARTICULATED_PHYSICS = "articulated"

BOAT_PARAMETERS = boat.BoatParameters(
	air_density_kg_m3=1.225,
	water_density_kg_m3=1025,
	hull_mass_kg=222,
	length_m=4,
	beam_m=1.5,
	bow_length_m=0.7,
	sail_area_m2=8,
	boom_length_m=2,
	boom_mass_kg=10,
	mast_position_m=(0.4, 0),
	sail_center_of_effort_from_mast_m=0.7,
	sail_mass_kg=12,
	keel_area_m2=0.6,
	keel_position_m=(0, 0),
	rudder_area_m2=0.12,
	rudder_draw_length_m=0.5,
	rudder_mass_kg=4,
	tiller_length_m=1,
	tiller_mass_kg=2,
	rudder_position_m=(-1.8, 0),
	hull_reference_area_m2=0.3,
	hull_drag_coefficient=0.08,
	hull_lateral_drag_coefficient=0.02,
	sail_base_drag_coefficient=0.02,
	underwater_foil_base_drag_coefficient=0.02,
	sail_bearing_damping_n_m_s_rad=20,
	rudder_bearing_damping_n_m_s_rad=2,
	sail_sheet_lever_arm_m=1.5,
	max_sail_angle_rad=math.radians(90),
	max_rudder_angle_rad=math.radians(30),
)
ENVIRONMENT = boat.Environment(
	true_wind_velocity_m_s=pymunk.Vec2d(6, 0),
	true_water_velocity_m_s=pymunk.Vec2d(0, 0),
)
CONTROLLER_SETTINGS = controller.ControllerSettings(
	mode=controller.DIRECT_GRADIENT_CONTROL,
	use_global_slope=True,
	angle_resolution_deg=CONTROL_ANGLE_RESOLUTION_DEG,
)
ASSISTED_CONTROL_SETTINGS = {
	GUIDED_GRADIENT_CONTROL: CONTROLLER_SETTINGS,
	GLOBAL_OPTIMUM_CONTROL: replace(
		CONTROLLER_SETTINGS,
		mode=controller.OPTIMIZED_TARGET_CONTROL,
		use_global_slope=False,
	),
	RAW_GRADIENT_CONTROL: replace(
		CONTROLLER_SETTINGS,
		use_global_slope=False,
	),
}


@dataclass(frozen=True)
class SimulationTelemetry:
	physics: boat.PhysicsTelemetry
	decision: controller.ControlDecision | None
	controller_active: bool
	control_mode: str | None = None
	physics_mode: str = LUMPED_PHYSICS
	surface_targets: arcade.SurfaceTargets | None = None


ControllerComponent = Callable[
	[controller.ControlInput, controller.MotionIntent, float],
	controller.ControlOutput,
]


def foil_control_effect(
	boat_angle_rad: float,
	apparent_flow_m_s: pymunk.Vec2d,
	hinge_position_local_m: tuple[float, float],
	center_distance_m: float,
	angle_rad: float,
	fluid_density_kg_m3: float,
	area_m2: float,
	boat_angular_velocity_rad_s: float = 0,
	base_drag_coefficient: float | None = None,
) -> controller.ControlEffect:
	center_local_m = (
		pymunk.Vec2d(*hinge_position_local_m)
		+ pymunk.Vec2d(-center_distance_m, 0).rotated(angle_rad)
	)
	center_world_m = center_local_m.rotated(boat_angle_rad)
	center_apparent_flow_m_s = (
		apparent_flow_m_s
		- center_world_m.perpendicular() * boat_angular_velocity_rad_s
	)
	force_n = boat.force_on_foil(
		center_apparent_flow_m_s,
		boat_angle_rad + angle_rad,
		fluid_density_kg_m3,
		area_m2,
		(
			BOAT_PARAMETERS.underwater_foil_base_drag_coefficient
			if base_drag_coefficient is None
			else base_drag_coefficient
		),
	)
	boat_forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)
	return controller.ControlEffect(
		angle_rad,
		center_world_m.cross(force_n),
		force_n.dot(boat_forward),
		force_n.length,
	)


def sample_control_effects(
	effect_at: Callable[[float], controller.ControlEffect],
	maximum_angle_rad: float,
) -> tuple[controller.ControlEffect, ...]:
	maximum_angle_deg = round(math.degrees(maximum_angle_rad))
	return tuple(
		effect_at(math.radians(angle_deg))
		for angle_deg in range(
			-maximum_angle_deg,
			maximum_angle_deg + 1,
			CONTROL_ANGLE_RESOLUTION_DEG,
		)
	)


def controller_input(observation: boat.BoatObservation) -> controller.ControlInput:
	def sail_effect_at(angle_rad: float) -> controller.ControlEffect:
		return foil_control_effect(
			observation.boat_angle_rad,
			observation.apparent_wind_m_s,
			BOAT_PARAMETERS.mast_position_m,
			BOAT_PARAMETERS.sail_center_of_effort_from_mast_m,
			angle_rad,
			BOAT_PARAMETERS.air_density_kg_m3,
			BOAT_PARAMETERS.sail_area_m2,
			observation.boat_angular_velocity_rad_s,
			BOAT_PARAMETERS.sail_base_drag_coefficient,
		)

	def rudder_effect_at(angle_rad: float) -> controller.ControlEffect:
		return foil_control_effect(
			observation.boat_angle_rad,
			observation.apparent_water_m_s,
			BOAT_PARAMETERS.rudder_position_m,
			BOAT_PARAMETERS.rudder_draw_length_m / 2,
			angle_rad,
			BOAT_PARAMETERS.water_density_kg_m3,
			BOAT_PARAMETERS.rudder_area_m2,
			observation.boat_angular_velocity_rad_s,
			BOAT_PARAMETERS.underwater_foil_base_drag_coefficient,
		)

	return controller.ControlInput(
		observation.boat_angular_velocity_rad_s,
		boat.lumped_moment_of_inertia_kg_m2(BOAT_PARAMETERS),
		controller.ActuatorState(
			observation.sail_angle_rad,
			observation.sail_angular_velocity_rad_s,
			observation.sail_load.torque_n_m,
			boat.sail_moment_of_inertia_kg_m2(BOAT_PARAMETERS),
			BOAT_PARAMETERS.sail_bearing_damping_n_m_s_rad,
			SAILOR_MAX_SHEET_FORCE_N * BOAT_PARAMETERS.sail_sheet_lever_arm_m,
			BOAT_PARAMETERS.max_sail_angle_rad,
		),
		controller.ActuatorState(
			observation.rudder_angle_rad,
			observation.rudder_angular_velocity_rad_s,
			observation.rudder_load.torque_n_m,
			boat.rudder_moment_of_inertia_kg_m2(BOAT_PARAMETERS),
			BOAT_PARAMETERS.rudder_bearing_damping_n_m_s_rad,
			SAILOR_MAX_TILLER_FORCE_N * BOAT_PARAMETERS.tiller_length_m,
			BOAT_PARAMETERS.max_rudder_angle_rad,
		),
		sample_control_effects(sail_effect_at, BOAT_PARAMETERS.max_sail_angle_rad),
		sample_control_effects(rudder_effect_at, BOAT_PARAMETERS.max_rudder_angle_rad),
		sail_effect_at,
		rudder_effect_at,
		boat.luffing_sail_angle(
			observation.boat_angle_rad,
			observation.apparent_wind_m_s,
		),
	)


def surface_controller_input(
	observation: boat.BoatObservation,
	actuator_input: controller.ControlInput | None = None,
) -> arcade.ControlInput:
	if actuator_input is None:
		actuator_input = controller_input(observation)
	return arcade.ControlInput(
		observation.boat_angle_rad,
		observation.boat_angular_velocity_rad_s,
		boat.free_sail_angle(
			observation.boat_angle_rad,
			observation.apparent_wind_m_s,
			BOAT_PARAMETERS.max_sail_angle_rad,
		),
		BOAT_PARAMETERS.max_sail_angle_rad,
		BOAT_PARAMETERS.max_rudder_angle_rad,
		tuple(
			arcade.SurfaceEffect(
				effect.angle_rad,
				effect.drive_force_n,
				effect.yaw_moment_n_m,
			)
			for effect in actuator_input.sail_candidates
		),
		tuple(
			arcade.SurfaceEffect(
				effect.angle_rad,
				effect.drive_force_n,
				effect.yaw_moment_n_m,
			)
			for effect in actuator_input.rudder_candidates
		),
	)


def create_simulation() -> boat.SailingState:
	return boat.create_simulation(BOAT_PARAMETERS, INITIAL_BOAT_ANGLE_RAD)


def configured_assisted_controller(
	settings: controller.ControllerSettings,
) -> ControllerComponent:
	def run_controller(
		control_input: controller.ControlInput,
		intent: controller.MotionIntent,
		dt_seconds: float,
	) -> controller.ControlOutput:
		return controller.control(control_input, intent, dt_seconds, settings)

	return run_controller


def configured_manual_controller(
	manual_input: manual.DirectionInput,
) -> ControllerComponent:
	def run_controller(
		control_input: controller.ControlInput,
		_intent: controller.MotionIntent,
		_dt_seconds: float,
	) -> controller.ControlOutput:
		torques = manual.control(
			control_input.sail,
			control_input.rudder,
			manual_input,
		)
		return controller.ControlOutput(torques, None)

	return run_controller


def _target_control_torque(
	actuator: controller.ActuatorState,
	target_angle_rad: float,
	physics_mode: str,
	dt_seconds: float,
) -> float:
	if physics_mode == ARTICULATED_PHYSICS:
		return controller.damped_actuator_control_torque(actuator, target_angle_rad)
	return controller.actuator_control_torque(actuator, target_angle_rad, dt_seconds)


def target_control_torques(
	actuator_input: controller.ControlInput,
	target_sail_angle_rad: float,
	target_rudder_angle_rad: float,
	physics_mode: str,
	dt_seconds: float,
) -> boat.ActuatorTorques:
	return boat.ActuatorTorques(
		_target_control_torque(
			actuator_input.sail,
			target_sail_angle_rad,
			physics_mode,
			dt_seconds,
		),
		_target_control_torque(
			actuator_input.rudder,
			target_rudder_angle_rad,
			physics_mode,
			dt_seconds,
		),
	)


def _step_physics(
	state: boat.SailingState,
	environment: boat.Environment,
	torques: boat.ActuatorTorques,
	dt_seconds: float,
	physics_mode: str,
) -> boat.PhysicsTelemetry:
	if physics_mode == ARTICULATED_PHYSICS:
		return articulated_model.step(
			state, environment, BOAT_PARAMETERS, torques, dt_seconds
		)
	articulated_model.deactivate(state, BOAT_PARAMETERS)
	return lumped_model.step(
		state, environment, BOAT_PARAMETERS, torques, dt_seconds
	)


def advance_game(
	state: boat.SailingState,
	intent: controller.MotionIntent,
	controller_component: ControllerComponent | None,
	environment: boat.Environment = ENVIRONMENT,
	dt_seconds: float = DT_SECONDS,
	physics_mode: str = LUMPED_PHYSICS,
) -> SimulationTelemetry:
	observation = boat.observe(state, environment, BOAT_PARAMETERS)
	control_output = None
	actuator_input = None
	if controller_component is not None:
		actuator_input = controller_input(observation)
		control_output = controller_component(actuator_input, intent, dt_seconds)
	torques = boat.ActuatorTorques()
	if control_output is not None:
		torques = boat.ActuatorTorques(
			control_output.torques.sail_n_m,
			control_output.torques.rudder_n_m,
		)
	if (
		actuator_input is not None
		and control_output is not None
		and control_output.decision is not None
		and control_output.decision.target_angles_rad is not None
	):
		torques = target_control_torques(
			actuator_input,
			*control_output.decision.target_angles_rad,
			physics_mode,
			dt_seconds,
		)
	physics = _step_physics(
		state, environment, torques, dt_seconds, physics_mode
	)
	decision = control_output.decision if control_output is not None else None
	return SimulationTelemetry(
		physics=physics,
		decision=decision,
		controller_active=controller_component is not None,
		physics_mode=physics_mode,
	)


def advance_surface_game(
	state: boat.SailingState,
	intent: arcade.AutoSheetIntent | arcade.HeadingIntent | controller.MotionIntent,
	control_mode: str,
	physics_mode: str,
	environment: boat.Environment = ENVIRONMENT,
	dt_seconds: float = DT_SECONDS,
) -> SimulationTelemetry:
	observation = boat.observe(state, environment, BOAT_PARAMETERS)
	actuator_input = controller_input(observation)
	surface_input = surface_controller_input(observation, actuator_input)
	if control_mode == AUTO_SHEET_CONTROL:
		targets = arcade.auto_sheet_manual_tiller(surface_input, intent)
	elif control_mode == ARCADE_CONTROL:
		targets = arcade.heading_control(
			surface_input, intent, MAXIMUM_YAW_RATE_RAD_S
		)
	elif control_mode == ARCADE_STERNWAY_CONTROL:
		targets = arcade.sternway_control(
			surface_input, intent, MAXIMUM_YAW_RATE_RAD_S
		)
	elif control_mode == IMPROVED_ARCADE_CONTROL:
		targets = improved_arcade.control(
			surface_input,
			intent,
			observation.apparent_water_m_s.length,
			MAXIMUM_YAW_RATE_RAD_S,
		)
	else:
		targets = arcade.move_control(
			surface_input, intent, MAXIMUM_YAW_RATE_RAD_S
		)

	if targets.sail_target_angle_rad is None:
		sail_target_angle_rad = max(
			-targets.sheet_limit_angle_rad,
			min(targets.sheet_limit_angle_rad, surface_input.free_sail_angle_rad),
		)
	else:
		sail_target_angle_rad = targets.sail_target_angle_rad
	torques = target_control_torques(
		actuator_input,
		sail_target_angle_rad,
		targets.rudder_target_angle_rad,
		physics_mode,
		dt_seconds,
	)
	physics = _step_physics(
		state, environment, torques, dt_seconds, physics_mode
	)
	return SimulationTelemetry(
		physics=physics,
		decision=None,
		controller_active=True,
		physics_mode=physics_mode,
		control_mode=control_mode,
		surface_targets=targets,
	)


def step(
	state: boat.SailingState,
	environment: boat.Environment,
	intent: controller.MotionIntent,
	dt_seconds: float = DT_SECONDS,
	control_mode: str = controller.OPTIMIZED_TARGET_CONTROL,
	use_global_slope: bool = True,
) -> SimulationTelemetry:
	settings = replace(
		CONTROLLER_SETTINGS,
		mode=control_mode,
		use_global_slope=use_global_slope,
	)
	return advance_game(
		state,
		intent,
		configured_assisted_controller(settings),
		environment,
		dt_seconds,
	)
