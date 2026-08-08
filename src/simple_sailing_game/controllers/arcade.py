import math
from dataclasses import dataclass


@dataclass(frozen=True)
class MotionIntent:
	forward: float = 0
	turn: float = 0


@dataclass(frozen=True)
class AutoSheetIntent:
	power: float = 0
	rudder: float = 0


@dataclass(frozen=True)
class SurfaceEffect:
	angle_rad: float
	drive_force_n: float
	yaw_moment_n_m: float


@dataclass(frozen=True)
class ControlInput:
	boat_angular_velocity_rad_s: float
	boat_moment_of_inertia_kg_m2: float
	current_sail_yaw_moment_n_m: float
	free_sail_angle_rad: float
	maximum_rudder_angle_rad: float
	sail_effects: tuple[SurfaceEffect, ...]
	rudder_effects: tuple[SurfaceEffect, ...]


@dataclass(frozen=True)
class SurfaceTargets:
	sheet_limit_angle_rad: float
	rudder_target_angle_rad: float
	sail_target_angle_rad: float | None = None


def _clamp(value: float, minimum: float, maximum: float) -> float:
	return max(minimum, min(maximum, value))


def _critical_yaw_damping_moment(
	angular_velocity_rad_s: float,
	moment_of_inertia_kg_m2: float,
	maximum_rudder_angle_rad: float,
	available_moments_n_m: tuple[float, ...],
) -> float:
	minimum_moment_n_m = min(available_moments_n_m)
	maximum_moment_n_m = max(available_moments_n_m)
	moment_stiffness_n_m_rad = (
		(maximum_moment_n_m - minimum_moment_n_m)
		/ (2 * maximum_rudder_angle_rad)
	)
	critical_damping_n_m_s_rad = 2 * math.sqrt(
		moment_of_inertia_kg_m2 * moment_stiffness_n_m_rad
	)
	return _clamp(
		-angular_velocity_rad_s * critical_damping_n_m_s_rad,
		minimum_moment_n_m,
		maximum_moment_n_m,
	)


def _trimmed_sail_effect(control_input: ControlInput) -> SurfaceEffect:
	free_angle_rad = control_input.free_sail_angle_rad
	if free_angle_rad < 0:
		available = tuple(
			effect
			for effect in control_input.sail_effects
			if free_angle_rad <= effect.angle_rad <= 0
		)
	elif free_angle_rad > 0:
		available = tuple(
			effect
			for effect in control_input.sail_effects
			if 0 <= effect.angle_rad <= free_angle_rad
		)
	else:
		available = tuple(
			effect for effect in control_input.sail_effects if effect.angle_rad == 0
		)
	return max(available, key=lambda effect: (effect.drive_force_n, -abs(effect.angle_rad)))


def _sail_target(
	control_input: ControlInput,
	power: float,
	allow_sternway: bool,
	gradual_sternway: bool = False,
) -> tuple[float, SurfaceEffect, float | None]:
	if power < 0 and allow_sternway:
		backed = min(
			control_input.sail_effects,
			key=lambda effect: (effect.drive_force_n, abs(effect.angle_rad)),
		)
		if not gradual_sternway:
			return abs(backed.angle_rad), backed, backed.angle_rad
		backing_power = _clamp(-power, 0, 1)
		target_angle_rad = control_input.free_sail_angle_rad + backing_power * (
			backed.angle_rad - control_input.free_sail_angle_rad
		)
		nearest_effect = min(
			control_input.sail_effects,
			key=lambda effect: abs(effect.angle_rad - target_angle_rad),
		)
		return abs(target_angle_rad), nearest_effect, target_angle_rad
	power = _clamp(power, 0, 1)
	trimmed = _trimmed_sail_effect(control_input)
	target_angle_rad = control_input.free_sail_angle_rad + power * (
		trimmed.angle_rad - control_input.free_sail_angle_rad
	)
	nearest_effect = min(
		control_input.sail_effects,
		key=lambda effect: abs(effect.angle_rad - target_angle_rad),
	)
	return abs(target_angle_rad), nearest_effect, None


def auto_sheet_control(
	control_input: ControlInput,
	intent: AutoSheetIntent,
) -> SurfaceTargets:
	sheet_limit_angle_rad, _, sail_target_angle_rad = _sail_target(
		control_input,
		intent.power,
		False,
	)
	rudder_target_angle_rad = (
		_clamp(intent.rudder, -1, 1) * control_input.maximum_rudder_angle_rad
	)
	return SurfaceTargets(
		sheet_limit_angle_rad,
		rudder_target_angle_rad,
		sail_target_angle_rad,
	)


def yaw_rate_move_control(
	control_input: ControlInput,
	intent: MotionIntent,
	maximum_yaw_rate_rad_s: float,
) -> SurfaceTargets:
	sheet_limit_angle_rad, sail_effect, sail_target_angle_rad = _sail_target(
		control_input,
		_clamp(intent.forward, -1, 1),
		True,
		True,
	)
	desired_yaw_rate_rad_s = (
		_clamp(intent.turn, -1, 1) * maximum_yaw_rate_rad_s
	)
	rate_error = _clamp(
		(desired_yaw_rate_rad_s - control_input.boat_angular_velocity_rad_s)
		/ maximum_yaw_rate_rad_s,
		-1,
		1,
	)
	total_moments_n_m = tuple(
		sail_effect.yaw_moment_n_m + rudder.yaw_moment_n_m
		for rudder in control_input.rudder_effects
	)
	if rate_error > 0:
		target_moment_n_m = rate_error * max(0, max(total_moments_n_m))
	elif rate_error < 0:
		target_moment_n_m = -rate_error * min(0, min(total_moments_n_m))
	else:
		target_moment_n_m = 0
	rudder_effect = min(
		zip(control_input.rudder_effects, total_moments_n_m),
		key=lambda item: (
			abs(item[1] - target_moment_n_m),
			abs(item[0].angle_rad),
		),
	)[0]
	return SurfaceTargets(
		sheet_limit_angle_rad,
		rudder_effect.angle_rad,
		sail_target_angle_rad,
	)


def direct_move_control(
	control_input: ControlInput,
	intent: MotionIntent,
) -> SurfaceTargets:
	power = _clamp(intent.forward, -1, 1)
	free_effect = min(
		control_input.sail_effects,
		key=lambda effect: abs(effect.angle_rad - control_input.free_sail_angle_rad),
	)
	if power >= 0:
		end_effect = _trimmed_sail_effect(control_input)
		if control_input.free_sail_angle_rad < 0:
			available_sails = tuple(
				effect
				for effect in control_input.sail_effects
				if control_input.free_sail_angle_rad <= effect.angle_rad <= 0
			)
		elif control_input.free_sail_angle_rad > 0:
			available_sails = tuple(
				effect
				for effect in control_input.sail_effects
				if 0 <= effect.angle_rad <= control_input.free_sail_angle_rad
			)
		else:
			available_sails = (free_effect,)
		fraction = power
		sail_target_angle_rad = None
	else:
		end_effect = min(
			control_input.sail_effects,
			key=lambda effect: (effect.drive_force_n, abs(effect.angle_rad)),
		)
		available_sails = control_input.sail_effects
		fraction = -power
		sail_target_angle_rad = end_effect.angle_rad
	target_drive_force_n = free_effect.drive_force_n + fraction * (
		end_effect.drive_force_n - free_effect.drive_force_n
	)
	target_angle_rad = free_effect.angle_rad + fraction * (
		end_effect.angle_rad - free_effect.angle_rad
	)
	sail_effect = min(
		available_sails,
		key=lambda effect: (
			abs(effect.drive_force_n - target_drive_force_n),
			abs(effect.angle_rad - target_angle_rad),
		),
	)
	if sail_target_angle_rad is not None:
		sail_target_angle_rad = sail_effect.angle_rad

	turn = _clamp(intent.turn, -1, 1)
	sail_yaw_moment_n_m = (
		sail_effect.yaw_moment_n_m
		if turn
		else control_input.current_sail_yaw_moment_n_m
	)
	total_moments_n_m = tuple(
		sail_yaw_moment_n_m + rudder.yaw_moment_n_m
		for rudder in control_input.rudder_effects
	)
	if turn > 0:
		target_moment_n_m = turn * max(0, max(total_moments_n_m))
	elif turn < 0:
		target_moment_n_m = -turn * min(0, min(total_moments_n_m))
	else:
		target_moment_n_m = _critical_yaw_damping_moment(
			control_input.boat_angular_velocity_rad_s,
			control_input.boat_moment_of_inertia_kg_m2,
			control_input.maximum_rudder_angle_rad,
			total_moments_n_m,
		)
	rudder_effect = min(
		zip(control_input.rudder_effects, total_moments_n_m),
		key=lambda item: (
			abs(item[1] - target_moment_n_m),
			abs(item[0].angle_rad),
		),
	)[0]
	return SurfaceTargets(
		abs(sail_effect.angle_rad),
		rudder_effect.angle_rad,
		sail_target_angle_rad,
	)
