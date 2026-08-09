import math
import os
import subprocess
import sys
import tomllib
import unittest
from pathlib import Path


def isolated_python(code):
	return subprocess.run(
		[sys.executable, "-c", code],
		capture_output=True,
		env=os.environ | {"PYGAME_HIDE_SUPPORT_PROMPT": "1"},
		text=True,
		timeout=2,
	)


class ModuleBoundaryTest(unittest.TestCase):
	def test_development_tools_are_not_core_dependencies(self):
		configuration = tomllib.loads(
			(Path(__file__).parents[1] / "pyproject.toml").read_text()
		)

		core_dependencies = configuration["project"]["dependencies"]
		development_dependencies = configuration["dependency-groups"]["dev"]
		self.assertTrue(any(name.startswith("pygame") for name in core_dependencies))
		self.assertTrue(any(name.startswith("pymunk") for name in core_dependencies))
		self.assertFalse(any(name.startswith("matplotlib") for name in core_dependencies))
		self.assertFalse(any(name.startswith("pytest") for name in core_dependencies))
		self.assertTrue(
			any(name.startswith("matplotlib") for name in development_dependencies)
		)
		self.assertTrue(any(name.startswith("pytest") for name in development_dependencies))

	def test_project_exposes_short_commands(self):
		configuration = tomllib.loads(
			(Path(__file__).parents[1] / "pyproject.toml").read_text()
		)

		self.assertEqual(
			configuration["project"]["scripts"],
			{
				"game": "game.main:main",
				"polar": "plotting.polar:main",
				"coefficients": "plotting.coefficients:main",
			},
		)
		self.assertEqual(
			configuration["tool"]["uv"]["build-backend"]["module-name"],
			["game", "sailing", "plotting"],
		)

	def test_sailing_modules_do_not_import_pygame(self):
		result = isolated_python(
			"import sys, sailing.boat, "
			"sailing.controllers.actuator, "
			"sailing.controllers.arcade; "
			"assert 'pygame' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)

	def test_simulation_does_not_import_pygame(self):
		result = isolated_python(
			"import sys, sailing.simulation; "
			"assert 'pygame' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)

	def test_controller_does_not_depend_on_boat_or_pymunk(self):
		result = isolated_python(
			"import sys, sailing.controllers.actuator, "
			"sailing.controllers.arcade; "
			"assert 'sailing.boat' not in sys.modules; "
			"assert 'pymunk' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)

	def test_boat_does_not_depend_on_controller(self):
		result = isolated_python(
			"import sys, sailing.boat; "
			"assert 'sailing.controllers' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)

	def test_sailing_does_not_depend_on_game_or_plotting(self):
		result = isolated_python(
			"import sys, sailing.simulation; "
			"assert 'game' not in sys.modules; "
			"assert 'plotting' not in sys.modules"
		)

		self.assertEqual(result.returncode, 0, result.stderr)


class ComponentWiringTest(unittest.TestCase):
	def test_dead_ship_keeps_simulating_without_sailor_torque(self):
		from game import config
		from sailing import boat
		from sailing.physics import coupled

		state = coupled.create_simulation(
			config.BOAT_PARAMETERS,
			config.INITIAL_BOAT_ANGLE_RAD,
		)
		initial_sail_angle_rad = state.sail_angle_rad
		for _ in range(round(2 / config.DT_SECONDS)):
			coupled.step(
				state,
				config.ENVIRONMENT,
				config.BOAT_PARAMETERS,
				boat.ActuatorTorques(),
				config.DT_SECONDS,
			)

		self.assertNotEqual(state.sail_angle_rad, initial_sail_angle_rad)
		self.assertTrue(math.isfinite(state.boat.angle))

	def test_game_can_construct_different_boat_parameters(self):
		from dataclasses import replace

		from game import config
		from sailing import boat
		from sailing.physics import coupled

		heavy_parameters = replace(
			config.BOAT_PARAMETERS,
			hull_mass_kg=2 * config.BOAT_PARAMETERS.hull_mass_kg,
		)
		standard = coupled.create_simulation(config.BOAT_PARAMETERS, 0)
		heavy = coupled.create_simulation(heavy_parameters, 0)

		self.assertEqual(
			sum(body.mass for body in standard.space.bodies),
			boat.total_mass_kg(config.BOAT_PARAMETERS),
		)
		self.assertEqual(
			sum(body.mass for body in heavy.space.bodies),
			boat.total_mass_kg(heavy_parameters),
		)

	def test_each_component_mass_contributes_to_assembly_total(self):
		from dataclasses import replace

		from game import config
		from sailing import boat
		from sailing.physics import coupled

		for field in (
			"hull_mass_kg",
			"sail_mass_kg",
			"boom_mass_kg",
			"rudder_mass_kg",
			"tiller_mass_kg",
		):
			with self.subTest(field=field):
				parameters = replace(
					config.BOAT_PARAMETERS,
					**{field: getattr(config.BOAT_PARAMETERS, field) + 1},
				)
				state = coupled.create_simulation(parameters, 0)

				self.assertEqual(
					sum(body.mass for body in state.space.bodies),
					boat.total_mass_kg(config.BOAT_PARAMETERS) + 1,
				)

	def test_default_component_masses_preserve_the_250_kg_boat(self):
		from game import config
		from sailing import boat

		self.assertEqual(boat.total_mass_kg(config.BOAT_PARAMETERS), 250)


if __name__ == "__main__":
	unittest.main()
