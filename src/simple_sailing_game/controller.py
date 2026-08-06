import math
from collections.abc import Callable
from dataclasses import dataclass

import numpy as np


OPTIMIZED_TARGET_CONTROL = "optimized target"
DIRECT_GRADIENT_CONTROL = "direct gradient"


@dataclass(frozen=True)
class MotionIntent:
	forward: float = 0
	turn: float = 0


@dataclass(frozen=True)
class ControlEffect:
	angle_rad: float
	yaw_moment_n_m: float
	drive_force_n: float
	force_magnitude_n: float


@dataclass(frozen=True)
class ActuatorState:
	angle_rad: float
	angular_velocity_rad_s: float
	external_torque_n_m: float
	moment_of_inertia_kg_m2: float
	bearing_damping_n_m_s_rad: float
	maximum_torque_n_m: float
	maximum_angle_rad: float


@dataclass(frozen=True)
class ControlInput:
	boat_angular_velocity_rad_s: float
	boat_moment_of_inertia_kg_m2: float
	sail: ActuatorState
	rudder: ActuatorState
	sail_candidates: tuple[ControlEffect, ...]
	rudder_candidates: tuple[ControlEffect, ...]
	sail_effect_at: Callable[[float], ControlEffect]
	rudder_effect_at: Callable[[float], ControlEffect]
	neutral_sail_angle_rad: float


@dataclass(frozen=True)
class ControllerSettings:
	mode: str
	use_global_slope: bool
	angle_resolution_deg: float


@dataclass(frozen=True)
class ControlLandscape:
	errors: np.ndarray
	gradient: tuple[float, float]
	curvature: tuple[float, float]
	optimum_sail_angle_rad: float
	optimum_rudder_angle_rad: float


@dataclass(frozen=True)
class ControlDecision:
	target_angles_rad: tuple[float, float] | None
	landscape: ControlLandscape


@dataclass(frozen=True)
class ActuatorTorques:
	sail_n_m: float = 0
	rudder_n_m: float = 0


@dataclass(frozen=True)
class ControlOutput:
	torques: ActuatorTorques
	decision: ControlDecision | None


def actuator_control_torque(
	actuator: ActuatorState,
	target_angle_rad: float,
	dt_seconds: float,
) -> float:
	angle_error_rad = target_angle_rad - actuator.angle_rad
	maximum_acceleration_rad_s2 = (
		actuator.maximum_torque_n_m / actuator.moment_of_inertia_kg_m2
	)
	target_speed_rad_s = math.copysign(
		min(
			math.sqrt(2 * maximum_acceleration_rad_s2 * abs(angle_error_rad)),
			abs(angle_error_rad) / dt_seconds,
		),
		angle_error_rad,
	)
	required_acceleration_rad_s2 = (
		target_speed_rad_s - actuator.angular_velocity_rad_s
	) / dt_seconds
	torque_n_m = (
		actuator.moment_of_inertia_kg_m2 * required_acceleration_rad_s2
		- actuator.external_torque_n_m
		+ actuator.angular_velocity_rad_s * actuator.bearing_damping_n_m_s_rad
	)
	return max(
		-actuator.maximum_torque_n_m,
		min(actuator.maximum_torque_n_m, torque_n_m),
	)


def direct_gradient_control_torque(
	actuator: ActuatorState,
	objective_gradient: float,
	objective_curvature: float,
) -> float:
	potential_scale_n_m = actuator.maximum_torque_n_m * actuator.maximum_angle_rad
	gradient_torque_n_m = -potential_scale_n_m * objective_gradient
	stiffness_n_m_rad = max(0, potential_scale_n_m * objective_curvature)
	critical_damping_n_m_s_rad = 2 * math.sqrt(
		stiffness_n_m_rad * actuator.moment_of_inertia_kg_m2
	)
	sailor_damping_n_m_s_rad = max(
		0,
		critical_damping_n_m_s_rad - actuator.bearing_damping_n_m_s_rad,
	)
	torque_n_m = (
		gradient_torque_n_m
		- sailor_damping_n_m_s_rad * actuator.angular_velocity_rad_s
		- actuator.external_torque_n_m
	)
	return max(
		-actuator.maximum_torque_n_m,
		min(actuator.maximum_torque_n_m, torque_n_m),
	)


def normalized_control_effect(
	value: float | np.ndarray,
	negative_limit: float,
	positive_limit: float,
) -> float | np.ndarray:
	if not isinstance(value, np.ndarray):
		if value > 0 and positive_limit:
			return value / positive_limit
		if value < 0 and negative_limit:
			return value / -negative_limit
		return 0

	normalized = np.zeros_like(value)
	if positive_limit:
		normalized[value > 0] = value[value > 0] / positive_limit
	if negative_limit:
		normalized[value < 0] = value[value < 0] / -negative_limit
	return normalized


def control_limits(
	sails: tuple[ControlEffect, ...],
	rudders: tuple[ControlEffect, ...],
) -> tuple[float, float, float, float]:
	sail_drives = [sail.drive_force_n for sail in sails]
	rudder_drives = [rudder.drive_force_n for rudder in rudders]
	sail_moments = [sail.yaw_moment_n_m for sail in sails]
	rudder_moments = [rudder.yaw_moment_n_m for rudder in rudders]
	return (
		min(0, min(sail_drives) + min(rudder_drives)),
		max(0, max(sail_drives) + max(rudder_drives)),
		min(0, min(sail_moments) + min(rudder_moments)),
		max(0, max(sail_moments) + max(rudder_moments)),
	)


def stopping_turn_request(
	angular_velocity_rad_s: float,
	negative_moment_n_m: float,
	positive_moment_n_m: float,
	boat_moment_of_inertia_kg_m2: float,
) -> float:
	if angular_velocity_rad_s > 0:
		yaw_stiffness_n_m_rad = -negative_moment_n_m
	else:
		yaw_stiffness_n_m_rad = positive_moment_n_m
	damping_n_m_s_rad = 2 * math.sqrt(
		yaw_stiffness_n_m_rad * boat_moment_of_inertia_kg_m2
	)
	required_moment_n_m = -angular_velocity_rad_s * damping_n_m_s_rad
	return max(
		-1,
		min(
			1,
			normalized_control_effect(
				required_moment_n_m,
				negative_moment_n_m,
				positive_moment_n_m,
			),
		),
	)


def control_error(
	drive_force_n: float | np.ndarray,
	yaw_moment_n_m: float | np.ndarray,
	desired_drive: float,
	desired_turn: float,
	limits: tuple[float, float, float, float],
) -> float | np.ndarray:
	negative_drive, positive_drive, negative_moment, positive_moment = limits
	drive = normalized_control_effect(
		drive_force_n,
		negative_drive,
		positive_drive,
	)
	moment = normalized_control_effect(
		yaw_moment_n_m,
		negative_moment,
		positive_moment,
	)
	return (drive - desired_drive) ** 2 + (moment - desired_turn) ** 2


def neutral_control_error(
	sail_force_n: float | np.ndarray,
	yaw_moment_n_m: float | np.ndarray,
	desired_turn: float,
	negative_moment_n_m: float,
	positive_moment_n_m: float,
	maximum_sail_force_n: float,
) -> float | np.ndarray:
	load = sail_force_n / maximum_sail_force_n if maximum_sail_force_n else 0
	moment = normalized_control_effect(
		yaw_moment_n_m,
		negative_moment_n_m,
		positive_moment_n_m,
	)
	return load**2 + (moment - desired_turn) ** 2


def effect_grid(
	sails: tuple[ControlEffect, ...],
	rudders: tuple[ControlEffect, ...],
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
	sail_forces = np.array(
		[sail.force_magnitude_n for sail in sails]
	)[:, np.newaxis]
	drives = (
		np.array([sail.drive_force_n for sail in sails])[:, np.newaxis]
		+ np.array([rudder.drive_force_n for rudder in rudders])
	)
	moments = (
		np.array([sail.yaw_moment_n_m for sail in sails])[:, np.newaxis]
		+ np.array([rudder.yaw_moment_n_m for rudder in rudders])
	)
	return sail_forces, drives, moments


def control_objective_derivatives(
	sail_angle_rad: float,
	rudder_angle_rad: float,
	error_at: Callable[[float, float], float],
	maximum_sail_angle_rad: float,
	maximum_rudder_angle_rad: float,
	angle_step_rad: float,
) -> tuple[tuple[float, float], tuple[float, float]]:
	angles_rad = [sail_angle_rad, rudder_angle_rad]
	maximum_angles_rad = [maximum_sail_angle_rad, maximum_rudder_angle_rad]
	current_error = error_at(*angles_rad)
	gradient = []
	curvature = []
	for axis, (angle_rad, maximum_angle_rad) in enumerate(
		zip(angles_rad, maximum_angles_rad)
	):
		lower_rad = max(-maximum_angle_rad, angle_rad - angle_step_rad)
		upper_rad = min(maximum_angle_rad, angle_rad + angle_step_rad)
		lower_angles_rad = angles_rad.copy()
		upper_angles_rad = angles_rad.copy()
		lower_angles_rad[axis] = lower_rad
		upper_angles_rad[axis] = upper_rad
		lower_error = error_at(*lower_angles_rad)
		upper_error = error_at(*upper_angles_rad)
		gradient.append((upper_error - lower_error) / (upper_rad - lower_rad))
		if lower_rad < angle_rad < upper_rad:
			curvature.append(
				(upper_error - 2 * current_error + lower_error) / angle_step_rad**2
			)
		else:
			curvature.append(0)
	return (gradient[0], gradient[1]), (curvature[0], curvature[1])


def select_control_angles(
	sails: tuple[ControlEffect, ...],
	rudders: tuple[ControlEffect, ...],
	desired_turn: float,
	errors: np.ndarray,
	neutral_sail_angle_rad: float,
) -> tuple[float, float]:
	sail_angles = np.array([sail.angle_rad for sail in sails])[:, np.newaxis]
	rudder_angles = np.array([rudder.angle_rad for rudder in rudders])
	angle_travel = np.abs(sail_angles) + np.abs(rudder_angles)
	if desired_turn:
		tie_break = np.broadcast_to(-sail_angles * desired_turn, errors.shape)
	else:
		tie_break = np.broadcast_to(
			np.abs(sail_angles - neutral_sail_angle_rad),
			errors.shape,
		)
	flat_index = np.lexsort(
		(tie_break.ravel(), angle_travel.ravel(), np.round(errors, 12).ravel())
	)[0]
	sail_index, rudder_index = np.unravel_index(flat_index, errors.shape)
	return sails[sail_index].angle_rad, rudders[rudder_index].angle_rad


def control_decision(
	control_input: ControlInput,
	intent: MotionIntent,
	settings: ControllerSettings,
) -> ControlDecision:
	sails = control_input.sail_candidates
	rudders = control_input.rudder_candidates
	limits = control_limits(sails, rudders)
	sail_forces_n, drives_n, moments_n_m = effect_grid(sails, rudders)
	desired_turn = intent.turn or stopping_turn_request(
		control_input.boat_angular_velocity_rad_s,
		limits[2],
		limits[3],
		control_input.boat_moment_of_inertia_kg_m2,
	)
	if not intent.forward and not intent.turn:
		maximum_sail_force_n = max(sail.force_magnitude_n for sail in sails)

		def error_at(sail_angle: float, rudder_angle: float) -> float:
			sail = control_input.sail_effect_at(sail_angle)
			rudder = control_input.rudder_effect_at(rudder_angle)
			return neutral_control_error(
				sail.force_magnitude_n,
				sail.yaw_moment_n_m + rudder.yaw_moment_n_m,
				desired_turn,
				limits[2],
				limits[3],
				maximum_sail_force_n,
			)

		errors = neutral_control_error(
			sail_forces_n,
			moments_n_m,
			desired_turn,
			limits[2],
			limits[3],
			maximum_sail_force_n,
		)
	else:
		def error_at(sail_angle: float, rudder_angle: float) -> float:
			sail = control_input.sail_effect_at(sail_angle)
			rudder = control_input.rudder_effect_at(rudder_angle)
			return control_error(
				sail.drive_force_n + rudder.drive_force_n,
				sail.yaw_moment_n_m + rudder.yaw_moment_n_m,
				intent.forward,
				desired_turn,
				limits,
			)

		errors = control_error(
			drives_n,
			moments_n_m,
			intent.forward,
			desired_turn,
			limits,
		)
	optimal_sail_angle_rad, optimal_rudder_angle_rad = select_control_angles(
		sails,
		rudders,
		desired_turn,
		errors,
		control_input.neutral_sail_angle_rad,
	)
	angle_step_rad = math.radians(settings.angle_resolution_deg)
	if settings.mode == DIRECT_GRADIENT_CONTROL and settings.use_global_slope:
		global_error = error_at(optimal_sail_angle_rad, optimal_rudder_angle_rad)
		regret = max(
			0,
			error_at(control_input.sail.angle_rad, control_input.rudder.angle_rad)
			- global_error,
		)
		if regret:
			def global_slope_penalty(
				sail_angle: float | np.ndarray,
			) -> float | np.ndarray:
				return regret * (
					(sail_angle - optimal_sail_angle_rad)
					/ control_input.sail.maximum_angle_rad
				) ** 4

			base_error_at = error_at

			def error_at(sail_angle: float, rudder_angle: float) -> float:
				return base_error_at(sail_angle, rudder_angle) + float(
					global_slope_penalty(sail_angle)
				)

			sail_angles = np.array(
				[sail.angle_rad for sail in sails]
			)[:, np.newaxis]
			errors = errors + global_slope_penalty(sail_angles)
	gradient, curvature = control_objective_derivatives(
		control_input.sail.angle_rad,
		control_input.rudder.angle_rad,
		error_at,
		control_input.sail.maximum_angle_rad,
		control_input.rudder.maximum_angle_rad,
		angle_step_rad,
	)
	landscape = ControlLandscape(
		errors,
		gradient,
		curvature,
		optimal_sail_angle_rad,
		optimal_rudder_angle_rad,
	)
	if settings.mode == DIRECT_GRADIENT_CONTROL:
		return ControlDecision(None, landscape)
	return ControlDecision(
		(optimal_sail_angle_rad, optimal_rudder_angle_rad),
		landscape,
	)


def control(
	control_input: ControlInput,
	intent: MotionIntent,
	dt_seconds: float,
	settings: ControllerSettings,
) -> ControlOutput:
	decision = control_decision(control_input, intent, settings)
	if decision.target_angles_rad is None:
		sail_torque_n_m = direct_gradient_control_torque(
			control_input.sail,
			decision.landscape.gradient[0],
			decision.landscape.curvature[0],
		)
		rudder_torque_n_m = direct_gradient_control_torque(
			control_input.rudder,
			decision.landscape.gradient[1],
			decision.landscape.curvature[1],
		)
	else:
		sail_torque_n_m = actuator_control_torque(
			control_input.sail,
			decision.target_angles_rad[0],
			dt_seconds,
		)
		rudder_torque_n_m = actuator_control_torque(
			control_input.rudder,
			decision.target_angles_rad[1],
			dt_seconds,
		)
	return ControlOutput(
		ActuatorTorques(sail_torque_n_m, rudder_torque_n_m),
		decision,
	)
