import math
from collections.abc import Callable
from dataclasses import dataclass

import pymunk

from . import boat
from .controllers import actuator, arcade
from .physics import coupled, curves


COMPUTED_YAW_RATE_ARCADE_CONTROL = "computed yaw-rate arcade"
AUTO_SHEET_CONTROL = "auto-sheet + rudder"
YAW_RATE_ARCADE_CONTROL = "yaw-rate arcade"


@dataclass(frozen=True)
class SimulationSettings:
	parameters: boat.BoatParameters
	initial_boat_angle_rad: float
	control_angle_resolution_deg: int
	sailor_max_sheet_force_n: float
	sailor_max_tiller_force_n: float
	fixed_maximum_yaw_rate_rad_s: float


@dataclass(frozen=True)
class SimulationTelemetry:
	physics: boat.PhysicsTelemetry
	control_mode: str
	surface_targets: arcade.SurfaceTargets
	maximum_yaw_rate_rad_s: float | None


@dataclass(frozen=True)
class SurfaceControlState:
	sail: actuator.ActuatorState
	rudder: actuator.ActuatorState
	surfaces: arcade.ControlInput
	maximum_yaw_rate_rad_s: float


def _foil_effect(
	boat_angle_rad: float,
	apparent_flow_m_s: pymunk.Vec2d,
	hinge_position_local_m: tuple[float, float],
	center_distance_m: float,
	angle_rad: float,
	fluid_density_kg_m3: float,
	area_m2: float,
	boat_angular_velocity_rad_s: float,
	base_drag_coefficient: float,
	lift_coefficient_at: curves.LiftCurve,
	drag_coefficient_at: curves.DragCurve,
) -> arcade.SurfaceEffect:
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
		base_drag_coefficient,
		lift_coefficient_at,
		drag_coefficient_at,
	)
	boat_forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)
	return arcade.SurfaceEffect(
		angle_rad,
		force_n.dot(boat_forward),
		center_world_m.cross(force_n),
	)


def _sample_effects(
	effect_at: Callable[[float], arcade.SurfaceEffect],
	maximum_angle_rad: float,
	angle_resolution_deg: int,
) -> tuple[arcade.SurfaceEffect, ...]:
	maximum_angle_deg = round(math.degrees(maximum_angle_rad))
	return tuple(
		effect_at(math.radians(angle_deg))
		for angle_deg in range(
			-maximum_angle_deg,
			maximum_angle_deg + 1,
			angle_resolution_deg,
		)
	)


def surface_control_state(
	observation: boat.BoatObservation,
	settings: SimulationSettings,
) -> SurfaceControlState:
	parameters = settings.parameters

	def sail_effect_at(angle_rad: float) -> arcade.SurfaceEffect:
		return _foil_effect(
			observation.boat_angle_rad,
			observation.apparent_wind_m_s,
			parameters.mast_position_m,
			parameters.sail_center_of_effort_from_mast_m,
			angle_rad,
			parameters.air_density_kg_m3,
			parameters.sail_area_m2,
			observation.boat_angular_velocity_rad_s,
			parameters.sail_base_drag_coefficient,
			parameters.sail_lift_coefficient,
			parameters.sail_drag_coefficient,
		)

	def rudder_effect_at(angle_rad: float) -> arcade.SurfaceEffect:
		return _foil_effect(
			observation.boat_angle_rad,
			observation.apparent_water_m_s,
			parameters.rudder_position_m,
			parameters.rudder_draw_length_m / 2,
			angle_rad,
			parameters.water_density_kg_m3,
			parameters.rudder_area_m2,
			observation.boat_angular_velocity_rad_s,
			parameters.underwater_foil_base_drag_coefficient,
			parameters.underwater_foil_lift_coefficient,
			parameters.underwater_foil_drag_coefficient,
		)

	sail = actuator.ActuatorState(
		observation.sail_angle_rad,
		observation.sail_angular_velocity_rad_s,
		boat.sail_moment_of_inertia_kg_m2(parameters),
		parameters.sail_bearing_damping_n_m_s_rad,
		settings.sailor_max_sheet_force_n * parameters.sail_sheet_lever_arm_m,
		parameters.max_sail_angle_rad,
	)
	rudder = actuator.ActuatorState(
		observation.rudder_angle_rad,
		observation.rudder_angular_velocity_rad_s,
		boat.rudder_moment_of_inertia_kg_m2(parameters),
		parameters.rudder_bearing_damping_n_m_s_rad,
		settings.sailor_max_tiller_force_n * parameters.tiller_length_m,
		parameters.max_rudder_angle_rad,
	)
	rudder_position_world_m = pymunk.Vec2d(*parameters.rudder_position_m).rotated(
		observation.boat_angle_rad
	)
	rudder_apparent_water_m_s = (
		observation.apparent_water_m_s
		- rudder_position_world_m.perpendicular()
		* observation.boat_angular_velocity_rad_s
	)
	steering_baseline_m = (
		pymunk.Vec2d(*parameters.rudder_position_m)
		- pymunk.Vec2d(*parameters.keel_position_m)
	).length
	maximum_yaw_rate_rad_s = (
		rudder_apparent_water_m_s.length
		* math.tan(parameters.max_rudder_angle_rad)
		/ steering_baseline_m
	)
	return SurfaceControlState(
		sail,
		rudder,
		arcade.ControlInput(
			observation.boat_angular_velocity_rad_s,
			boat.free_sail_angle(
				observation.boat_angle_rad,
				observation.apparent_wind_m_s,
				parameters.max_sail_angle_rad,
			),
			parameters.max_rudder_angle_rad,
			_sample_effects(
				sail_effect_at,
				parameters.max_sail_angle_rad,
				settings.control_angle_resolution_deg,
			),
			_sample_effects(
				rudder_effect_at,
				parameters.max_rudder_angle_rad,
				settings.control_angle_resolution_deg,
			),
		),
		maximum_yaw_rate_rad_s,
	)


def create_simulation(settings: SimulationSettings) -> boat.SailingState:
	return coupled.create_simulation(
		settings.parameters,
		settings.initial_boat_angle_rad,
	)


def target_control_torques(
	control_state: SurfaceControlState,
	target_sail_angle_rad: float,
	target_rudder_angle_rad: float,
) -> boat.ActuatorTorques:
	return boat.ActuatorTorques(
		actuator.damped_control_torque(control_state.sail, target_sail_angle_rad),
		actuator.damped_control_torque(control_state.rudder, target_rudder_angle_rad),
	)


def sheet_control_torques(
	control_state: SurfaceControlState,
	target_sail_angle_rad: float,
	target_rudder_angle_rad: float,
) -> boat.ActuatorTorques:
	torques = target_control_torques(
		control_state,
		target_sail_angle_rad,
		target_rudder_angle_rad,
	)
	outward_direction = control_state.sail.angle_rad or target_sail_angle_rad
	if torques.sail_n_m * outward_direction > 0:
		return boat.ActuatorTorques(0, torques.rudder_n_m)
	return torques


def advance_surface_game(
	state: boat.SailingState,
	intent: arcade.AutoSheetIntent | arcade.MotionIntent,
	control_mode: str,
	settings: SimulationSettings,
	environment: boat.Environment,
	dt_seconds: float,
) -> SimulationTelemetry:
	observation = boat.observe(state, environment, settings.parameters)
	control_state = surface_control_state(observation, settings)
	surface_input = control_state.surfaces
	if control_mode == AUTO_SHEET_CONTROL:
		targets = arcade.auto_sheet_control(surface_input, intent)
		maximum_yaw_rate_rad_s = None
	elif control_mode == COMPUTED_YAW_RATE_ARCADE_CONTROL:
		maximum_yaw_rate_rad_s = control_state.maximum_yaw_rate_rad_s
		targets = arcade.yaw_rate_move_control(
			surface_input,
			intent,
			maximum_yaw_rate_rad_s,
		)
	else:
		assert control_mode == YAW_RATE_ARCADE_CONTROL
		maximum_yaw_rate_rad_s = settings.fixed_maximum_yaw_rate_rad_s
		targets = arcade.yaw_rate_move_control(
			surface_input,
			intent,
			maximum_yaw_rate_rad_s,
		)

	uses_sheet_limit = targets.sail_target_angle_rad is None
	if uses_sheet_limit:
		sail_target_angle_rad = max(
			-targets.sheet_limit_angle_rad,
			min(targets.sheet_limit_angle_rad, surface_input.free_sail_angle_rad),
		)
	else:
		sail_target_angle_rad = targets.sail_target_angle_rad
	control_torques = (
		sheet_control_torques if uses_sheet_limit else target_control_torques
	)
	torques = control_torques(
		control_state,
		sail_target_angle_rad,
		targets.rudder_target_angle_rad,
	)
	physics = coupled.step(
		state,
		environment,
		settings.parameters,
		torques,
		dt_seconds,
	)
	return SimulationTelemetry(
		physics,
		control_mode,
		targets,
		maximum_yaw_rate_rad_s,
	)
