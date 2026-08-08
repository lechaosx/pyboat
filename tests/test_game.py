import inspect
import math
import os
import subprocess
import sys
import unittest
from dataclasses import replace

import pygame
import pymunk

from simple_sailing_game import boat, game, input as player_input, render, simulation
from simple_sailing_game.controllers import arcade, manual


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
				game.MANUAL_CONTROL,
				game.AUTO_SHEET_CONTROL,
				game.DIRECT_ARCADE_CONTROL,
				game.ARCADE_MOVE_CONTROL,
				game.ARCADE_CONTROL,
			},
		)
		self.assertTrue(
			all(
				key not in game.CONTROL_MODES_BY_KEY
				for key in range(pygame.K_7, pygame.K_9 + 1)
			)
		)

	def test_direct_arcade_is_the_default_controller(self):
		self.assertEqual(game.DEFAULT_CONTROL_MODE, game.DIRECT_ARCADE_CONTROL)
		self.assertEqual(
			game.CONTROL_MODES_BY_KEY[pygame.K_3],
			game.DIRECT_ARCADE_CONTROL,
		)

	def test_playable_simulation_has_no_physics_model_switch(self):
		self.assertFalse(hasattr(game, "next_physics_mode"))
		self.assertFalse(hasattr(game, "LUMPED_PHYSICS"))
		self.assertNotIn(
			"physics_mode",
			inspect.signature(simulation.advance_surface_game).parameters,
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

	def test_heading_input_preserves_world_direction_and_reverse_modifier(self):
		intent = player_input.analog_heading(
			-1,
			0.25,
			True,
			game.ANALOG_STICK_DEADZONE,
		)

		self.assertEqual(intent, arcade.HeadingIntent((-1, 0.25), True))

	def test_manual_input_is_incremental_sheet_and_direct_rudder(self):
		keyboard = player_input.keyboard_manual(True, False, True, False)
		analog = player_input.analog_manual(
			-0.5,
			-0.75,
			game.ANALOG_STICK_DEADZONE,
		)

		self.assertEqual(keyboard, manual.Input(sheet=1, rudder=-1))
		self.assertEqual(analog, manual.Input(sheet=0.75, rudder=-0.5))

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
			(game.ARCADE_CONTROL, arcade.HeadingIntent((1, 0))),
			(game.ARCADE_MOVE_CONTROL, arcade.MotionIntent(1, 0)),
			(game.DIRECT_ARCADE_CONTROL, arcade.MotionIntent(1, 0)),
		)
		for control_mode, intent in cases:
			with self.subTest(control_mode=control_mode):
				self.state = simulation.create_simulation(game.SIMULATION_SETTINGS)
				telemetry = self.advance(intent, control_mode)
				self.assertEqual(telemetry.control_mode, control_mode)
				self.assertTrue(all(map(math.isfinite, self.state.boat.velocity)))

	def test_manual_sheet_is_retained_and_rudder_is_direct(self):
		sheet_limit_angle_rad = game.BOAT_PARAMETERS.max_sail_angle_rad
		for _ in range(round(1 / game.DT_SECONDS)):
			telemetry = simulation.advance_manual_game(
				self.state,
				manual.Input(sheet=1, rudder=0.5),
				sheet_limit_angle_rad,
				game.SIMULATION_SETTINGS,
				game.ENVIRONMENT,
				game.DT_SECONDS,
			)
			sheet_limit_angle_rad = telemetry.surface_targets.sheet_limit_angle_rad

		self.assertAlmostEqual(sheet_limit_angle_rad, 0)
		self.assertAlmostEqual(
			telemetry.surface_targets.rudder_target_angle_rad,
			game.BOAT_PARAMETERS.max_rudder_angle_rad / 2,
		)

	def test_direct_arcade_forward_input_damps_a_released_turn(self):
		self.advance(arcade.MotionIntent(forward=1), game.DIRECT_ARCADE_CONTROL, 5)
		self.advance(
			arcade.MotionIntent(forward=1, turn=-1),
			game.DIRECT_ARCADE_CONTROL,
			2,
		)
		released_yaw_rate_rad_s = abs(self.state.boat.angular_velocity)
		released_heading_rad = self.state.boat.angle
		self.advance(arcade.MotionIntent(forward=1), game.DIRECT_ARCADE_CONTROL, 3)

		self.assertGreater(released_yaw_rate_rad_s, math.radians(3))
		self.assertLess(
			abs(self.state.boat.angular_velocity),
			released_yaw_rate_rad_s * 0.25,
		)
		self.assertLess(
			abs(math.degrees(self.state.boat.angle - released_heading_rad)),
			5,
		)

	def test_direct_arcade_settles_after_turning_downwind(self):
		settings = replace(
			game.SIMULATION_SETTINGS,
			initial_boat_angle_rad=math.radians(135),
		)
		self.state = simulation.create_simulation(settings)
		for body in self.state.space.bodies:
			body.velocity = pymunk.Vec2d(1.5, 0).rotated(self.state.boat.angle)
		heading_hold = arcade.HeadingHold()
		for _ in range(round(15 / game.DT_SECONDS)):
			intent = heading_hold.apply(
				self.state.boat.angle,
				arcade.MotionIntent(forward=1, turn=-1),
			)
			simulation.advance_surface_game(
				self.state,
				intent,
				game.DIRECT_ARCADE_CONTROL,
				settings,
				game.ENVIRONMENT,
				game.DT_SECONDS,
			)
			if self.state.boat.angle <= 0:
				break
		else:
			self.fail("boat did not reach downwind")

		headings_deg = []
		for step in range(round(20 / game.DT_SECONDS)):
			intent = heading_hold.apply(
				self.state.boat.angle,
				arcade.MotionIntent(forward=1),
			)
			simulation.advance_surface_game(
				self.state,
				intent,
				game.DIRECT_ARCADE_CONTROL,
				settings,
				game.ENVIRONMENT,
				game.DT_SECONDS,
			)
			if step >= round(10 / game.DT_SECONDS):
				headings_deg.append(
					math.degrees(math.remainder(self.state.boat.angle, 2 * math.pi))
				)

		self.assertLess(
			max(headings_deg) - min(headings_deg),
			settings.control_angle_resolution_deg,
		)
		self.assertLess(
			abs(headings_deg[-1] - math.degrees(intent.held_heading_rad)),
			settings.control_angle_resolution_deg,
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

	def test_heading_reverse_input_exits_irons(self):
		settings = replace(game.SIMULATION_SETTINGS, initial_boat_angle_rad=math.pi)
		self.state = simulation.create_simulation(settings)
		neutral = simulation.create_simulation(settings)
		for _ in range(round(20 / game.DT_SECONDS)):
			telemetry = simulation.advance_surface_game(
				self.state,
				arcade.HeadingIntent((-1, 0), explicit_reverse=True),
				game.ARCADE_CONTROL,
				game.SIMULATION_SETTINGS,
				game.ENVIRONMENT,
				game.DT_SECONDS,
			)
			simulation.advance_surface_game(
				neutral,
				arcade.HeadingIntent(),
				game.ARCADE_CONTROL,
				game.SIMULATION_SETTINGS,
				game.ENVIRONMENT,
				game.DT_SECONDS,
			)

		forward = pymunk.Vec2d(1, 0).rotated(self.state.boat.angle)
		self.assertIsNotNone(telemetry.surface_targets.sail_target_angle_rad)
		self.assertLess(
			self.state.boat.velocity.dot(forward),
			neutral.boat.velocity.dot(forward) - 0.05,
		)

	def test_draw_renders_each_intent_and_telemetry(self):
		telemetry = self.advance(
			arcade.MotionIntent(1, 0),
			game.DIRECT_ARCADE_CONTROL,
		)
		pygame.font.init()
		for intent in (
			arcade.MotionIntent(1, 0),
			manual.Input(sheet=1, rudder=-1),
			arcade.AutoSheetIntent(power=1, rudder=-1),
			arcade.HeadingIntent((0, -1)),
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

	def test_telemetry_contains_force_and_surface_data_without_lab_fields(self):
		telemetry = self.advance(
			arcade.MotionIntent(1, 0),
			game.DIRECT_ARCADE_CONTROL,
		)

		self.assertTrue(
			all(map(math.isfinite, telemetry.physics.observation.sail_load.force_n))
		)
		self.assertFalse(hasattr(telemetry, "decision"))
		self.assertFalse(hasattr(telemetry, "physics_mode"))


class ModelAndActuatorTest(unittest.TestCase):
	def test_sheet_never_pushes_the_sail_outward(self):
		state = simulation.create_simulation(game.SIMULATION_SETTINGS)
		control_state = simulation.surface_control_state(
			boat.observe(state, game.ENVIRONMENT, game.BOAT_PARAMETERS),
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
						external_torque_n_m=0,
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
