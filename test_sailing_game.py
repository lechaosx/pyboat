import math
import os
import subprocess
import sys
import unittest

import pymunk
import plot

from sailing_game import (
	AIR_DENSITY_KG_M3,
	DEFAULT_ENVIRONMENT,
	DT_SECONDS,
	FOIL_BASE_DRAG_COEFFICIENT,
	apparent_flow,
	create_simulation,
	drag_coefficient,
	force_on_foil,
	lift_coefficient,
	step,
)


def run_simulation(simulation, seconds, rudder_direction=0):
	for _ in range(round(seconds / DT_SECONDS)):
		step(
			simulation,
			DEFAULT_ENVIRONMENT,
			trim=True,
			rudder_direction=rudder_direction,
		)


class StartupTest(unittest.TestCase):
	def test_import_does_not_start_game(self):
		environment = os.environ | {
			"PYGAME_HIDE_SUPPORT_PROMPT": "1",
			"SDL_AUDIODRIVER": "dummy",
			"SDL_VIDEODRIVER": "dummy",
		}

		result = subprocess.run(
			[sys.executable, "-c", "import sailing_game"],
			capture_output=True,
			env=environment,
			text=True,
			timeout=2,
		)

		self.assertEqual(result.returncode, 0, result.stderr)


class SailingSimulationTest(unittest.TestCase):
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
			fluid_density_kg_m3=AIR_DENSITY_KG_M3,
			area_m2=area_m2,
		)

		dynamic_pressure = 0.5 * AIR_DENSITY_KG_M3 * flow.length**2
		expected_drag = dynamic_pressure * area_m2 * FOIL_BASE_DRAG_COEFFICIENT
		self.assertAlmostEqual(force.x, expected_drag)
		self.assertAlmostEqual(force.y, 0)

	def test_trimmed_sail_produces_visible_progress(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=10)

		self.assertGreater(simulation.boat.position.length, 1)
		self.assertTrue(all(map(math.isfinite, simulation.boat.position)))
		self.assertTrue(all(map(math.isfinite, simulation.boat.velocity)))

	def test_rudder_changes_heading(self):
		straight = create_simulation()
		turning = create_simulation()

		run_simulation(straight, seconds=10)
		run_simulation(turning, seconds=10, rudder_direction=1)

		self.assertGreater(abs(turning.boat.angle - straight.boat.angle), 0.25)

	def test_rudder_turn_rate_remains_bounded(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=30, rudder_direction=1)

		self.assertLess(abs(simulation.boat.angular_velocity), 1)

	def test_trimmed_sailing_reaches_a_stable_speed(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=60)

		self.assertTrue(all(map(math.isfinite, simulation.boat.velocity)))
		self.assertLess(simulation.boat.velocity.length, 20)


if __name__ == "__main__":
	unittest.main()
