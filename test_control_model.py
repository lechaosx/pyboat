import itertools
import math
import unittest
from functools import lru_cache

import pymunk

import sailing_game as game


HEADINGS_RAD = tuple(math.radians(degrees) for degrees in (0, 45, 90, 135, 180))
THROUGH_WATER_SPEEDS_M_S = (-1, 0, 2)
ANGULAR_VELOCITIES_RAD_S = (-0.2, 0, 0.2)
LOCAL_STICK_POSITIONS = (
	(0, 0),
	(0.5, 0),
	(1, 0),
	(-0.5, 0),
	(-1, 0),
	(0, 0.5),
	(0, 1),
	(0.5, 0.5),
	(1, 1),
	(-0.5, 0.5),
	(-1, 1),
)


def world_stick(
	local_stick: tuple[float, float],
	boat_heading_rad: float,
) -> pymunk.Vec2d:
	local_vector = pymunk.Vec2d(*local_stick)
	radius = max(abs(local_vector.x), abs(local_vector.y))
	if not radius:
		return pymunk.Vec2d(0, 0)
	return local_vector.normalized().rotated(boat_heading_rad) * radius


def flow_state(
	boat_heading_rad: float,
	through_water_speed_m_s: float,
) -> tuple[pymunk.Vec2d, pymunk.Vec2d]:
	boat_velocity_m_s = pymunk.Vec2d(through_water_speed_m_s, 0).rotated(
		boat_heading_rad
	)
	return (
		game.apparent_flow(game.DEFAULT_ENVIRONMENT.true_wind_velocity_m_s, boat_velocity_m_s),
		game.apparent_flow(game.DEFAULT_ENVIRONMENT.true_water_velocity_m_s, boat_velocity_m_s),
	)


@lru_cache
def control_candidates(
	boat_heading_rad: float,
	through_water_speed_m_s: float,
) -> tuple[
	tuple[game.ControlEffect, ...],
	tuple[game.ControlEffect, ...],
]:
	apparent_wind_m_s, apparent_water_m_s = flow_state(
		boat_heading_rad,
		through_water_speed_m_s,
	)
	return (
		game.foil_control_candidates(
			boat_heading_rad,
			apparent_wind_m_s,
			game.MAST_POSITION_M,
			game.SAIL_CENTER_OF_EFFORT_FROM_MAST_M,
			game.MAX_SAIL_ANGLE_RAD,
			game.AIR_DENSITY_KG_M3,
			game.SAIL_AREA_M2,
		),
		game.foil_control_candidates(
			boat_heading_rad,
			apparent_water_m_s,
			game.RUDDER_POSITION_M,
			game.RUDDER_DRAW_LENGTH_M / 2,
			game.MAX_RUDDER_ANGLE_RAD,
			game.WATER_DENSITY_KG_M3,
			game.RUDDER_AREA_M2,
		),
	)


def combined_effects(
	sails: tuple[game.ControlEffect, ...],
	rudders: tuple[game.ControlEffect, ...],
) -> tuple[tuple[game.ControlEffect, game.ControlEffect, float, float], ...]:
	return tuple(
		(
			sail,
			rudder,
			sail.drive_force_n + rudder.drive_force_n,
			sail.yaw_moment_n_m + rudder.yaw_moment_n_m,
		)
		for sail, rudder in itertools.product(sails, rudders)
	)


def normalized_effect(value: float, negative_limit: float, positive_limit: float) -> float:
	if value > 0 and positive_limit:
		return value / positive_limit
	if value < 0 and negative_limit:
		return value / -negative_limit
	return 0


def effect_limits(
	effects: tuple[tuple[game.ControlEffect, game.ControlEffect, float, float], ...],
) -> tuple[float, float, float, float]:
	drives = tuple(effect[2] for effect in effects)
	moments = tuple(effect[3] for effect in effects)
	return (
		min(0, min(drives)),
		max(0, max(drives)),
		min(0, min(moments)),
		max(0, max(moments)),
	)


def desired_turn(
	turn: float,
	angular_velocity_rad_s: float,
	negative_moment_n_m: float,
	positive_moment_n_m: float,
) -> float:
	if turn:
		return turn
	if angular_velocity_rad_s > 0:
		yaw_stiffness_n_m_rad = -negative_moment_n_m
	else:
		yaw_stiffness_n_m_rad = positive_moment_n_m
	damping_n_m_s_rad = 2 * math.sqrt(
		yaw_stiffness_n_m_rad * game.BOAT_MOMENT_OF_INERTIA_KG_M2
	)
	required_moment_n_m = -angular_velocity_rad_s * damping_n_m_s_rad
	return max(
		-1,
		min(
			1,
			normalized_effect(
				required_moment_n_m,
				negative_moment_n_m,
				positive_moment_n_m,
			),
		),
	)


def target_effect(
	boat_heading_rad: float,
	angular_velocity_rad_s: float,
	intent: game.MotionIntent,
	apparent_wind_m_s: pymunk.Vec2d,
	apparent_water_m_s: pymunk.Vec2d,
) -> tuple[game.ControlEffect, game.ControlEffect]:
	target_sail_angle_rad, target_rudder_angle_rad = game.intended_actuator_angles(
		boat_heading_rad,
		angular_velocity_rad_s,
		intent,
		apparent_wind_m_s,
		apparent_water_m_s,
	)
	return (
		game.foil_control_effect(
			boat_heading_rad,
			apparent_wind_m_s,
			game.MAST_POSITION_M,
			game.SAIL_CENTER_OF_EFFORT_FROM_MAST_M,
			target_sail_angle_rad,
			game.AIR_DENSITY_KG_M3,
			game.SAIL_AREA_M2,
		),
		game.foil_control_effect(
			boat_heading_rad,
			apparent_water_m_s,
			game.RUDDER_POSITION_M,
			game.RUDDER_DRAW_LENGTH_M / 2,
			target_rudder_angle_rad,
			game.WATER_DENSITY_KG_M3,
			game.RUDDER_AREA_M2,
		),
	)


def non_neutral_failure(
	intent: game.MotionIntent,
	angular_velocity_rad_s: float,
	selected: tuple[game.ControlEffect, game.ControlEffect],
	effects: tuple[tuple[game.ControlEffect, game.ControlEffect, float, float], ...],
) -> str | None:
	negative_drive, positive_drive, negative_moment, positive_moment = effect_limits(effects)
	desired = pymunk.Vec2d(
		intent.forward,
		desired_turn(
			intent.turn,
			angular_velocity_rad_s,
			negative_moment,
			positive_moment,
		),
	)

	def distance(drive_force_n: float, yaw_moment_n_m: float) -> float:
		normalized = pymunk.Vec2d(
			normalized_effect(drive_force_n, negative_drive, positive_drive),
			normalized_effect(yaw_moment_n_m, negative_moment, positive_moment),
		)
		return (normalized - desired).length

	selected_distance = distance(
		selected[0].drive_force_n + selected[1].drive_force_n,
		selected[0].yaw_moment_n_m + selected[1].yaw_moment_n_m,
	)
	best_distance = min(distance(effect[2], effect[3]) for effect in effects)
	if selected_distance > best_distance + 1e-9:
		return f"distance {selected_distance:.3f}, best {best_distance:.3f}"
	return None


def neutral_failure(
	selected: tuple[game.ControlEffect, game.ControlEffect],
	sails: tuple[game.ControlEffect, ...],
	rudders: tuple[game.ControlEffect, ...],
	boat_heading_rad: float,
	apparent_wind_m_s: pymunk.Vec2d,
	angular_velocity_rad_s: float,
) -> str | None:
	def sail_force(angle_rad: float) -> pymunk.Vec2d:
		return game.force_on_foil(
			apparent_wind_m_s,
			boat_heading_rad + angle_rad,
			game.AIR_DENSITY_KG_M3,
			game.SAIL_AREA_M2,
		)

	selected_force_n = sail_force(selected[0].angle_rad).length
	minimum_force_n = min(sail_force(sail.angle_rad).length for sail in sails)
	if selected_force_n > minimum_force_n + 1e-9:
		return f"sail load {selected_force_n:.3f} N, minimum {minimum_force_n:.3f} N"

	moments = tuple(selected[0].yaw_moment_n_m + rudder.yaw_moment_n_m for rudder in rudders)
	turn_request = desired_turn(
		0,
		angular_velocity_rad_s,
		min(0, min(moments)),
		max(0, max(moments)),
	)
	if turn_request < 0:
		target_moment_n_m = turn_request * -min(moments)
	else:
		target_moment_n_m = turn_request * max(moments)
	selected_moment_n_m = selected[0].yaw_moment_n_m + selected[1].yaw_moment_n_m
	best_error_n_m = min(abs(moment - target_moment_n_m) for moment in moments)
	if abs(selected_moment_n_m - target_moment_n_m) > best_error_n_m + 1e-9:
		return "rudder does not apply the closest achievable yaw-stopping moment"
	return None


class ControlModelSpecificationTest(unittest.TestCase):
	def test_representative_sticks_across_characteristic_boat_states(self):
		failures = []
		for heading_rad, speed_m_s, angular_velocity_rad_s, local_stick in itertools.product(
			HEADINGS_RAD,
			THROUGH_WATER_SPEEDS_M_S,
			ANGULAR_VELOCITIES_RAD_S,
			LOCAL_STICK_POSITIONS,
		):
			stick = world_stick(local_stick, heading_rad)
			intent = game.analog_motion_intent(
				heading_rad,
				stick.x,
				stick.y,
			)
			apparent_wind_m_s, apparent_water_m_s = flow_state(heading_rad, speed_m_s)
			selected = target_effect(
				heading_rad,
				angular_velocity_rad_s,
				intent,
				apparent_wind_m_s,
				apparent_water_m_s,
			)
			case = (
				f"heading={math.degrees(heading_rad):.0f}°, speed={speed_m_s:+g} m/s, "
				f"yaw={angular_velocity_rad_s:+g} rad/s, stick={local_stick}"
			)
			sails, rudders = control_candidates(heading_rad, speed_m_s)
			if local_stick == (0, 0):
				failure = neutral_failure(
					selected,
					sails,
					rudders,
					heading_rad,
					apparent_wind_m_s,
					angular_velocity_rad_s,
				)
			else:
				failure = non_neutral_failure(
					intent,
					angular_velocity_rad_s,
					selected,
					combined_effects(sails, rudders),
				)
			if failure:
				failures.append(f"{case}: {failure}")

		if failures:
			self.fail(
				f"{len(failures)} control cases violated the specification:\n"
				+ "\n".join(failures[:12])
			)

	def test_mirrored_cases_produce_mirrored_targets(self):
		for heading_rad, speed_m_s, angular_velocity_rad_s, local_stick in itertools.product(
			HEADINGS_RAD[1:-1],
			THROUGH_WATER_SPEEDS_M_S,
			ANGULAR_VELOCITIES_RAD_S,
			LOCAL_STICK_POSITIONS,
		):
			stick = world_stick(local_stick, heading_rad)
			intent = game.analog_motion_intent(
				heading_rad,
				stick.x,
				stick.y,
			)
			wind, water = flow_state(heading_rad, speed_m_s)
			targets = game.intended_actuator_angles(
				heading_rad,
				angular_velocity_rad_s,
				intent,
				wind,
				water,
			)

			mirrored_heading_rad = -heading_rad
			mirrored_local_stick = (local_stick[0], -local_stick[1])
			mirrored_stick = world_stick(mirrored_local_stick, mirrored_heading_rad)
			mirrored_intent = game.analog_motion_intent(
				mirrored_heading_rad,
				mirrored_stick.x,
				mirrored_stick.y,
			)
			mirrored_wind, mirrored_water = flow_state(mirrored_heading_rad, speed_m_s)
			mirrored_targets = game.intended_actuator_angles(
				mirrored_heading_rad,
				-angular_velocity_rad_s,
				mirrored_intent,
				mirrored_wind,
				mirrored_water,
			)

			for target, mirrored_target in zip(targets, mirrored_targets):
				self.assertAlmostEqual(
					target,
					-mirrored_target,
					delta=math.radians(game.CONTROL_ANGLE_RESOLUTION_DEG) + 1e-12,
				)


if __name__ == "__main__":
	unittest.main()
