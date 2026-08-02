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


def distance_from_intent(
	drive_force_n: float,
	yaw_moment_n_m: float,
	intent: game.MotionIntent,
	angular_velocity_rad_s: float,
	limits: tuple[float, float, float, float],
) -> float:
	negative_drive, positive_drive, negative_moment, positive_moment = limits
	desired = pymunk.Vec2d(
		intent.forward,
		desired_turn(
			intent.turn,
			angular_velocity_rad_s,
			negative_moment,
			positive_moment,
		),
	)
	normalized = pymunk.Vec2d(
		normalized_effect(drive_force_n, negative_drive, positive_drive),
		normalized_effect(yaw_moment_n_m, negative_moment, positive_moment),
	)
	return (normalized - desired).length


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
	limits = effect_limits(effects)
	selected_distance = distance_from_intent(
		selected[0].drive_force_n + selected[1].drive_force_n,
		selected[0].yaw_moment_n_m + selected[1].yaw_moment_n_m,
		intent,
		angular_velocity_rad_s,
		limits,
	)
	best_distance = min(
		distance_from_intent(
			effect[2],
			effect[3],
			intent,
			angular_velocity_rad_s,
			limits,
		)
		for effect in effects
	)
	if selected_distance > best_distance + 1e-9:
		return f"distance {selected_distance:.3f}, best {best_distance:.3f}"
	return None


def neutral_failure(
	selected: tuple[game.ControlEffect, game.ControlEffect],
	sails: tuple[game.ControlEffect, ...],
	rudders: tuple[game.ControlEffect, ...],
	angular_velocity_rad_s: float,
) -> str | None:
	effects = combined_effects(sails, rudders)
	moments = tuple(effect[3] for effect in effects)
	negative_moment_n_m = min(0, min(moments))
	positive_moment_n_m = max(0, max(moments))
	turn_request = desired_turn(
		0,
		angular_velocity_rad_s,
		negative_moment_n_m,
		positive_moment_n_m,
	)
	maximum_sail_force_n = max(sail.force_magnitude_n for sail in sails)

	def error(sail: game.ControlEffect, moment_n_m: float) -> float:
		load = sail.force_magnitude_n / maximum_sail_force_n if maximum_sail_force_n else 0
		yaw = normalized_effect(
			moment_n_m,
			negative_moment_n_m,
			positive_moment_n_m,
		)
		return load**2 + (yaw - turn_request) ** 2

	selected_error = error(
		selected[0],
		selected[0].yaw_moment_n_m + selected[1].yaw_moment_n_m,
	)
	best_error = min(error(effect[0], effect[3]) for effect in effects)
	if selected_error > best_error + 1e-9:
		return f"objective {selected_error:.3f}, best {best_error:.3f}"
	return None


class ControlModelSpecificationTest(unittest.TestCase):
	def test_optimized_mode_preserves_the_existing_target(self):
		heading_rad = 0
		wind, water = flow_state(heading_rad, -1)
		intent = game.MotionIntent(0.5, 0)
		existing_target = game.intended_actuator_angles(
			heading_rad,
			-0.2,
			intent,
			wind,
			water,
		)

		decision = game.control_decision(
			heading_rad,
			-0.2,
			intent,
			wind,
			water,
			sail_angle_rad=0,
			rudder_angle_rad=0,
			mode=game.OPTIMIZED_TARGET_CONTROL,
		)

		self.assertEqual(
			(decision.target_sail_angle_rad, decision.target_rudder_angle_rad),
			existing_target,
		)
		self.assertEqual(
			(decision.target_sail_angle_rad, decision.target_rudder_angle_rad),
			(
				decision.landscape.optimum_sail_angle_rad,
				decision.landscape.optimum_rudder_angle_rad,
			),
		)

	def test_direct_gradient_mode_has_a_landscape_but_no_target(self):
		heading_rad = 0
		wind, water = flow_state(heading_rad, -1)
		decision = game.control_decision(
			heading_rad,
			-0.2,
			game.MotionIntent(0.5, 0),
			wind,
			water,
			sail_angle_rad=0,
			rudder_angle_rad=0,
			mode=game.DIRECT_GRADIENT_CONTROL,
		)

		self.assertIsNone(decision.target_sail_angle_rad)
		self.assertIsNone(decision.target_rudder_angle_rad)
		self.assertEqual(decision.landscape.errors.shape, (181, 61))
		self.assertTrue(all(map(math.isfinite, decision.landscape.gradient)))
		self.assertNotEqual(decision.landscape.gradient, (0, 0))
		self.assertTrue(math.isfinite(decision.landscape.optimum_sail_angle_rad))
		self.assertTrue(math.isfinite(decision.landscape.optimum_rudder_angle_rad))

	def test_neutral_is_one_landscape_in_direct_gradient_mode(self):
		heading_rad = math.pi / 2
		wind, water = flow_state(heading_rad, 2)
		decision = game.control_decision(
			heading_rad,
			0.2,
			game.MotionIntent(),
			wind,
			water,
			sail_angle_rad=0,
			rudder_angle_rad=0,
			mode=game.DIRECT_GRADIENT_CONTROL,
		)

		self.assertIsNone(decision.target_sail_angle_rad)
		self.assertIsNone(decision.target_rudder_angle_rad)
		self.assertEqual(decision.landscape.errors.shape, (181, 61))
		self.assertTrue(all(map(math.isfinite, decision.landscape.gradient)))
		self.assertTrue(math.isfinite(decision.landscape.optimum_sail_angle_rad))
		self.assertTrue(math.isfinite(decision.landscape.optimum_rudder_angle_rad))

	def test_direct_gradient_uses_full_torque_downhill(self):
		for gradient, expected_torque in ((2, -75), (-2, 75)):
			with self.subTest(gradient=gradient):
				torque = game.direct_gradient_control_torque(
					objective_gradient=gradient,
					objective_curvature=1,
					angular_velocity_rad_s=0,
					external_torque_n_m=0,
					moment_of_inertia_kg_m2=1,
					bearing_damping_n_m_s_rad=0,
					max_torque_n_m=75,
					maximum_angle_rad=1,
				)
				self.assertEqual(torque, expected_torque)

	def test_direct_gradient_scales_torque_before_full_effort_would_be_useful(self):
		torque = game.direct_gradient_control_torque(
			objective_gradient=0.1,
			objective_curvature=0,
			angular_velocity_rad_s=0,
			external_torque_n_m=0,
			moment_of_inertia_kg_m2=1,
			bearing_damping_n_m_s_rad=0,
			max_torque_n_m=75,
			maximum_angle_rad=1,
		)

		self.assertAlmostEqual(torque, -7.5)

	def test_direct_gradient_balances_environment_and_damps_at_a_stationary_point(self):
		torque = game.direct_gradient_control_torque(
			objective_gradient=0,
			objective_curvature=1,
			angular_velocity_rad_s=1,
			external_torque_n_m=20,
			moment_of_inertia_kg_m2=1,
			bearing_damping_n_m_s_rad=0,
			max_torque_n_m=75,
			maximum_angle_rad=1,
		)

		self.assertAlmostEqual(torque, -20 - 2 * math.sqrt(75))

	def test_allocator_refines_beyond_the_best_discrete_angle_pair(self):
		heading_rad = 0
		speed_m_s = -1
		angular_velocity_rad_s = -0.2
		intent = game.MotionIntent(0.5, 0)
		wind, water = flow_state(heading_rad, speed_m_s)
		sails, rudders = control_candidates(heading_rad, speed_m_s)
		effects = combined_effects(sails, rudders)
		limits = effect_limits(effects)
		selected = target_effect(
			heading_rad,
			angular_velocity_rad_s,
			intent,
			wind,
			water,
		)
		selected_distance = distance_from_intent(
			selected[0].drive_force_n + selected[1].drive_force_n,
			selected[0].yaw_moment_n_m + selected[1].yaw_moment_n_m,
			intent,
			angular_velocity_rad_s,
			limits,
		)
		best_discrete_distance = min(
			distance_from_intent(
				effect[2],
				effect[3],
				intent,
				angular_velocity_rad_s,
				limits,
			)
			for effect in effects
		)

		self.assertLess(selected_distance, best_discrete_distance - 1e-8)

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
