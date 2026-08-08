import math
from dataclasses import dataclass


@dataclass(frozen=True)
class MotionIntent:
	forward: float = 0
	turn: float = 0
	held_heading_rad: float | None = None


@dataclass
class HeadingHold:
	held_heading_rad: float | None = None

	def apply(
		self,
		boat_heading_rad: float,
		intent: MotionIntent,
	) -> MotionIntent:
		if intent.turn:
			self.held_heading_rad = None
		elif self.held_heading_rad is None:
			self.held_heading_rad = boat_heading_rad
		return MotionIntent(
			intent.forward,
			intent.turn,
			self.held_heading_rad,
		)


@dataclass(frozen=True)
class AutoSheetIntent:
	power: float = 0
	rudder: float = 0


@dataclass(frozen=True)
class HeadingIntent:
	world_direction: tuple[float, float] = (0, 0)
	explicit_reverse: bool = False


@dataclass(frozen=True)
class SurfaceEffect:
	angle_rad: float
	drive_force_n: float
	yaw_moment_n_m: float


@dataclass(frozen=True)
class ControlInput:
	boat_heading_rad: float
	boat_angular_velocity_rad_s: float
	boat_moment_of_inertia_kg_m2: float
	current_sail_yaw_moment_n_m: float
	free_sail_angle_rad: float
	maximum_sail_angle_rad: float
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


def _critical_yaw_correction_moment(
	heading_error_rad: float,
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
		-heading_error_rad * moment_stiffness_n_m_rad
		- angular_velocity_rad_s * critical_damping_n_m_s_rad,
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


def auto_sheet_manual_tiller(
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


def _heading_motion(
	control_input: ControlInput,
	intent: HeadingIntent,
) -> tuple[float, float]:
	direction_x, direction_y = intent.world_direction
	magnitude = math.hypot(direction_x, direction_y)
	if not magnitude:
		return 0, 0
	power = min(1, magnitude)
	heading_error_rad = math.remainder(
		math.atan2(direction_y, direction_x) - control_input.boat_heading_rad,
		2 * math.pi,
	)
	if intent.explicit_reverse:
		power = -power
	turn = _clamp(heading_error_rad / (math.pi / 2), -1, 1)
	return power, turn


def _motion_control(
	control_input: ControlInput,
	power: float,
	turn: float,
	maximum_yaw_rate_rad_s: float,
	gradual_sternway: bool = False,
) -> SurfaceTargets:
	sheet_limit_angle_rad, sail_effect, sail_target_angle_rad = _sail_target(
		control_input,
		power,
		True,
		gradual_sternway,
	)
	desired_yaw_rate_rad_s = turn * maximum_yaw_rate_rad_s
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


def heading_control(
	control_input: ControlInput,
	intent: HeadingIntent,
	maximum_yaw_rate_rad_s: float,
) -> SurfaceTargets:
	power, turn = _heading_motion(control_input, intent)
	return _motion_control(
		control_input,
		power,
		turn,
		maximum_yaw_rate_rad_s,
		True,
	)


def move_control(
	control_input: ControlInput,
	intent: MotionIntent,
	maximum_yaw_rate_rad_s: float,
) -> SurfaceTargets:
	return _motion_control(
		control_input,
		_clamp(intent.forward, -1, 1),
		_clamp(intent.turn, -1, 1),
		maximum_yaw_rate_rad_s,
		True,
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
		heading_error_rad = 0.0
		if intent.held_heading_rad is not None:
			heading_error_rad = math.remainder(
				control_input.boat_heading_rad - intent.held_heading_rad,
				2 * math.pi,
			)
		target_moment_n_m = _critical_yaw_correction_moment(
			heading_error_rad,
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
