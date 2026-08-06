import math
import os
import subprocess
import sys
import unittest

import pymunk
import pygame
from simple_sailing_game import boat, plot
from simple_sailing_game.controllers import assisted as controller, manual
from simple_sailing_game import game

from simple_sailing_game.game import (
	ANALOG_STICK_DEADZONE,
	BOAT_PARAMETERS,
	DT_SECONDS,
	ENVIRONMENT,
	SimulationTelemetry,
	analog_motion_intent,
	create_simulation,
	draw,
	intent_vector,
	keyboard_motion_intent,
	step,
)
from tests.support import intended_actuator_angles


apparent_flow = boat.apparent_flow
lift_coefficient = boat.lift_coefficient
integrate_hinge = boat.integrate_hinge
Environment = boat.Environment
MotionIntent = controller.MotionIntent
DEFAULT_ENVIRONMENT = ENVIRONMENT
MAX_SAIL_ANGLE_RAD = BOAT_PARAMETERS.max_sail_angle_rad
MAX_RUDDER_ANGLE_RAD = BOAT_PARAMETERS.max_rudder_angle_rad


def drag_coefficient(angle_of_attack_rad):
	return boat.drag_coefficient(
		angle_of_attack_rad,
		BOAT_PARAMETERS.foil_base_drag_coefficient,
	)


def force_on_foil(
	apparent_flow_m_s,
	foil_angle_rad,
	fluid_density_kg_m3,
	area_m2,
):
	return boat.force_on_foil(
		apparent_flow_m_s,
		foil_angle_rad,
		fluid_density_kg_m3,
		area_m2,
		BOAT_PARAMETERS.foil_base_drag_coefficient,
	)


def actuator_control_torque(
	angle_rad,
	angular_velocity_rad_s,
	target_angle_rad,
	external_torque_n_m,
	moment_of_inertia_kg_m2,
	bearing_damping_n_m_s_rad,
	max_torque_n_m,
	dt_seconds,
):
	actuator = controller.ActuatorState(
		angle_rad,
		angular_velocity_rad_s,
		external_torque_n_m,
		moment_of_inertia_kg_m2,
		bearing_damping_n_m_s_rad,
		max_torque_n_m,
		maximum_angle_rad=1,
	)
	return controller.actuator_control_torque(
		actuator,
		target_angle_rad,
		dt_seconds,
	)


def run_simulation(simulation, seconds, keys="w"):
	for _ in range(round(seconds / DT_SECONDS)):
		intent = keyboard_motion_intent(
			forward="w" in keys,
			backward="s" in keys,
			left="a" in keys,
			right="d" in keys,
		)
		step(simulation, DEFAULT_ENVIRONMENT, intent)


def target_angles(telemetry):
	assert telemetry.decision is not None
	assert telemetry.decision.target_angles_rad is not None
	return telemetry.decision.target_angles_rad


class StartupTest(unittest.TestCase):
	def test_import_does_not_start_game(self):
		environment = os.environ | {
			"PYGAME_HIDE_SUPPORT_PROMPT": "1",
			"SDL_AUDIODRIVER": "dummy",
			"SDL_VIDEODRIVER": "dummy",
		}

		result = subprocess.run(
			[sys.executable, "-c", "import simple_sailing_game.game"],
			capture_output=True,
			env=environment,
			text=True,
			timeout=2,
		)

		self.assertEqual(result.returncode, 0, result.stderr)


class InputAdapterTest(unittest.TestCase):
	def test_manual_keys_control_rudder_and_sail_axes(self):
		self.assertEqual(
			game.keyboard_manual_input(
				rudder_left=True,
				rudder_right=False,
				sail_left=False,
				sail_right=True,
			),
			manual.ManualInput(sail=1, rudder=-1),
		)

	def test_manual_mode_toggle_round_trips(self):
		manual = game.toggled_control_mode(game.ASSISTED_CONTROL)

		self.assertEqual(manual, game.MANUAL_CONTROL)
		self.assertEqual(
			game.toggled_control_mode(manual),
			game.ASSISTED_CONTROL,
		)

	def test_manual_sticks_map_left_to_rudder_and_right_to_sail(self):
		self.assertEqual(
			game.analog_manual_input(
				rudder_stick_x=-0.5,
				sail_stick_x=0.25,
			),
			manual.ManualInput(sail=0.25, rudder=-0.5),
		)
		self.assertEqual(
			game.analog_manual_input(
				rudder_stick_x=game.ANALOG_STICK_DEADZONE / 2,
				sail_stick_x=0,
			),
			manual.ManualInput(),
		)

	def test_interactive_game_starts_in_direct_gradient_mode(self):
		self.assertEqual(
			game.CONTROLLER_SETTINGS.mode,
			controller.DIRECT_GRADIENT_CONTROL,
		)

	def test_control_landscape_is_square(self):
		self.assertEqual(
			game.CONTROL_LANDSCAPE_RECT.width,
			game.CONTROL_LANDSCAPE_RECT.height,
		)

	def test_control_landscape_arrows_preserve_normalized_magnitudes(self):
		rect = game.CONTROL_LANDSCAPE_RECT
		gradient = game.control_gradient_vector_px(
			rect,
			pymunk.Vec2d(1 / MAX_SAIL_ANGLE_RAD, 1 / MAX_RUDDER_ANGLE_RAD),
		)
		motion = game.control_motion_vector_px(
			rect,
			MAX_SAIL_ANGLE_RAD,
			MAX_RUDDER_ANGLE_RAD,
		)

		self.assertEqual(
			gradient,
			(
				-game.CONTROL_GRADIENT_PIXELS_PER_ERROR,
				game.CONTROL_GRADIENT_PIXELS_PER_ERROR,
			),
		)
		self.assertEqual(
			motion,
			(
				rect.width / 2 * game.CONTROL_MOTION_PREVIEW_SECONDS,
				-rect.height / 2 * game.CONTROL_MOTION_PREVIEW_SECONDS,
			),
		)

	def test_intent_stores_a_local_square_vector(self):
		intent = MotionIntent(0.5, -0.5)

		self.assertEqual(intent.forward, 0.5)
		self.assertEqual(intent.turn, -0.5)

	def test_visual_intent_vector_preserves_square_components(self):
		vector = intent_vector(MotionIntent(0.5, 0.5), math.pi / 2)

		self.assertAlmostEqual(vector.x, -0.5)
		self.assertAlmostEqual(vector.y, 0.5)

	def test_keyboard_turn_is_a_full_lateral_preference(self):
		intent = keyboard_motion_intent(
			forward=False,
			backward=False,
			left=True,
			right=False,
		)

		self.assertEqual(intent, MotionIntent(0, -1))

	def test_keyboard_diagonal_is_a_full_boat_relative_vector(self):
		intent = keyboard_motion_intent(
			forward=True,
			backward=False,
			left=True,
			right=False,
		)

		self.assertEqual(intent, MotionIntent(1, -1))

	def test_keyboard_forward_is_boat_local(self):
		intent = keyboard_motion_intent(
			forward=True,
			backward=False,
			left=False,
			right=False,
		)

		self.assertEqual(intent, MotionIntent(1, 0))

	def test_keyboard_reverse_and_opposing_keys_are_distinct(self):
		neutral = keyboard_motion_intent(
			forward=False,
			backward=False,
			left=False,
			right=False,
		)
		backwind = keyboard_motion_intent(
			forward=False,
			backward=True,
			left=False,
			right=False,
		)
		cancelled = keyboard_motion_intent(
			forward=True,
			backward=True,
			left=False,
			right=False,
		)

		self.assertEqual(neutral, MotionIntent())
		self.assertEqual(backwind, MotionIntent(-1, 0))
		self.assertEqual(cancelled, MotionIntent())

	def test_analog_stick_rotates_world_direction_into_boat_coordinates(self):
		forward = analog_motion_intent(
			boat_heading_rad=math.pi / 2,
			stick_x=0,
			stick_y=1,
		)

		self.assertEqual(forward, MotionIntent(1, 0))

	def test_analog_stick_projects_radius_onto_local_square(self):
		diagonal = 1 / math.sqrt(2)
		full = analog_motion_intent(
			boat_heading_rad=0,
			stick_x=diagonal,
			stick_y=diagonal,
		)
		half = analog_motion_intent(
			boat_heading_rad=0,
			stick_x=diagonal / 2,
			stick_y=diagonal / 2,
		)
		neutral = analog_motion_intent(
			boat_heading_rad=0,
			stick_x=ANALOG_STICK_DEADZONE / 2,
			stick_y=0,
		)

		self.assertEqual(full, MotionIntent(1, 1))
		self.assertAlmostEqual(half.forward, 0.5)
		self.assertAlmostEqual(half.turn, 0.5)
		self.assertEqual(neutral, MotionIntent())


class SailingSimulationTest(unittest.TestCase):
	def test_rearward_intent_produces_sternway_when_in_irons(self):
		simulation = create_simulation()
		simulation.boat.angle = math.pi
		initial_forward = pymunk.Vec2d(-1, 0)
		intent = MotionIntent(-1, 0)

		telemetry = step(simulation, DEFAULT_ENVIRONMENT, intent)
		target_force_n = force_on_foil(
			telemetry.physics.observation.apparent_wind_m_s,
			simulation.boat.angle + target_angles(telemetry)[0],
			BOAT_PARAMETERS.air_density_kg_m3,
			area_m2=1,
		)
		self.assertLess(target_force_n.dot(initial_forward), 0)
		self.assertLessEqual(abs(target_angles(telemetry)[0]), MAX_SAIL_ANGLE_RAD)
		self.assertAlmostEqual(target_angles(telemetry)[1], 0)
		for _ in range(round(5 / DT_SECONDS) - 1):
			telemetry = step(simulation, DEFAULT_ENVIRONMENT, intent)

		self.assertLessEqual(abs(target_angles(telemetry)[0]), MAX_SAIL_ANGLE_RAD)
		self.assertLess(simulation.boat.position.dot(initial_forward), -0.1)

	def test_draw_renders_intent_and_telemetry(self):
		simulation = create_simulation()
		intent = MotionIntent(1, 0)
		telemetry = step(simulation, DEFAULT_ENVIRONMENT, intent)
		pygame.font.init()
		screen = pygame.Surface((1280, 720))

		draw(
			screen,
			pygame.font.Font(None, 24),
			simulation,
			DEFAULT_ENVIRONMENT,
			intent,
			telemetry,
		)

		self.assertNotEqual(screen.get_at((640, 360)), (30, 30, 40, 255))

	def test_draw_renders_direct_control_landscapes_without_a_target(self):
		simulation = create_simulation()
		intent = MotionIntent(1, 0)
		telemetry = step(
			simulation,
			DEFAULT_ENVIRONMENT,
			intent,
			control_mode=controller.DIRECT_GRADIENT_CONTROL,
		)
		pygame.font.init()
		screen = pygame.Surface((1280, 720))

		draw(
			screen,
			pygame.font.Font(None, 24),
			simulation,
			DEFAULT_ENVIRONMENT,
			intent,
			telemetry,
		)

		self.assertNotEqual(screen.get_at((800, 280)), (30, 30, 40, 255))

	def test_neutral_landscape_does_not_flicker_when_controls_reach_the_optimum(self):
		simulation = create_simulation()
		maximum_errors = []
		for frame in range(round(6 / DT_SECONDS)):
			telemetry = step(
				simulation,
				DEFAULT_ENVIRONMENT,
				MotionIntent(),
				control_mode=controller.DIRECT_GRADIENT_CONTROL,
			)
			if frame >= round(3 / DT_SECONDS):
				maximum_errors.append(telemetry.decision.landscape.errors.max())

		self.assertLess(max(maximum_errors), 8)

	def test_step_returns_force_and_flow_telemetry(self):
		simulation = create_simulation()
		intent = MotionIntent(1, 0)

		telemetry = step(simulation, DEFAULT_ENVIRONMENT, intent)

		self.assertIsInstance(telemetry, SimulationTelemetry)
		self.assertTrue(
			all(map(math.isfinite, telemetry.physics.observation.sail_load.force_n))
		)
		self.assertTrue(
			all(map(math.isfinite, telemetry.physics.observation.rudder_load.force_n))
		)

	def test_partial_intent_changes_actuator_targets(self):
		boat_angle_rad = 1
		apparent_wind = pymunk.Vec2d(6, 0)
		apparent_water = pymunk.Vec2d(-1, 0).rotated(boat_angle_rad)
		partial_intent = MotionIntent(0.5, 0.25)
		full_intent = MotionIntent(1, 0.5)

		partial_targets = intended_actuator_angles(
			boat_angle_rad,
			boat_angular_velocity_rad_s=0,
			intent=partial_intent,
			apparent_wind_m_s=apparent_wind,
			apparent_water_m_s=apparent_water,
		)
		full_targets = intended_actuator_angles(
			boat_angle_rad,
			boat_angular_velocity_rad_s=0,
			intent=full_intent,
			apparent_wind_m_s=apparent_wind,
			apparent_water_m_s=apparent_water,
		)

		self.assertNotEqual(partial_targets, full_targets)

	def test_lateral_intent_uses_sail_for_yaw_without_backwinding(self):
		boat_angle_rad = 1
		apparent_wind = pymunk.Vec2d(6, 0)
		apparent_water = pymunk.Vec2d(-2, 0).rotated(boat_angle_rad)
		powered_sail_angle_rad, _ = intended_actuator_angles(
			boat_angle_rad,
			boat_angular_velocity_rad_s=0,
			intent=MotionIntent(1, -1),
			apparent_wind_m_s=apparent_wind,
			apparent_water_m_s=apparent_water,
		)
		sharp_sail_angle_rad, _ = intended_actuator_angles(
			boat_angle_rad,
			boat_angular_velocity_rad_s=0,
			intent=MotionIntent(0, -1),
			apparent_wind_m_s=apparent_wind,
			apparent_water_m_s=apparent_water,
		)

		sharp_sail_force_n = force_on_foil(
			apparent_wind,
			boat_angle_rad + sharp_sail_angle_rad,
			BOAT_PARAMETERS.air_density_kg_m3,
			area_m2=1,
		)
		boat_forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)
		self.assertNotEqual(sharp_sail_angle_rad, powered_sail_angle_rad)
		self.assertGreaterEqual(sharp_sail_force_n.dot(boat_forward), 0)

	def test_rear_diagonal_intent_permits_backwinding(self):
		boat_angle_rad = 1
		apparent_wind = pymunk.Vec2d(6, 0)
		diagonal_sail_angle_rad, _ = intended_actuator_angles(
			boat_angle_rad,
			boat_angular_velocity_rad_s=0,
			intent=MotionIntent(-1, -1),
			apparent_wind_m_s=apparent_wind,
			apparent_water_m_s=pymunk.Vec2d(-2, 0).rotated(boat_angle_rad),
		)

		boat_forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)
		diagonal_force_n = force_on_foil(
			apparent_wind,
			boat_angle_rad + diagonal_sail_angle_rad,
			BOAT_PARAMETERS.air_density_kg_m3,
			area_m2=1,
		).dot(boat_forward)
		self.assertLess(diagonal_force_n, 0)

	def test_keyboard_turn_modes_trade_speed_for_tighter_paths(self):
		def average_turn_radius(keys: str) -> float:
			simulation = create_simulation()
			simulation.boat.velocity = pymunk.Vec2d(2, 0).rotated(
				simulation.boat.angle
			)
			initial_angle_rad = simulation.boat.angle
			previous_position = simulation.boat.position
			distance_m = 0.0
			for _ in range(round(5 / DT_SECONDS)):
				intent = keyboard_motion_intent(
					forward="w" in keys,
					backward="s" in keys,
					left="a" in keys,
					right="d" in keys,
				)
				step(simulation, DEFAULT_ENVIRONMENT, intent)
				distance_m += (simulation.boat.position - previous_position).length
				previous_position = simulation.boat.position
			turn_rad = abs(simulation.boat.angle - initial_angle_rad)
			return distance_m / turn_rad

		for turn_key in ("a", "d"):
			with self.subTest(turn_key=turn_key):
				powered_radius_m = average_turn_radius("w" + turn_key)
				sharp_radius_m = average_turn_radius(turn_key)
				braking_radius_m = average_turn_radius("s" + turn_key)

				self.assertGreater(powered_radius_m, sharp_radius_m)
				self.assertGreater(sharp_radius_m, braking_radius_m)

	def test_direct_gradient_retrims_after_turning_downwind_to_the_opposite_reach(self):
		simulation = create_simulation()
		simulation.boat.angle = math.pi / 2
		simulation.boat.velocity = pymunk.Vec2d(0, 2)
		apparent_wind = apparent_flow(
			DEFAULT_ENVIRONMENT.true_wind_velocity_m_s,
			simulation.boat.velocity,
		)
		apparent_water = apparent_flow(
			DEFAULT_ENVIRONMENT.true_water_velocity_m_s,
			simulation.boat.velocity,
		)
		simulation.sail_angle_rad, simulation.rudder_angle_rad = intended_actuator_angles(
			simulation.boat.angle,
			0,
			MotionIntent(1, 0),
			apparent_wind,
			apparent_water,
		)
		initial_sail_angle_rad = simulation.sail_angle_rad
		initial_heading_rad = simulation.boat.angle
		sail_crossed = False

		for _ in range(round(20 / DT_SECONDS)):
			step(
				simulation,
				DEFAULT_ENVIRONMENT,
				MotionIntent(1, -0.2),
				control_mode=controller.DIRECT_GRADIENT_CONTROL,
			)
			sail_crossed |= simulation.sail_angle_rad * initial_sail_angle_rad < 0
			if simulation.boat.angle - initial_heading_rad <= -math.pi:
				break
		else:
			self.fail("boat did not reach the opposite beam reach")

		forward_speeds_m_s = []
		for _ in range(round(10 / DT_SECONDS)):
			telemetry = step(
				simulation,
				DEFAULT_ENVIRONMENT,
				MotionIntent(1, 0),
				control_mode=controller.DIRECT_GRADIENT_CONTROL,
			)
			forward = pymunk.Vec2d(1, 0).rotated(simulation.boat.angle)
			forward_speeds_m_s.append(simulation.boat.velocity.dot(forward))
			sail_crossed |= simulation.sail_angle_rad * initial_sail_angle_rad < 0

		self.assertTrue(sail_crossed)
		self.assertGreater(min(forward_speeds_m_s), 0)
		self.assertLess(simulation.sail_angle_rad * initial_sail_angle_rad, 0)
		self.assertGreater(
			simulation.sail_angle_rad
			* telemetry.decision.landscape.optimum_sail_angle_rad,
			0,
		)

	def test_continuous_turn_does_not_flicker_when_it_reaches_irons(self):
		simulation = create_simulation()
		rudder_angles_rad = []

		for frame in range(round(40 / DT_SECONDS)):
			step(
				simulation,
				DEFAULT_ENVIRONMENT,
				MotionIntent(1, -1),
				control_mode=controller.DIRECT_GRADIENT_CONTROL,
			)
			if frame >= round(30 / DT_SECONDS):
				rudder_angles_rad.append(simulation.rudder_angle_rad)

		rudder_travel_rad = sum(
			abs(after - before)
			for before, after in zip(rudder_angles_rad, rudder_angles_rad[1:])
		)
		self.assertLessEqual(rudder_travel_rad, 2 * MAX_RUDDER_ANGLE_RAD)

	def test_low_speed_tack_does_not_repeatedly_sweep_the_rudder(self):
		simulation = create_simulation()
		simulation.boat.velocity = pymunk.Vec2d(0.3, 0).rotated(
			simulation.boat.angle
		)
		rudder_angles_rad = []

		for _ in range(round(15 / DT_SECONDS)):
			step(
				simulation,
				DEFAULT_ENVIRONMENT,
				MotionIntent(1, 1),
				control_mode=controller.DIRECT_GRADIENT_CONTROL,
			)
			rudder_angles_rad.append(simulation.rudder_angle_rad)

		rudder_movements_rad = [
			abs(after - before)
			for before, after in zip(rudder_angles_rad, rudder_angles_rad[1:])
		]
		one_second = round(1 / DT_SECONDS)
		maximum_one_second_travel_rad = max(
			sum(rudder_movements_rad[start : start + one_second])
			for start in range(len(rudder_movements_rad) - one_second + 1)
		)
		self.assertGreater(simulation.boat.angle, math.pi)
		self.assertLessEqual(
			maximum_one_second_travel_rad,
			2 * MAX_RUDDER_ANGLE_RAD,
		)

	def test_alternating_powered_turns_complete_tacks_and_retrim(self):
		simulation = create_simulation()
		simulation.boat.velocity = pymunk.Vec2d(0.75, 0).rotated(
			simulation.boat.angle
		)
		apparent_wind = apparent_flow(
			DEFAULT_ENVIRONMENT.true_wind_velocity_m_s,
			simulation.boat.velocity,
		)
		apparent_water = apparent_flow(
			DEFAULT_ENVIRONMENT.true_water_velocity_m_s,
			simulation.boat.velocity,
		)
		simulation.sail_angle_rad, simulation.rudder_angle_rad = intended_actuator_angles(
			simulation.boat.angle,
			0,
			MotionIntent(1, 0),
			apparent_wind,
			apparent_water,
		)
		initial_heading_rad = simulation.boat.angle
		previous_sail_angle_rad = simulation.sail_angle_rad

		for turn, target_heading_rad in (
			(1, initial_heading_rad + math.pi / 2),
			(-1, initial_heading_rad),
		):
			for _ in range(round(25 / DT_SECONDS)):
				step(
					simulation,
					DEFAULT_ENVIRONMENT,
					MotionIntent(1, turn),
					control_mode=controller.DIRECT_GRADIENT_CONTROL,
				)
				heading_error_rad = simulation.boat.angle - target_heading_rad
				if heading_error_rad * turn >= 0:
					break
			else:
				self.fail("boat did not reach the opposite close-hauled heading")

			forward = pymunk.Vec2d(1, 0).rotated(simulation.boat.angle)
			self.assertGreater(simulation.boat.velocity.dot(forward), 0)
			self.assertLess(
				simulation.sail_angle_rad * previous_sail_angle_rad,
				0,
			)
			previous_sail_angle_rad = simulation.sail_angle_rad

	def test_forward_intent_countersteers_existing_rotation(self):
		boat_angle_rad = 1
		intent = MotionIntent(1, 0)
		apparent_water = pymunk.Vec2d(-2, 0).rotated(boat_angle_rad)
		_, stationary_rudder_angle_rad = intended_actuator_angles(
			boat_angle_rad,
			boat_angular_velocity_rad_s=0,
			intent=intent,
			apparent_wind_m_s=pymunk.Vec2d(6, 0),
			apparent_water_m_s=apparent_water,
		)
		_, rotating_rudder_angle_rad = intended_actuator_angles(
			boat_angle_rad,
			boat_angular_velocity_rad_s=0.2,
			intent=intent,
			apparent_wind_m_s=pymunk.Vec2d(6, 0),
			apparent_water_m_s=apparent_water,
		)

		self.assertNotEqual(rotating_rudder_angle_rad, stationary_rudder_angle_rad)


class ModelAndActuatorTest(unittest.TestCase):
	def test_plot_uses_the_simulation_coefficients(self):
		self.assertEqual(plot.lift(0), lift_coefficient(0))
		self.assertEqual(plot.drag(0), drag_coefficient(0))

	def test_apparent_flow_is_medium_velocity_relative_to_boat(self):
		medium_velocity = pymunk.Vec2d(6, 1)
		boat_velocity = pymunk.Vec2d(2, -1)

		self.assertEqual(apparent_flow(medium_velocity, boat_velocity), (4, 2))

	def test_foil_force_uses_fluid_density_area_and_velocity(self):
		flow = pymunk.Vec2d(10, 0)
		area_m2 = 2

		force = force_on_foil(
			flow,
			foil_angle_rad=math.pi,
			fluid_density_kg_m3=BOAT_PARAMETERS.air_density_kg_m3,
			area_m2=area_m2,
		)

		dynamic_pressure = 0.5 * BOAT_PARAMETERS.air_density_kg_m3 * flow.length**2
		expected_drag = (
			dynamic_pressure * area_m2 * BOAT_PARAMETERS.foil_base_drag_coefficient
		)
		self.assertAlmostEqual(force.x, expected_drag)
		self.assertAlmostEqual(force.y, 0)

	def test_trimmed_sail_produces_visible_progress(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=10)

		self.assertGreater(simulation.boat.position.length, 1)
		self.assertTrue(all(map(math.isfinite, simulation.boat.position)))
		self.assertTrue(all(map(math.isfinite, simulation.boat.velocity)))

	def test_sailor_force_is_limited(self):
		torque = actuator_control_torque(
			angle_rad=0,
			angular_velocity_rad_s=0,
			target_angle_rad=math.pi / 2,
			external_torque_n_m=0,
			moment_of_inertia_kg_m2=1,
			bearing_damping_n_m_s_rad=0,
			max_torque_n_m=75,
			dt_seconds=DT_SECONDS,
		)

		self.assertEqual(torque, 75)

	def test_sailor_balances_supported_wind_load_at_target(self):
		simulation = create_simulation()
		intent = MotionIntent(1, 0)
		apparent_wind = apparent_flow(
			DEFAULT_ENVIRONMENT.true_wind_velocity_m_s,
			simulation.boat.velocity,
		)
		apparent_water = apparent_flow(
			DEFAULT_ENVIRONMENT.true_water_velocity_m_s,
			simulation.boat.velocity,
		)
		target_sail_angle_rad, _ = intended_actuator_angles(
			simulation.boat.angle,
			simulation.boat.angular_velocity,
			intent,
			apparent_wind,
			apparent_water,
		)
		simulation.sail_angle_rad = target_sail_angle_rad

		step(simulation, DEFAULT_ENVIRONMENT, intent)

		self.assertAlmostEqual(simulation.sail_angular_velocity_rad_s, 0)

	def test_sailor_balances_load_at_target(self):
		torque = actuator_control_torque(
			angle_rad=0,
			angular_velocity_rad_s=0,
			target_angle_rad=0,
			external_torque_n_m=20,
			moment_of_inertia_kg_m2=1,
			bearing_damping_n_m_s_rad=0,
			max_torque_n_m=75,
			dt_seconds=DT_SECONDS,
		)

		self.assertEqual(torque, -20)

	def test_sailor_does_not_overshoot_a_target_within_one_step(self):
		target_angle_rad = 0.001
		torque = actuator_control_torque(
			angle_rad=0,
			angular_velocity_rad_s=0,
			target_angle_rad=target_angle_rad,
			external_torque_n_m=0,
			moment_of_inertia_kg_m2=1,
			bearing_damping_n_m_s_rad=0,
			max_torque_n_m=75,
			dt_seconds=DT_SECONDS,
		)
		angle_rad, _ = integrate_hinge(
			angle_rad=0,
			angular_velocity_rad_s=0,
			torque_n_m=torque,
			moment_kg_m2=1,
			bearing_damping_n_m_s_rad=0,
			max_angle_rad=1,
			dt_seconds=DT_SECONDS,
		)

		self.assertAlmostEqual(angle_rad, target_angle_rad)

	def test_sailor_cannot_balance_load_above_maximum(self):
		torque = actuator_control_torque(
			angle_rad=0,
			angular_velocity_rad_s=0,
			target_angle_rad=0,
			external_torque_n_m=100,
			moment_of_inertia_kg_m2=1,
			bearing_damping_n_m_s_rad=0,
			max_torque_n_m=75,
			dt_seconds=DT_SECONDS,
		)

		self.assertEqual(torque, -75)


class SailingDynamicsTest(unittest.TestCase):
	def test_controlled_sail_does_not_snap_to_its_target(self):
		simulation = create_simulation()
		intent = MotionIntent(1, 0)

		telemetry = step(simulation, DEFAULT_ENVIRONMENT, intent)

		self.assertNotEqual(simulation.sail_angle_rad, target_angles(telemetry)[0])
		self.assertLess(
			abs(simulation.sail_angle_rad),
			abs(target_angles(telemetry)[0]),
		)

	def test_forward_intent_resists_wind_driven_rotation(self):
		simulation = create_simulation()
		initial_heading = simulation.boat.angle

		run_simulation(simulation, seconds=10)

		heading_error = math.remainder(simulation.boat.angle - initial_heading, 2 * math.pi)
		self.assertLess(abs(heading_error), math.radians(15))

	def test_forward_intent_damps_rotation_after_a_turn(self):
		simulation = create_simulation()

		run_simulation(
			simulation,
			seconds=5,
			keys="wa",
		)
		new_heading_rad = simulation.boat.angle
		run_simulation(simulation, seconds=10, keys="w")

		heading_error = math.remainder(simulation.boat.angle - new_heading_rad, 2 * math.pi)
		self.assertLess(abs(heading_error), math.radians(15))

	def test_lateral_intent_turns_more_than_forward_intent(self):
		straight = create_simulation()
		turning = create_simulation()

		run_simulation(straight, seconds=10)
		run_simulation(turning, seconds=10, keys="a")

		self.assertGreater(
			abs(turning.boat.angular_velocity),
			abs(straight.boat.angular_velocity),
		)

	def test_rudder_turn_rate_remains_bounded(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=30, keys="a")

		self.assertLess(abs(simulation.boat.angular_velocity), 1)

	def test_trimmed_sailing_reaches_a_stable_speed(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=60)

		self.assertTrue(all(map(math.isfinite, simulation.boat.velocity)))
		self.assertLess(simulation.boat.velocity.length, 20)

	def test_neutral_intent_controls_sail_and_rudder(self):
		simulation = create_simulation()

		telemetry = step(simulation, DEFAULT_ENVIRONMENT, MotionIntent())

		self.assertIsNotNone(telemetry.decision)
		self.assertIsNotNone(telemetry.decision.target_angles_rad)

	def test_strong_wind_makes_sail_control_less_accurate(self):
		moderate = create_simulation()
		strong = create_simulation()
		moderate_environment = DEFAULT_ENVIRONMENT
		strong_environment = Environment(pymunk.Vec2d(20, 0), pymunk.Vec2d(0, 0))
		moderate_intent = MotionIntent(1, 0)
		strong_intent = MotionIntent(1, 0)

		for _ in range(round(2 / DT_SECONDS)):
			moderate_telemetry = step(moderate, moderate_environment, moderate_intent)
			strong_telemetry = step(strong, strong_environment, strong_intent)

		moderate_error = abs(
			moderate.sail_angle_rad - target_angles(moderate_telemetry)[0]
		)
		strong_error = abs(strong.sail_angle_rad - target_angles(strong_telemetry)[0])
		self.assertGreater(strong_error, moderate_error)

	def test_strong_current_makes_rudder_control_less_accurate(self):
		still = create_simulation()
		strong = create_simulation()
		still_environment = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0))
		strong_environment = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(5, 0))
		still_intent = MotionIntent(0, -1)
		strong_intent = MotionIntent(0, -1)

		for _ in range(round(0.1 / DT_SECONDS)):
			still_telemetry = step(still, still_environment, still_intent)
			strong_telemetry = step(strong, strong_environment, strong_intent)

		still_error = abs(target_angles(still_telemetry)[1] - still.rudder_angle_rad)
		strong_error = abs(target_angles(strong_telemetry)[1] - strong.rudder_angle_rad)
		self.assertGreater(strong_error, still_error)

	def test_reverse_intent_reduces_forward_speed_through_water(self):
		simulation = create_simulation()
		simulation.boat.velocity = pymunk.Vec2d(3, 0).rotated(simulation.boat.angle)
		initial_forward = simulation.boat.velocity.normalized()
		intent = MotionIntent(-1, 0)

		for _ in range(round(2 / DT_SECONDS)):
			step(simulation, DEFAULT_ENVIRONMENT, intent)

		self.assertLess(simulation.boat.velocity.dot(initial_forward), 3)


if __name__ == "__main__":
	unittest.main()
