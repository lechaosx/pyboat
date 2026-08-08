import math
import unittest
from dataclasses import replace

import pymunk

from simple_sailing_game import boat, game, polar, simulation
from simple_sailing_game.controllers.arcade import MotionIntent
from simple_sailing_game.physics import coupled


class ReferenceDinghyAcceptanceTest(unittest.TestCase):
	@classmethod
	def setUpClass(cls):
		cls.polar_points = {
			point.heading_from_upwind_deg: point
			for point in polar.sample(
				game.SIMULATION_SETTINGS,
				game.ENVIRONMENT,
				game.DT_SECONDS,
				headings_from_upwind_deg=(15, 30, 45, 90, 120, 180),
				settling_seconds=15,
				sampling_seconds=3,
			)
		}

	def test_no_go_attempt_makes_little_upwind_progress(self):
		point = self.polar_points[15]
		course_from_upwind_deg = (
			point.actual_heading_from_upwind_deg + point.leeway_angle_deg
		)
		upwind_velocity_m_s = point.speed_through_water_m_s * math.cos(
			math.radians(course_from_upwind_deg)
		)

		self.assertLess(upwind_velocity_m_s, 0.2)
		self.assertLess(point.forward_speed_m_s, 0.3)

	def test_close_hauled_and_reach_speeds_stay_in_reference_ranges(self):
		close_hauled = self.polar_points[30]
		beam_reach = self.polar_points[90]
		broad_reach = self.polar_points[120]

		self.assertGreaterEqual(close_hauled.actual_heading_from_upwind_deg, 25)
		self.assertLess(abs(close_hauled.leeway_angle_deg), 12)
		self.assertGreaterEqual(close_hauled.speed_through_water_m_s, 0.6)
		self.assertGreaterEqual(beam_reach.speed_through_water_m_s, 2)
		self.assertGreaterEqual(broad_reach.speed_through_water_m_s, 2.2)

	def test_dead_run_retains_forward_motion(self):
		dead_run = self.polar_points[180]

		self.assertGreater(dead_run.forward_speed_m_s, 0.5)

	def test_polar_heading_error_is_visible_and_bounded(self):
		for point in self.polar_points.values():
			with self.subTest(heading=point.heading_from_upwind_deg):
				self.assertLess(abs(point.heading_error_deg), 8)

	def test_unforced_coast_down_is_monotonic(self):
		parameters = replace(
			game.BOAT_PARAMETERS,
			air_density_kg_m3=0,
			sail_area_m2=0,
			keel_area_m2=0,
			rudder_area_m2=0,
		)
		environment = boat.Environment(
			pymunk.Vec2d(0, 0),
			pymunk.Vec2d(0, 0),
		)
		state = coupled.create_simulation(parameters, 0)
		for body in state.space.bodies:
			body.velocity = pymunk.Vec2d(2, 0)
		previous_speed_m_s = state.boat.velocity.length
		for _ in range(round(5 / game.DT_SECONDS)):
			coupled.step(
				state,
				environment,
				parameters,
				boat.ActuatorTorques(),
				game.DT_SECONDS,
			)
			speed_m_s = state.boat.velocity.length
			self.assertLessEqual(speed_m_s, previous_speed_m_s + 1e-12)
			previous_speed_m_s = speed_m_s
		self.assertLess(previous_speed_m_s, 1.5)

	def test_mirrored_direct_control_produces_mirrored_motion(self):
		states = []
		for angle_rad, turn in (
			(math.radians(135), -1),
			(math.radians(-135), 1),
		):
			state = simulation.create_simulation(
				replace(
					game.SIMULATION_SETTINGS,
					initial_boat_angle_rad=angle_rad,
				)
			)
			for _ in range(round(6 / game.DT_SECONDS)):
				simulation.advance_surface_game(
					state,
					MotionIntent(forward=1, turn=turn),
					game.DIRECT_ARCADE_CONTROL,
					game.SIMULATION_SETTINGS,
					game.ENVIRONMENT,
					game.DT_SECONDS,
				)
			states.append(state)

		port, starboard = states
		self.assertAlmostEqual(port.boat.position.x, starboard.boat.position.x)
		self.assertAlmostEqual(port.boat.position.y, -starboard.boat.position.y)
		self.assertAlmostEqual(port.boat.velocity.x, starboard.boat.velocity.x)
		self.assertAlmostEqual(port.boat.velocity.y, -starboard.boat.velocity.y)
		self.assertAlmostEqual(port.boat.angle, -starboard.boat.angle)

	def test_direct_control_completes_tacks_on_both_sides(self):
		for angle_rad, turn in (
			(math.radians(135), 1),
			(math.radians(-135), -1),
		):
			with self.subTest(angle_rad=angle_rad):
				settings = replace(
					game.SIMULATION_SETTINGS,
					initial_boat_angle_rad=angle_rad,
				)
				state = simulation.create_simulation(settings)
				for body in state.space.bodies:
					body.velocity = pymunk.Vec2d(0.75, 0).rotated(state.boat.angle)
				target_heading_rad = state.boat.angle + turn * math.pi / 2

				for _ in range(round(15 / game.DT_SECONDS)):
					simulation.advance_surface_game(
						state,
						MotionIntent(forward=1, turn=turn),
						game.DIRECT_ARCADE_CONTROL,
						settings,
						game.ENVIRONMENT,
						game.DT_SECONDS,
					)
					if (state.boat.angle - target_heading_rad) * turn >= 0:
						break
				else:
					self.fail("boat did not complete the tack within 15 seconds")

				forward = pymunk.Vec2d(1, 0).rotated(state.boat.angle)
				self.assertGreater(state.boat.velocity.dot(forward), 0.5)

	def test_direct_control_turns_from_rest_without_excessive_yaw_damping(self):
		state = simulation.create_simulation(game.SIMULATION_SETTINGS)
		target_heading_rad = state.boat.angle + math.radians(135)

		for _ in range(round(28 / game.DT_SECONDS)):
			simulation.advance_surface_game(
				state,
				MotionIntent(forward=1, turn=1),
				game.DIRECT_ARCADE_CONTROL,
				game.SIMULATION_SETTINGS,
				game.ENVIRONMENT,
				game.DT_SECONDS,
			)
			if state.boat.angle >= target_heading_rad:
				break
		else:
			self.fail("boat did not turn 135 degrees from rest within 28 seconds")


if __name__ == "__main__":
	unittest.main()
