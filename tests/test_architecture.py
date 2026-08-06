import math
import os
import subprocess
import sys
import unittest

import pymunk


def isolated_python(code):
	return subprocess.run(
		[sys.executable, "-c", code],
		capture_output=True,
		env=os.environ | {"PYGAME_HIDE_SUPPORT_PROMPT": "1"},
		text=True,
		timeout=2,
	)


class ModuleBoundaryTest(unittest.TestCase):
	def test_internal_modules_do_not_import_pygame(self):
		result = isolated_python(
			"import sys, simple_sailing_game.boat, "
			"simple_sailing_game.controller; "
			"assert 'pygame' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)

	def test_controller_does_not_depend_on_boat_or_pymunk(self):
		result = isolated_python(
			"import sys, simple_sailing_game.controller; "
			"assert 'simple_sailing_game.boat' not in sys.modules; "
			"assert 'pymunk' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)

	def test_boat_does_not_depend_on_controller(self):
		result = isolated_python(
			"import sys, simple_sailing_game.boat; "
			"assert 'simple_sailing_game.controller' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)


class ComponentWiringTest(unittest.TestCase):
	def test_dead_ship_keeps_simulating_without_sailor_torque(self):
		from simple_sailing_game import boat, game

		state = boat.create_simulation(
			game.BOAT_PARAMETERS,
			game.INITIAL_BOAT_ANGLE_RAD,
		)
		initial_sail_angle_rad = state.sail_angle_rad
		for _ in range(round(2 / game.DT_SECONDS)):
			boat.step(
				state,
				game.ENVIRONMENT,
				game.BOAT_PARAMETERS,
				boat.ActuatorTorques(),
				game.DT_SECONDS,
			)

		self.assertNotEqual(state.sail_angle_rad, initial_sail_angle_rad)
		self.assertTrue(math.isfinite(state.boat.angle))

	def test_game_accepts_a_controller_function_or_none(self):
		from simple_sailing_game import controller, game

		calls = []

		def fixed_controller(observation, intent, dt_seconds):
			calls.append((observation, intent, dt_seconds))
			return controller.ControlOutput(controller.ActuatorTorques(1, 0), None)

		controlled = game.create_simulation()
		game.advance_game(
			controlled,
			controller.MotionIntent(1, 0),
			fixed_controller,
		)
		dead = game.create_simulation()
		game.advance_game(
			dead,
			controller.MotionIntent(1, 0),
			None,
		)

		self.assertEqual(len(calls), 1)
		self.assertEqual(calls[0][1], controller.MotionIntent(1, 0))
		self.assertNotEqual(dead.sail_angle_rad, controlled.sail_angle_rad)

	def test_controller_does_not_mutate_boat_observation(self):
		from simple_sailing_game import boat, controller, game

		state = game.create_simulation()
		observation = boat.observe(
			state,
			game.ENVIRONMENT,
			game.BOAT_PARAMETERS,
		)
		controller_input = game.controller_input(observation)
		before = controller_input

		controller.control(
			controller_input,
			controller.MotionIntent(1, -1),
			game.DT_SECONDS,
			game.CONTROLLER_SETTINGS,
		)

		self.assertEqual(controller_input, before)

	def test_game_can_construct_different_boat_parameters(self):
		from dataclasses import replace

		from simple_sailing_game import boat, game

		heavy_parameters = replace(
			game.BOAT_PARAMETERS,
			mass_kg=2 * game.BOAT_PARAMETERS.mass_kg,
		)
		standard = boat.create_simulation(game.BOAT_PARAMETERS, 0)
		heavy = boat.create_simulation(heavy_parameters, 0)

		self.assertEqual(standard.boat.mass, game.BOAT_PARAMETERS.mass_kg)
		self.assertEqual(heavy.boat.mass, heavy_parameters.mass_kg)


if __name__ == "__main__":
	unittest.main()
