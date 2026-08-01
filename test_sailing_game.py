import math
import os
import subprocess
import sys
import unittest

import pymunk
import pygame
import plot

from sailing_game import (
	AIR_DENSITY_KG_M3,
	ANALOG_STICK_DEADZONE,
	DEFAULT_ENVIRONMENT,
	DT_SECONDS,
	Environment,
	FOIL_BASE_DRAG_COEFFICIENT,
	MAX_SAIL_ANGLE_RAD,
	MotionIntent,
	SimulationTelemetry,
	actuator_control_torque,
	analog_motion_intent,
	apparent_flow,
	create_simulation,
	drag_coefficient,
	draw,
	force_on_foil,
	intended_actuator_angles,
	intent_vector,
	keyboard_motion_intent,
	lift_coefficient,
	luffing_sail_angle,
	optimal_sail_angle,
	resolve_motion_intent,
	step,
)


def run_simulation(simulation, seconds, keys="w"):
	for _ in range(round(seconds / DT_SECONDS)):
		intent = keyboard_motion_intent(
			simulation.boat.angle,
			forward="w" in keys,
			backward="s" in keys,
			left="a" in keys,
			right="d" in keys,
			brake=" " in keys,
		)
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


class InputAdapterTest(unittest.TestCase):
	def test_intent_vector_contains_heading_and_throttle(self):
		vector = intent_vector(MotionIntent(math.pi / 2, throttle=0.5))

		self.assertAlmostEqual(vector.x, 0)
		self.assertAlmostEqual(vector.y, 0.5)

	def test_keyboard_turn_points_perpendicular_to_current_heading(self):
		intent = keyboard_motion_intent(
			boat_heading_rad=1,
			forward=False,
			backward=False,
			left=True,
			right=False,
			brake=False,
		)

		self.assertAlmostEqual(intent.direction_world_rad, 1 - math.pi / 2)
		self.assertEqual(intent.throttle, 1)
		self.assertFalse(intent.brake)

		next_intent = keyboard_motion_intent(
			boat_heading_rad=0.5,
			forward=False,
			backward=False,
			left=True,
			right=False,
			brake=False,
		)
		self.assertAlmostEqual(
			next_intent.direction_world_rad,
			0.5 - math.pi / 2,
		)

	def test_keyboard_diagonal_is_a_full_boat_relative_vector(self):
		intent = keyboard_motion_intent(
			boat_heading_rad=0.5,
			forward=True,
			backward=False,
			left=True,
			right=False,
			brake=False,
		)

		self.assertAlmostEqual(intent.direction_world_rad, 0.5 - math.pi / 4)
		self.assertEqual(intent.throttle, 1)

	def test_keyboard_forward_follows_current_heading(self):
		keyboard_motion_intent(
			boat_heading_rad=1,
			forward=True,
			backward=False,
			left=False,
			right=False,
			brake=False,
		)
		intent = keyboard_motion_intent(
			boat_heading_rad=0.5,
			forward=True,
			backward=False,
			left=False,
			right=False,
			brake=False,
		)

		self.assertAlmostEqual(intent.direction_world_rad, 0.5)

	def test_keyboard_opposing_motion_and_brake_are_distinct(self):
		neutral = keyboard_motion_intent(
			boat_heading_rad=1,
			forward=False,
			backward=False,
			left=False,
			right=False,
			brake=False,
		)
		backwind = keyboard_motion_intent(
			boat_heading_rad=1,
			forward=False,
			backward=True,
			left=False,
			right=False,
			brake=False,
		)
		braking = keyboard_motion_intent(
			boat_heading_rad=1,
			forward=False,
			backward=False,
			left=False,
			right=False,
			brake=True,
		)
		cancelled = keyboard_motion_intent(
			boat_heading_rad=1,
			forward=True,
			backward=True,
			left=False,
			right=False,
			brake=False,
		)

		self.assertEqual(neutral, MotionIntent())
		self.assertIsNone(neutral.direction_world_rad)
		self.assertEqual(backwind.throttle, 1)
		self.assertAlmostEqual(
			abs(math.remainder(backwind.direction_world_rad - 1, 2 * math.pi)),
			math.pi,
		)
		self.assertFalse(backwind.brake)
		self.assertTrue(braking.brake)
		self.assertEqual(braking.direction_world_rad, 1)
		self.assertEqual(cancelled, MotionIntent())

	def test_analog_stick_uses_world_direction_and_amplitude(self):
		forward = analog_motion_intent(
			boat_heading_rad=1,
			stick_x=0,
			stick_y=-1,
			brake=False,
		)
		left = analog_motion_intent(
			boat_heading_rad=1,
			stick_x=-1,
			stick_y=0,
			brake=False,
		)
		neutral = analog_motion_intent(
			boat_heading_rad=1,
			stick_x=ANALOG_STICK_DEADZONE / 2,
			stick_y=0,
			brake=False,
		)

		self.assertAlmostEqual(forward.direction_world_rad, -math.pi / 2)
		self.assertEqual(forward.throttle, 1)
		self.assertAlmostEqual(
			abs(left.direction_world_rad),
			math.pi,
		)
		self.assertEqual(left.throttle, 1)
		self.assertEqual(neutral, MotionIntent())


class SailingSimulationTest(unittest.TestCase):
	def test_rearward_motion_intent_keeps_bow_forward_and_backwinds(self):
		boat_heading_rad = 1
		intent = MotionIntent(
			math.remainder(boat_heading_rad + math.pi, 2 * math.pi),
			throttle=1,
		)

		desired_boat_heading_rad, drive_direction = resolve_motion_intent(
			boat_heading_rad,
			intent,
		)

		self.assertAlmostEqual(desired_boat_heading_rad, boat_heading_rad)
		self.assertEqual(drive_direction, -1)

	def test_rearward_intent_produces_sternway_when_in_irons(self):
		simulation = create_simulation()
		simulation.boat.angle = math.pi
		initial_forward = pymunk.Vec2d(-1, 0)
		intent = MotionIntent(direction_world_rad=0, throttle=1)

		telemetry = step(simulation, DEFAULT_ENVIRONMENT, intent)
		self.assertGreater(abs(telemetry.target_sail_angle_rad), math.radians(80))
		self.assertLessEqual(abs(telemetry.target_sail_angle_rad), MAX_SAIL_ANGLE_RAD)
		self.assertAlmostEqual(telemetry.target_rudder_angle_rad, 0)
		for _ in range(round(5 / DT_SECONDS) - 1):
			telemetry = step(simulation, DEFAULT_ENVIRONMENT, intent)

		self.assertLessEqual(abs(telemetry.target_sail_angle_rad), MAX_SAIL_ANGLE_RAD)
		self.assertLess(simulation.boat.position.dot(initial_forward), -0.1)

	def test_draw_renders_intent_and_telemetry(self):
		simulation = create_simulation()
		intent = MotionIntent(simulation.boat.angle, throttle=1)
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

	def test_step_returns_force_and_flow_telemetry(self):
		simulation = create_simulation()
		intent = MotionIntent(simulation.boat.angle, throttle=1)

		telemetry = step(simulation, DEFAULT_ENVIRONMENT, intent)

		self.assertIsInstance(telemetry, SimulationTelemetry)
		self.assertTrue(all(map(math.isfinite, telemetry.sail_force_n)))
		self.assertTrue(all(map(math.isfinite, telemetry.rudder_force_n)))

	def test_partial_throttle_blends_luffing_and_driving_sail_angles(self):
		boat_angle_rad = 1
		apparent_wind = pymunk.Vec2d(6, 0)
		intent = MotionIntent(boat_angle_rad, throttle=0.5)

		sail_angle_rad, _ = intended_actuator_angles(
			boat_angle_rad,
			boat_angular_velocity_rad_s=0,
			intent=intent,
			apparent_wind_m_s=apparent_wind,
			apparent_water_m_s=pymunk.Vec2d(0, 0),
		)

		luffing_angle_rad = luffing_sail_angle(boat_angle_rad, apparent_wind)
		driving_angle_rad = optimal_sail_angle(boat_angle_rad, apparent_wind)
		expected_angle_rad = luffing_angle_rad + math.remainder(
			driving_angle_rad - luffing_angle_rad,
			math.pi,
		) * 0.5
		self.assertAlmostEqual(sail_angle_rad, expected_angle_rad)

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
		intent = MotionIntent(simulation.boat.angle, throttle=1)

		for _ in range(round(30 / DT_SECONDS)):
			step(simulation, DEFAULT_ENVIRONMENT, intent)
			maximum_rate_rad_s = max(
				maximum_rate_rad_s,
				abs(simulation.rudder_angular_velocity_rad_s),
			)

		self.assertLess(maximum_rate_rad_s, math.radians(120))

	def test_controlled_sail_does_not_snap_to_its_target(self):
		simulation = create_simulation()
		maximum_rate_rad_s = 0.0
		still_air_and_water = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0))
		intent = MotionIntent(simulation.boat.angle, throttle=1)

		for _ in range(round(5 / DT_SECONDS)):
			step(simulation, still_air_and_water, intent)
			maximum_rate_rad_s = max(
				maximum_rate_rad_s,
				abs(simulation.sail_angular_velocity_rad_s),
			)

		self.assertLess(maximum_rate_rad_s, math.radians(180))

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

	def test_rudder_changes_heading(self):
		straight = create_simulation()
		turning = create_simulation()

		run_simulation(straight, seconds=10)
		run_simulation(turning, seconds=10, keys="a")

		self.assertGreater(abs(turning.boat.angle - straight.boat.angle), 0.25)

	def test_rudder_turn_rate_remains_bounded(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=30, keys="a")

		self.assertLess(abs(simulation.boat.angular_velocity), 1)

	def test_trimmed_sailing_reaches_a_stable_speed(self):
		simulation = create_simulation()

		run_simulation(simulation, seconds=60)

		self.assertTrue(all(map(math.isfinite, simulation.boat.velocity)))
		self.assertLess(simulation.boat.velocity.length, 20)

	def test_uncontrolled_sail_moves_under_wind_load(self):
		simulation = create_simulation()
		simulation.sail_angle_rad = math.radians(60)

		run_simulation(simulation, seconds=2, keys="")

		self.assertGreater(abs(simulation.sail_angle_rad - math.radians(60)), math.radians(5))

	def test_uncontrolled_rudder_moves_under_water_load(self):
		simulation = create_simulation()
		simulation.boat.velocity = (3, 0)
		simulation.rudder_angle_rad = math.radians(20)
		initial_angle_rad = simulation.rudder_angle_rad
		still_air_and_water = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0))

		for _ in range(round(2 / DT_SECONDS)):
			step(simulation, still_air_and_water, MotionIntent())

		self.assertGreater(abs(simulation.rudder_angle_rad - initial_angle_rad), math.radians(5))

	def test_strong_wind_makes_sail_control_less_accurate(self):
		moderate = create_simulation()
		strong = create_simulation()
		moderate_environment = DEFAULT_ENVIRONMENT
		strong_environment = Environment(pymunk.Vec2d(20, 0), pymunk.Vec2d(0, 0))
		moderate_intent = MotionIntent(moderate.boat.angle, throttle=1)
		strong_intent = MotionIntent(strong.boat.angle, throttle=1)

		for _ in range(round(2 / DT_SECONDS)):
			step(moderate, moderate_environment, moderate_intent)
			step(strong, strong_environment, strong_intent)

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
		still_intent = MotionIntent(still.boat.angle - math.pi / 2, throttle=1)
		strong_intent = MotionIntent(strong.boat.angle - math.pi / 2, throttle=1)

		for _ in range(round(0.1 / DT_SECONDS)):
			still_telemetry = step(still, still_environment, still_intent)
			strong_telemetry = step(strong, strong_environment, strong_intent)

		still_error = abs(still_telemetry.target_rudder_angle_rad - still.rudder_angle_rad)
		strong_error = abs(strong_telemetry.target_rudder_angle_rad - strong.rudder_angle_rad)
		self.assertGreater(strong_error, still_error)

	def test_stop_intent_reduces_speed_through_water(self):
		simulation = create_simulation()
		simulation.boat.velocity = (3, 0)
		still_air_and_water = Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0))
		intent = MotionIntent(simulation.boat.angle, brake=True)

		for _ in range(round(10 / DT_SECONDS)):
			step(simulation, still_air_and_water, intent)

		self.assertLess(simulation.boat.velocity.length, 1)


if __name__ == "__main__":
	unittest.main()
