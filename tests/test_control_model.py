import itertools
import math
import unittest
from dataclasses import replace
from functools import lru_cache

import numpy as np
import pymunk

from simple_sailing_game import boat, game
from simple_sailing_game.controllers import assisted as controller, manual
from tests.support import controller_input_from_flows, intended_actuator_angles


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
		boat.apparent_flow(game.ENVIRONMENT.true_wind_velocity_m_s, boat_velocity_m_s),
		boat.apparent_flow(game.ENVIRONMENT.true_water_velocity_m_s, boat_velocity_m_s),
	)


def control_decision(
	boat_angle_rad,
	boat_angular_velocity_rad_s,
	intent,
	apparent_wind_m_s,
	apparent_water_m_s,
	sail_angle_rad,
	rudder_angle_rad,
	mode,
	use_global_slope=True,
):
	control_input = controller_input_from_flows(
		boat_angle_rad,
		boat_angular_velocity_rad_s,
		apparent_wind_m_s,
		apparent_water_m_s,
		sail_angle_rad,
		rudder_angle_rad,
	)
	settings = controller.ControllerSettings(
		mode,
		use_global_slope,
		game.CONTROL_ANGLE_RESOLUTION_DEG,
	)
	return controller.control_decision(control_input, intent, settings)


def direct_gradient_control_torque(
	objective_gradient,
	objective_curvature,
	angular_velocity_rad_s,
	external_torque_n_m,
	moment_of_inertia_kg_m2,
	bearing_damping_n_m_s_rad,
	max_torque_n_m,
	maximum_angle_rad,
):
	actuator = controller.ActuatorState(
		0,
		angular_velocity_rad_s,
		external_torque_n_m,
		moment_of_inertia_kg_m2,
		bearing_damping_n_m_s_rad,
		max_torque_n_m,
		maximum_angle_rad,
	)
	return controller.direct_gradient_control_torque(
		actuator,
		objective_gradient,
		objective_curvature,
	)


@lru_cache
def control_candidates(
	boat_heading_rad: float,
	through_water_speed_m_s: float,
	angular_velocity_rad_s: float,
) -> tuple[
	tuple[controller.ControlEffect, ...],
	tuple[controller.ControlEffect, ...],
]:
	apparent_wind_m_s, apparent_water_m_s = flow_state(
		boat_heading_rad,
		through_water_speed_m_s,
	)
	control_input = controller_input_from_flows(
		boat_heading_rad,
		angular_velocity_rad_s,
		apparent_wind_m_s,
		apparent_water_m_s,
	)
	return control_input.sail_candidates, control_input.rudder_candidates


def combined_effects(
	sails: tuple[controller.ControlEffect, ...],
	rudders: tuple[controller.ControlEffect, ...],
) -> tuple[tuple[controller.ControlEffect, controller.ControlEffect, float, float], ...]:
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
	effects: tuple[tuple[controller.ControlEffect, controller.ControlEffect, float, float], ...],
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
		yaw_stiffness_n_m_rad
		* boat.boat_moment_of_inertia_kg_m2(game.BOAT_PARAMETERS)
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
	intent: controller.MotionIntent,
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
	intent: controller.MotionIntent,
	apparent_wind_m_s: pymunk.Vec2d,
	apparent_water_m_s: pymunk.Vec2d,
) -> tuple[controller.ControlEffect, controller.ControlEffect]:
	target_sail_angle_rad, target_rudder_angle_rad = intended_actuator_angles(
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
			game.BOAT_PARAMETERS.mast_position_m,
			game.BOAT_PARAMETERS.sail_center_of_effort_from_mast_m,
			target_sail_angle_rad,
			game.BOAT_PARAMETERS.air_density_kg_m3,
			game.BOAT_PARAMETERS.sail_area_m2,
			angular_velocity_rad_s,
		),
		game.foil_control_effect(
			boat_heading_rad,
			apparent_water_m_s,
			game.BOAT_PARAMETERS.rudder_position_m,
			game.BOAT_PARAMETERS.rudder_draw_length_m / 2,
			target_rudder_angle_rad,
			game.BOAT_PARAMETERS.water_density_kg_m3,
			game.BOAT_PARAMETERS.rudder_area_m2,
			angular_velocity_rad_s,
		),
	)


def non_neutral_failure(
	intent: controller.MotionIntent,
	angular_velocity_rad_s: float,
	selected: tuple[controller.ControlEffect, controller.ControlEffect],
	effects: tuple[tuple[controller.ControlEffect, controller.ControlEffect, float, float], ...],
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
	selected: tuple[controller.ControlEffect, controller.ControlEffect],
	sails: tuple[controller.ControlEffect, ...],
	rudders: tuple[controller.ControlEffect, ...],
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

	def error(sail: controller.ControlEffect, moment_n_m: float) -> float:
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
		intent = controller.MotionIntent(0.5, 0)
		existing_target = intended_actuator_angles(
			heading_rad,
			-0.2,
			intent,
			wind,
			water,
		)

		decision = control_decision(
			heading_rad,
			-0.2,
			intent,
			wind,
			water,
			sail_angle_rad=0,
			rudder_angle_rad=0,
			mode=controller.OPTIMIZED_TARGET_CONTROL,
		)

		self.assertEqual(
			decision.target_angles_rad,
			existing_target,
		)
		self.assertEqual(
			decision.target_angles_rad,
			(
				decision.landscape.optimum_sail_angle_rad,
				decision.landscape.optimum_rudder_angle_rad,
			),
		)

	def test_direct_gradient_mode_has_a_landscape_but_no_target(self):
		heading_rad = 0
		wind, water = flow_state(heading_rad, -1)
		decision = control_decision(
			heading_rad,
			-0.2,
			controller.MotionIntent(0.5, 0),
			wind,
			water,
			sail_angle_rad=0,
			rudder_angle_rad=0,
			mode=controller.DIRECT_GRADIENT_CONTROL,
		)

		self.assertIsNone(decision.target_angles_rad)
		self.assertEqual(decision.landscape.errors.shape, (181, 61))
		self.assertTrue(all(map(math.isfinite, decision.landscape.gradient)))
		self.assertNotEqual(decision.landscape.gradient, (0, 0))
		self.assertTrue(math.isfinite(decision.landscape.optimum_sail_angle_rad))
		self.assertTrue(math.isfinite(decision.landscape.optimum_rudder_angle_rad))

	def test_direct_gradient_global_slope_can_be_disabled(self):
		heading_rad = 0
		wind, water = flow_state(heading_rad, -1)
		arguments = (
			heading_rad,
			-0.2,
			controller.MotionIntent(0.5, 0.5),
			wind,
			water,
			game.BOAT_PARAMETERS.max_sail_angle_rad,
			game.BOAT_PARAMETERS.max_rudder_angle_rad,
		)
		base = control_decision(*arguments, controller.OPTIMIZED_TARGET_CONTROL)
		unguided = control_decision(
			*arguments,
			controller.DIRECT_GRADIENT_CONTROL,
			use_global_slope=False,
		)

		self.assertIsNone(unguided.target_angles_rad)
		np.testing.assert_allclose(unguided.landscape.errors, base.landscape.errors)
		self.assertEqual(unguided.landscape.gradient, base.landscape.gradient)

	def test_direct_landscape_adds_a_sail_only_well_that_steepens_with_distance(self):
		heading_rad = 0
		wind, water = flow_state(heading_rad, -1)
		arguments = (
			heading_rad,
			-0.2,
			controller.MotionIntent(0.5, 0.5),
			wind,
			water,
			game.BOAT_PARAMETERS.max_sail_angle_rad,
			game.BOAT_PARAMETERS.max_rudder_angle_rad,
		)
		base = control_decision(*arguments, controller.OPTIMIZED_TARGET_CONTROL)
		direct = control_decision(*arguments, controller.DIRECT_GRADIENT_CONTROL)

		self.assertEqual(
			(
				direct.landscape.optimum_sail_angle_rad,
				direct.landscape.optimum_rudder_angle_rad,
			),
			(
				base.landscape.optimum_sail_angle_rad,
				base.landscape.optimum_rudder_angle_rad,
			),
		)
		added_slope = direct.landscape.errors - base.landscape.errors
		optimum_index = np.argmin(added_slope[:, 0])
		sail_steps = np.abs(np.diff(added_slope[:, 0]))
		self.assertGreater(sail_steps[0], sail_steps[optimum_index - 1])
		self.assertGreater(sail_steps[-1], sail_steps[optimum_index])
		np.testing.assert_allclose(
			added_slope,
			np.broadcast_to(added_slope[:, :1], added_slope.shape),
			atol=1e-12,
		)
		self.assertGreaterEqual(added_slope.min(), -1e-12)

	def test_neutral_is_one_landscape_in_direct_gradient_mode(self):
		heading_rad = math.pi / 2
		wind, water = flow_state(heading_rad, 2)
		decision = control_decision(
			heading_rad,
			0.2,
			controller.MotionIntent(),
			wind,
			water,
			sail_angle_rad=0,
			rudder_angle_rad=0,
			mode=controller.DIRECT_GRADIENT_CONTROL,
		)

		self.assertIsNone(decision.target_angles_rad)
		self.assertEqual(decision.landscape.errors.shape, (181, 61))
		self.assertTrue(all(map(math.isfinite, decision.landscape.gradient)))
		self.assertTrue(math.isfinite(decision.landscape.optimum_sail_angle_rad))
		self.assertTrue(math.isfinite(decision.landscape.optimum_rudder_angle_rad))

	def test_direct_gradient_uses_full_torque_downhill(self):
		for gradient, expected_torque in ((2, -75), (-2, 75)):
			with self.subTest(gradient=gradient):
				torque = direct_gradient_control_torque(
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
		torque = direct_gradient_control_torque(
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
		torque = direct_gradient_control_torque(
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

	def test_allocator_returns_the_best_discrete_angle_pair(self):
		heading_rad = 0
		speed_m_s = -1
		angular_velocity_rad_s = -0.2
		intent = controller.MotionIntent(0.5, 0)
		wind, water = flow_state(heading_rad, speed_m_s)
		sails, rudders = control_candidates(
			heading_rad,
			speed_m_s,
			angular_velocity_rad_s,
		)
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

		self.assertAlmostEqual(selected_distance, best_discrete_distance)

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
			sails, rudders = control_candidates(
				heading_rad,
				speed_m_s,
				angular_velocity_rad_s,
			)
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
			targets = intended_actuator_angles(
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
			mirrored_targets = intended_actuator_angles(
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


class ManualControlTest(unittest.TestCase):
	def test_held_controls_apply_full_sailor_torque(self):
		control_input = controller_input_from_flows(
			boat_angle_rad=0,
			boat_angular_velocity_rad_s=0,
			apparent_wind_m_s=pymunk.Vec2d(6, 0),
			apparent_water_m_s=pymunk.Vec2d(-2, 0),
		)

		output = manual.control(
			control_input.sail,
			control_input.rudder,
			manual.ManualInput(sail=-1, rudder=1),
			game.DT_SECONDS,
		)

		self.assertEqual(
			output.sail_n_m,
			-control_input.sail.maximum_torque_n_m,
		)
		self.assertEqual(
			output.rudder_n_m,
			control_input.rudder.maximum_torque_n_m,
		)

	def test_released_controls_brake_and_hold_their_current_angles(self):
		control_input = controller_input_from_flows(
			boat_angle_rad=0,
			boat_angular_velocity_rad_s=0,
			apparent_wind_m_s=pymunk.Vec2d(6, 0),
			apparent_water_m_s=pymunk.Vec2d(-2, 0),
		)
		control_input = replace(
			control_input,
			sail=replace(
				control_input.sail,
				angle_rad=0.4,
				angular_velocity_rad_s=0.2,
				external_torque_n_m=10,
			),
			rudder=replace(
				control_input.rudder,
				angle_rad=-0.2,
				angular_velocity_rad_s=-0.1,
				external_torque_n_m=-5,
			),
		)

		output = manual.control(
			control_input.sail,
			control_input.rudder,
			manual.ManualInput(),
			game.DT_SECONDS,
		)

		self.assertEqual(
			output.sail_n_m,
			max(
				-control_input.sail.maximum_torque_n_m,
				min(
					control_input.sail.maximum_torque_n_m,
					-control_input.sail.moment_of_inertia_kg_m2
					* control_input.sail.angular_velocity_rad_s
					/ game.DT_SECONDS
					- control_input.sail.external_torque_n_m
					+ control_input.sail.angular_velocity_rad_s
					* control_input.sail.bearing_damping_n_m_s_rad,
				),
			),
		)
		self.assertEqual(
			output.rudder_n_m,
			max(
				-control_input.rudder.maximum_torque_n_m,
				min(
					control_input.rudder.maximum_torque_n_m,
					-control_input.rudder.moment_of_inertia_kg_m2
					* control_input.rudder.angular_velocity_rad_s
					/ game.DT_SECONDS
					- control_input.rudder.external_torque_n_m
					+ control_input.rudder.angular_velocity_rad_s
					* control_input.rudder.bearing_damping_n_m_s_rad,
				),
			),
		)

	def test_analog_deflection_scales_direct_sailor_torque(self):
		control_input = controller_input_from_flows(
			boat_angle_rad=0,
			boat_angular_velocity_rad_s=0,
			apparent_wind_m_s=pymunk.Vec2d(6, 0),
			apparent_water_m_s=pymunk.Vec2d(-2, 0),
		)

		output = manual.control(
			control_input.sail,
			control_input.rudder,
			manual.ManualInput(sail=0.25, rudder=-0.5),
			game.DT_SECONDS,
		)

		self.assertEqual(
			output.sail_n_m,
			0.25 * control_input.sail.maximum_torque_n_m,
		)
		self.assertEqual(
			output.rudder_n_m,
			-0.5 * control_input.rudder.maximum_torque_n_m,
		)


if __name__ == "__main__":
	unittest.main()
