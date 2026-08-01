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
	Environment,
	FOIL_BASE_DRAG_COEFFICIENT,
	MAX_RUDDER_ANGLE_RAD,
	SailorIntent,
	actuator_control_torque,
	apparent_flow,
	create_simulation,
	drag_coefficient,
	force_on_foil,
	lift_coefficient,
	optimal_sail_angle,
	step,
)


def run_simulation(simulation, seconds, intent=SailorIntent(forward=True)):
	for _ in range(round(seconds / DT_SECONDS)):
		step(simulation, DEFAULT_ENVIRONMENT, intent)


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

	def test_sailor_force_is_limited(self):
		torque = actuator_control_torque(
			angle_rad=0,
			angular_velocity_rad_s=0,
			target_angle_rad=math.pi / 2,
			stiffness_n_m_rad=1000,
			damping_n_m_s_rad=0,
			max_torque_n_m=75,
		)

		self.assertEqual(torque, 75)

	def test_controlled_rudder_does_not_snap_to_its_target(self):
		simulation = create_simulation()
		maximum_rate_rad_s = 0.0

		for _ in range(round(30 / DT_SECONDS)):
			step(simulation, DEFAULT_ENVIRONMENT, SailorIntent(forward=True))
			maximum_rate_rad_s = max(
				maximum_rate_rad_s,
				abs(simulation.rudder_angular_velocity_rad_s),
			)

		self.assertLess(maximum_rate_rad_s, math.radians(120))

	def test_controlled_sail_does_not_snap_to_its_target(self):
		simulation = create_simulation()
		maximum_rate_rad_s = 0.0
		still_air_and_water = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0))

		for _ in range(round(5 / DT_SECONDS)):
			step(simulation, still_air_and_water, SailorIntent(forward=True))
			maximum_rate_rad_s = max(
				maximum_rate_rad_s,
				abs(simulation.sail_angular_velocity_rad_s),
			)

		self.assertLess(maximum_rate_rad_s, math.radians(180))

	def test_forward_intent_holds_the_initial_heading(self):
		simulation = create_simulation()
		initial_heading = simulation.boat.angle

		run_simulation(simulation, seconds=10)

		heading_error = math.remainder(simulation.boat.angle - initial_heading, 2 * math.pi)
		self.assertLess(abs(heading_error), math.radians(15))

	def test_forward_intent_holds_the_new_heading_after_a_turn(self):
		simulation = create_simulation()

		run_simulation(
			simulation,
			seconds=5,
			intent=SailorIntent(forward=True, turn=1),
		)
		new_heading_rad = simulation.boat.angle
		run_simulation(simulation, seconds=10, intent=SailorIntent(forward=True))

		heading_error = math.remainder(simulation.boat.angle - new_heading_rad, 2 * math.pi)
		self.assertLess(abs(heading_error), math.radians(15))

	def test_rudder_changes_heading(self):
		straight = create_simulation()
		turning = create_simulation()

		run_simulation(straight, seconds=10)
		run_simulation(turning, seconds=10, intent=SailorIntent(turn=1))

		self.assertGreater(abs(turning.boat.angle - straight.boat.angle), 0.25)

	def test_rudder_turn_rate_remains_bounded(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=30, intent=SailorIntent(turn=1))

		self.assertLess(abs(simulation.boat.angular_velocity), 1)

	def test_trimmed_sailing_reaches_a_stable_speed(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=60)

		self.assertTrue(all(map(math.isfinite, simulation.boat.velocity)))
		self.assertLess(simulation.boat.velocity.length, 20)

	def test_uncontrolled_sail_moves_under_wind_load(self):
		simulation = create_simulation()
		simulation.sail_angle_rad = math.radians(60)

		run_simulation(simulation, seconds=2, intent=SailorIntent())

		self.assertGreater(abs(simulation.sail_angle_rad - math.radians(60)), math.radians(5))

	def test_uncontrolled_rudder_moves_under_water_load(self):
		simulation = create_simulation()
		simulation.boat.velocity = (3, 0)
		simulation.rudder_angle_rad = math.radians(20)
		initial_angle_rad = simulation.rudder_angle_rad
		still_air_and_water = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0))

		for _ in range(round(2 / DT_SECONDS)):
			step(simulation, still_air_and_water, SailorIntent())

		self.assertGreater(abs(simulation.rudder_angle_rad - initial_angle_rad), math.radians(5))

	def test_strong_wind_makes_sail_control_less_accurate(self):
		moderate = create_simulation()
		strong = create_simulation()
		moderate_environment = DEFAULT_ENVIRONMENT
		strong_environment = Environment(pymunk.Vec2d(20, 0), pymunk.Vec2d(0, 0))

		for _ in range(round(2 / DT_SECONDS)):
			step(moderate, moderate_environment, SailorIntent(forward=True))
			step(strong, strong_environment, SailorIntent(forward=True))

		moderate_target = optimal_sail_angle(
			moderate.boat.angle,
			apparent_flow(moderate_environment.true_wind_velocity_m_s, moderate.boat.velocity),
		)
		strong_target = optimal_sail_angle(
			strong.boat.angle,
			apparent_flow(strong_environment.true_wind_velocity_m_s, strong.boat.velocity),
		)
		moderate_error = abs(moderate.sail_angle_rad - moderate_target)
		strong_error = abs(strong.sail_angle_rad - strong_target)
		self.assertGreater(strong_error, moderate_error)

	def test_strong_current_makes_rudder_control_less_accurate(self):
		still = create_simulation()
		strong = create_simulation()
		still_environment = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0))
		strong_environment = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(5, 0))

		for _ in range(round(1 / DT_SECONDS)):
			step(still, still_environment, SailorIntent(turn=1))
			step(strong, strong_environment, SailorIntent(turn=1))

		still_error = abs(MAX_RUDDER_ANGLE_RAD - still.rudder_angle_rad)
		strong_error = abs(MAX_RUDDER_ANGLE_RAD - strong.rudder_angle_rad)
		self.assertGreater(strong_error, still_error)

	def test_stop_intent_reduces_speed_through_water(self):
		simulation = create_simulation()
		simulation.boat.velocity = (3, 0)
		still_air_and_water = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0))

		for _ in range(round(10 / DT_SECONDS)):
			step(simulation, still_air_and_water, SailorIntent(stop=True))

		self.assertLess(simulation.boat.velocity.length, 1)


if __name__ == "__main__":
	unittest.main()
