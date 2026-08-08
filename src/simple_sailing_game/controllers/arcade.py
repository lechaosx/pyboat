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
	turn = _clamp(intent.turn, -1, 1)
	rate_error_fraction = turn
	if control_input.boat_angular_velocity_rad_s:
		rate_error_fraction -= (
			control_input.boat_angular_velocity_rad_s
			/ maximum_yaw_rate_rad_s
		)
	rate_error_fraction = _clamp(rate_error_fraction, -1, 1)
	total_moments_n_m = tuple(
		sail_effect.yaw_moment_n_m + rudder.yaw_moment_n_m
		for rudder in control_input.rudder_effects
	)
	if rate_error_fraction < 0:
		target_moment_n_m = -rate_error_fraction * min(0, min(total_moments_n_m))
	else:
		target_moment_n_m = rate_error_fraction * max(0, max(total_moments_n_m))
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
