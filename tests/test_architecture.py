import importlib.util
import inspect
import math
import os
import subprocess
import sys
import tomllib
import unittest
from pathlib import Path

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
	def test_optional_tools_are_not_core_dependencies(self):
		configuration = tomllib.loads(
			(Path(__file__).parents[1] / "pyproject.toml").read_text()
		)["project"]

		self.assertEqual(
			configuration["dependencies"],
			["pygame>=2.5.0", "pymunk>=6.6.0"],
		)
		self.assertEqual(
			configuration["optional-dependencies"],
			{"plots": ["matplotlib>=3.10.8"]},
		)

	def test_game_owns_runtime_simulation_configuration(self):
		from simple_sailing_game import game, simulation

		self.assertIsInstance(game.SIMULATION_SETTINGS, simulation.SimulationSettings)
		for name in (
			"BOAT_PARAMETERS",
			"ENVIRONMENT",
			"DT_SECONDS",
			"INITIAL_BOAT_ANGLE_RAD",
		):
			with self.subTest(name=name):
				self.assertFalse(hasattr(simulation, name))

	def test_simulation_entry_points_require_runtime_inputs(self):
		from simple_sailing_game import simulation

		required = {
			simulation.create_simulation: ("settings",),
			simulation.advance_surface_game: (
				"settings",
				"environment",
				"dt_seconds",
			),
		}
		for function, parameter_names in required.items():
			with self.subTest(function=function.__name__):
				parameters = inspect.signature(function).parameters
				for name in parameter_names:
					self.assertIs(parameters[name].default, inspect.Parameter.empty)

	def test_rendering_requires_boat_parameters(self):
		from simple_sailing_game import render

		parameter = inspect.signature(render.draw).parameters["parameters"]

		self.assertIs(parameter.default, inspect.Parameter.empty)

	def test_articulated_plant_is_the_only_physics_model(self):
		from simple_sailing_game.physics import coupled

		self.assertEqual(
			coupled.step.__module__,
			"simple_sailing_game.physics.coupled",
		)
		self.assertIsNone(
			importlib.util.find_spec("simple_sailing_game.physics.dynamic")
		)
		self.assertIsNone(
			importlib.util.find_spec("simple_sailing_game.physics.common")
		)
		self.assertTrue(hasattr(coupled, "create_simulation"))
		self.assertFalse(hasattr(coupled, "activate"))
		self.assertFalse(hasattr(coupled, "deactivate"))
		self.assertIsNone(
			importlib.util.find_spec("simple_sailing_game.physics.kinematic")
		)

	def test_internal_modules_do_not_import_pygame(self):
		result = isolated_python(
			"import sys, simple_sailing_game.boat, "
			"simple_sailing_game.controllers.actuator, "
			"simple_sailing_game.controllers.arcade; "
			"assert 'pygame' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)

	def test_simulation_does_not_import_pygame(self):
		result = isolated_python(
			"import sys, simple_sailing_game.simulation; "
			"assert 'pygame' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)

	def test_core_modules_do_not_import_numpy(self):
		from simple_sailing_game import game, input, render, simulation

		for module in (game, input, render, simulation):
			with self.subTest(module=module.__name__):
				self.assertNotIn("import numpy", inspect.getsource(module))

	def test_rendering_has_its_own_module(self):
		from simple_sailing_game import render

		self.assertEqual(render.draw.__module__, "simple_sailing_game.render")

	def test_controller_does_not_depend_on_boat_or_pymunk(self):
		result = isolated_python(
			"import sys, simple_sailing_game.controllers.actuator, "
			"simple_sailing_game.controllers.arcade; "
			"assert 'simple_sailing_game.boat' not in sys.modules; "
			"assert 'pymunk' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)

	def test_boat_does_not_depend_on_controller(self):
		result = isolated_python(
			"import sys, simple_sailing_game.boat; "
			"assert 'simple_sailing_game.controllers' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)


class ComponentWiringTest(unittest.TestCase):
	def test_dead_ship_keeps_simulating_without_sailor_torque(self):
		from simple_sailing_game import boat, game
		from simple_sailing_game.physics import coupled

		state = coupled.create_simulation(
			game.BOAT_PARAMETERS,
			game.INITIAL_BOAT_ANGLE_RAD,
		)
		initial_sail_angle_rad = state.sail_angle_rad
		for _ in range(round(2 / game.DT_SECONDS)):
			coupled.step(
				state,
				game.ENVIRONMENT,
				game.BOAT_PARAMETERS,
				boat.ActuatorTorques(),
				game.DT_SECONDS,
			)

		self.assertNotEqual(state.sail_angle_rad, initial_sail_angle_rad)
		self.assertTrue(math.isfinite(state.boat.angle))

	def test_game_can_construct_different_boat_parameters(self):
		from dataclasses import replace

		from simple_sailing_game import boat, game
		from simple_sailing_game.physics import coupled

		heavy_parameters = replace(
			game.BOAT_PARAMETERS,
			hull_mass_kg=2 * game.BOAT_PARAMETERS.hull_mass_kg,
		)
		standard = coupled.create_simulation(game.BOAT_PARAMETERS, 0)
		heavy = coupled.create_simulation(heavy_parameters, 0)

		self.assertEqual(
			sum(body.mass for body in standard.space.bodies),
			boat.total_mass_kg(game.BOAT_PARAMETERS),
		)
		self.assertEqual(
			sum(body.mass for body in heavy.space.bodies),
			boat.total_mass_kg(heavy_parameters),
		)

	def test_each_component_mass_contributes_to_assembly_total(self):
		from dataclasses import replace

		from simple_sailing_game import boat, game
		from simple_sailing_game.physics import coupled

		for field in (
			"hull_mass_kg",
			"sail_mass_kg",
			"boom_mass_kg",
			"rudder_mass_kg",
			"tiller_mass_kg",
		):
			with self.subTest(field=field):
				parameters = replace(
					game.BOAT_PARAMETERS,
					**{field: getattr(game.BOAT_PARAMETERS, field) + 1},
				)
				state = coupled.create_simulation(parameters, 0)

				self.assertEqual(
					sum(body.mass for body in state.space.bodies),
					boat.total_mass_kg(game.BOAT_PARAMETERS) + 1,
				)

	def test_default_component_masses_preserve_the_250_kg_boat(self):
		from simple_sailing_game import boat, game

		self.assertEqual(boat.total_mass_kg(game.BOAT_PARAMETERS), 250)


if __name__ == "__main__":
	unittest.main()
