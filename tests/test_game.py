import math
import os
import subprocess
import sys
import unittest
from dataclasses import replace

import pygame

from simple_sailing_game import boat, game, input as player_input, render, simulation
from simple_sailing_game.controllers import actuator, arcade
from simple_sailing_game.physics import coupled


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
	def test_every_controller_has_a_selection_key(self):
		self.assertEqual(
			set(game.CONTROL_MODES_BY_KEY.values()),
			{
				game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
				game.AUTO_SHEET_CONTROL,
				game.YAW_RATE_ARCADE_CONTROL,
			},
		)

	def test_computed_yaw_rate_arcade_is_the_default_controller(self):
		self.assertEqual(
			game.DEFAULT_CONTROL_MODE,
			game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
		)
		self.assertEqual(
			game.CONTROL_MODES_BY_KEY[pygame.K_1],
			game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
		)

	def test_fixed_step_schedule_carries_partial_time_between_frames(self):
		steps, remaining_seconds = game.fixed_step_schedule(0, game.DT_SECONDS * 1.5)
		self.assertEqual(steps, 1)
		self.assertAlmostEqual(remaining_seconds, game.DT_SECONDS / 2)

		steps, remaining_seconds = game.fixed_step_schedule(
			remaining_seconds,
			game.DT_SECONDS / 2,
		)
		self.assertEqual(steps, 1)
		self.assertEqual(remaining_seconds, 0)

	def test_keyboard_motion_is_full_scale_and_boat_relative(self):
		self.assertEqual(
			player_input.keyboard_motion(True, False, True, False),
			arcade.MotionIntent(1, -1),
		)
		self.assertEqual(
			player_input.keyboard_motion(False, False, False, False),
			arcade.MotionIntent(),
		)

	def test_analog_motion_converts_world_direction_to_local_square(self):
		self.assertEqual(
			player_input.analog_motion(
				math.pi / 2,
				0,
				1,
				game.ANALOG_STICK_DEADZONE,
			),
			arcade.MotionIntent(1, 0),
		)
		diagonal = 1 / math.sqrt(2)
		self.assertEqual(
			player_input.analog_motion(
				0,
				diagonal,
				diagonal,
				game.ANALOG_STICK_DEADZONE,
			),
			arcade.MotionIntent(1, 1),
		)

	def test_intent_visualization_preserves_local_square_components(self):
		vector = render.intent_vector(arcade.MotionIntent(0.5, 0.5), math.pi / 2)

		self.assertAlmostEqual(vector.x, -0.5)
		self.assertAlmostEqual(vector.y, 0.5)


class SailingSimulationTest(unittest.TestCase):
	def setUp(self):
		self.state = simulation.create_simulation(game.SIMULATION_SETTINGS)

	def advance(self, intent, control_mode, seconds=1):
		for _ in range(round(seconds / game.DT_SECONDS)):
			telemetry = simulation.advance_surface_game(
				self.state,
				intent,
				control_mode,
				game.SIMULATION_SETTINGS,
				game.ENVIRONMENT,
				game.DT_SECONDS,
			)
		return telemetry

	def test_simulation_starts_as_articulated_boat(self):
		self.assertEqual(len(self.state.space.bodies), 3)

	def test_core_surface_controllers_run_on_articulated_physics(self):
		cases = (
			(game.AUTO_SHEET_CONTROL, arcade.AutoSheetIntent(1, 0)),
			(game.COMPUTED_YAW_RATE_ARCADE_CONTROL, arcade.MotionIntent(1, 0)),
			(game.YAW_RATE_ARCADE_CONTROL, arcade.MotionIntent(1, 0)),
		)
		for control_mode, intent in cases:
			with self.subTest(control_mode=control_mode):
				self.state = simulation.create_simulation(game.SIMULATION_SETTINGS)
				telemetry = self.advance(intent, control_mode)
				self.assertEqual(telemetry.control_mode, control_mode)
				self.assertTrue(all(map(math.isfinite, self.state.boat.velocity)))

	def test_computed_yaw_rate_arcade_countersteers_a_released_turn(self):
		self.advance(
			arcade.MotionIntent(forward=1),
			game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
			5,
		)
		self.advance(
			arcade.MotionIntent(forward=1, turn=-1),
			game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
			2,
		)
		released_yaw_rate_rad_s = abs(self.state.boat.angular_velocity)
		self.advance(
			arcade.MotionIntent(forward=1),
			game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
			3,
		)

		self.assertGreater(released_yaw_rate_rad_s, math.radians(3))
		self.assertLess(
			abs(self.state.boat.angular_velocity),
			released_yaw_rate_rad_s,
		)

	def test_computed_maximum_yaw_rate_uses_rudder_flow_and_steering_baseline(self):
		for body in self.state.space.bodies:
			body.velocity = (2, 0)
		observation = coupled.observe(
			self.state,
			game.ENVIRONMENT,
			game.BOAT_PARAMETERS,
		)

		control_state = simulation.surface_control_state(
			observation,
			game.SIMULATION_SETTINGS,
		)

		steering_baseline_m = (
			pygame.Vector2(game.BOAT_PARAMETERS.rudder_position_m)
			- pygame.Vector2(game.BOAT_PARAMETERS.keel_position_m)
		).length()
		expected_yaw_rate_rad_s = (
			2
			* math.tan(game.BOAT_PARAMETERS.max_rudder_angle_rad)
			/ steering_baseline_m
		)
		self.assertAlmostEqual(
			control_state.maximum_yaw_rate_rad_s,
			expected_yaw_rate_rad_s,
		)

	def test_auto_sheet_surfaces_settle_without_chatter(self):
		angles = []
		for step_index in range(round(12 / game.DT_SECONDS)):
			self.advance(
				arcade.AutoSheetIntent(power=1),
				game.AUTO_SHEET_CONTROL,
				game.DT_SECONDS,
			)
			if step_index >= round(11 / game.DT_SECONDS):
				angles.append((self.state.sail_angle_rad, self.state.rudder_angle_rad))

		for surface_angles in zip(*angles):
			self.assertLess(
				max(surface_angles) - min(surface_angles),
				math.radians(0.1),
			)

	def test_draw_renders_each_intent_and_telemetry(self):
		telemetry = self.advance(
			arcade.MotionIntent(1, 0),
			game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
		)
		pygame.font.init()
		for intent in (
			arcade.MotionIntent(1, 0),
			arcade.AutoSheetIntent(power=1, rudder=-1),
		):
			with self.subTest(intent=type(intent).__name__):
				screen = pygame.Surface((render.WIDTH, render.HEIGHT))
				render.draw(
					screen,
					pygame.font.Font(None, 24),
					self.state,
					game.ENVIRONMENT,
					intent,
					telemetry,
					game.BOAT_PARAMETERS,
				)
				self.assertNotEqual(
					screen.get_at((render.WIDTH // 2, render.HEIGHT // 2)),
					(30, 30, 40, 255),
				)

	def test_draw_renders_current_and_maximum_yaw_rate_for_rate_controllers(self):
		class RecordingFont:
			def __init__(self):
				self.font = pygame.font.Font(None, 24)
				self.texts = []

			def render(self, text, antialias, color):
				self.texts.append(text)
				return self.font.render(text, antialias, color)

		pygame.font.init()
		for control_mode in (
			game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
			game.YAW_RATE_ARCADE_CONTROL,
		):
			with self.subTest(control_mode=control_mode):
				self.state = simulation.create_simulation(game.SIMULATION_SETTINGS)
				telemetry = self.advance(arcade.MotionIntent(1, 0), control_mode)
				font = RecordingFont()
				render.draw(
					pygame.Surface((render.WIDTH, render.HEIGHT)),
					font,
					self.state,
					game.ENVIRONMENT,
					arcade.MotionIntent(1, 0),
					telemetry,
					game.BOAT_PARAMETERS,
				)
				observation = telemetry.physics.observation
				expected = (
					f"yaw {math.degrees(observation.boat_angular_velocity_rad_s):+.1f}°/s"
					f" / {math.degrees(telemetry.maximum_yaw_rate_rad_s):.1f}°/s max"
				)
				self.assertTrue(any(expected in text for text in font.texts))

	def test_draw_uses_true_wind_for_point_of_sail(self):
		class RecordingFont:
			def __init__(self):
				self.font = pygame.font.Font(None, 24)
				self.texts = []

			def render(self, text, antialias, color):
				self.texts.append(text)
				return self.font.render(text, antialias, color)

		upwind_angle_rad = game.ENVIRONMENT.true_wind_velocity_m_s.angle + math.pi
		heading_rad = upwind_angle_rad + math.radians(135)
		settings = replace(
			game.SIMULATION_SETTINGS,
			initial_boat_angle_rad=heading_rad,
		)
		state = simulation.create_simulation(settings)
		velocity = (
			2.5 * math.cos(heading_rad),
			2.5 * math.sin(heading_rad),
		)
		for body in state.space.bodies:
			body.velocity = velocity
		telemetry = simulation.advance_surface_game(
			state,
			arcade.MotionIntent(forward=1),
			game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
			settings,
			game.ENVIRONMENT,
			game.DT_SECONDS,
		)
		pygame.font.init()
		font = RecordingFont()
		render.draw(
			pygame.Surface((render.WIDTH, render.HEIGHT)),
			font,
			state,
			game.ENVIRONMENT,
			arcade.MotionIntent(forward=1),
			telemetry,
			game.BOAT_PARAMETERS,
		)

		self.assertTrue(
			any("point of sail: broad reach" in text for text in font.texts)
		)

	def test_telemetry_contains_force_and_surface_data(self):
		telemetry = self.advance(
			arcade.MotionIntent(1, 0),
			game.COMPUTED_YAW_RATE_ARCADE_CONTROL,
		)

		self.assertTrue(
			all(map(math.isfinite, telemetry.physics.observation.sail_load.force_n))
		)


class ModelAndActuatorTest(unittest.TestCase):
	def test_actuator_torque_damps_motion_at_its_target(self):
		state = actuator.ActuatorState(
			angle_rad=0,
			angular_velocity_rad_s=1,
			moment_of_inertia_kg_m2=2,
			maximum_torque_n_m=100,
			maximum_angle_rad=math.pi / 2,
		)

		self.assertLess(actuator.damped_control_torque(state, 0), 0)

	def test_sheet_never_pushes_the_sail_outward(self):
		state = simulation.create_simulation(game.SIMULATION_SETTINGS)
		control_state = simulation.surface_control_state(
			coupled.observe(state, game.ENVIRONMENT, game.BOAT_PARAMETERS),
			game.SIMULATION_SETTINGS,
		)
		for angle_rad, outward_target_rad in (
			(0.25, 0.5),
			(-0.25, -0.5),
			(0, 0.5),
		):
			with self.subTest(angle_rad=angle_rad):
				positioned_state = replace(
					control_state,
					sail=replace(
						control_state.sail,
						angle_rad=angle_rad,
						angular_velocity_rad_s=0,
					),
				)
				torque_n_m = simulation.sheet_control_torques(
					positioned_state,
					outward_target_rad,
					0,
				).sail_n_m
				self.assertLessEqual(torque_n_m * outward_target_rad, 0)


if __name__ == "__main__":
	unittest.main()
