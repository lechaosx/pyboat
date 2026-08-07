import math
import unittest
from dataclasses import replace

import pymunk

from simple_sailing_game import boat, game
from simple_sailing_game.controllers import arcade
from simple_sailing_game.physics import coupled, dynamic


class ForceInvariantTest(unittest.TestCase):
	def test_zero_flow_produces_zero_foil_force(self):
		force = boat.force_on_foil(
			pymunk.Vec2d(0, 0),
			foil_angle_rad=1,
			fluid_density_kg_m3=game.BOAT_PARAMETERS.water_density_kg_m3,
			area_m2=1,
			base_drag_coefficient=game.BOAT_PARAMETERS.foil_base_drag_coefficient,
		)

		self.assertEqual(force, (0, 0))

	def test_foil_force_never_adds_energy_relative_to_the_fluid(self):
		flow = pymunk.Vec2d(3, -2)
		for angle_deg in range(-180, 181, 15):
			with self.subTest(angle_deg=angle_deg):
				force = boat.force_on_foil(
					flow,
					math.radians(angle_deg),
					game.BOAT_PARAMETERS.water_density_kg_m3,
					area_m2=1,
					base_drag_coefficient=(
						game.BOAT_PARAMETERS.foil_base_drag_coefficient
					),
				)
				self.assertGreaterEqual(force.dot(flow), -1e-9)

	def test_keel_force_opposes_sideways_boat_motion(self):
		state = game.create_simulation()
		state.boat.angle = 0
		state.boat.velocity = pymunk.Vec2d(0, 1)
		velocity = pymunk.Vec2d(*state.boat.velocity)

		telemetry = dynamic.step(
			state,
			boat.Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0)),
			game.BOAT_PARAMETERS,
			boat.ActuatorTorques(),
			game.DT_SECONDS,
		)

		self.assertLess(telemetry.keel_force_n.dot(velocity), 0)


class IntegrationInvariantTest(unittest.TestCase):
	def test_coupled_actuator_torque_preserves_total_angular_momentum(self):
		parameters = replace(
			game.BOAT_PARAMETERS,
			air_density_kg_m3=0,
			water_density_kg_m3=0,
			hull_drag_coefficient=0,
			sail_bearing_damping_n_m_s_rad=0,
			rudder_bearing_damping_n_m_s_rad=0,
		)
		state = boat.create_simulation(parameters, 0)

		coupled.step(
			state,
			boat.Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0)),
			parameters,
			boat.ActuatorTorques(sail_n_m=100),
			game.DT_SECONDS,
		)

		angular_momentum = sum(
			body.moment * body.angular_velocity
			+ body.position.cross(body.mass * body.velocity)
			for body in state.space.bodies
		)
		self.assertEqual(len(state.space.bodies), 3)
		self.assertAlmostEqual(angular_momentum, 0, places=6)
		self.assertAlmostEqual(
			sum(body.mass for body in state.space.bodies),
			boat.total_mass_kg(parameters),
		)

	def simulate(self, dt_seconds):
		state = game.create_simulation()
		for _ in range(round(5 / dt_seconds)):
			direction = pymunk.Vec2d(1, 0).rotated(state.boat.angle)
			game.advance_surface_game(
				state,
				arcade.HeadingIntent(world_direction=tuple(direction)),
				game.ARCADE_CONTROL,
				game.COUPLED_PHYSICS,
				dt_seconds=dt_seconds,
			)
		return state

	def test_simulation_is_similar_at_30_and_120_hz(self):
		coarse = self.simulate(1 / 30)
		fine = self.simulate(1 / 120)

		self.assertLess((coarse.boat.position - fine.boat.position).length, 0.2)
		self.assertLess((coarse.boat.velocity - fine.boat.velocity).length, 0.1)

	def test_world_direction_control_aligns_the_nearest_end_on_both_plants(self):
		target_heading_rad = -math.pi / 2
		target = pymunk.Vec2d(1, 0).rotated(target_heading_rad)

		def nearest_end_error(boat_heading_rad):
			bow_error = abs(
				math.remainder(target_heading_rad - boat_heading_rad, 2 * math.pi)
			)
			stern_error = abs(
				math.remainder(
					target_heading_rad - boat_heading_rad - math.pi,
					2 * math.pi,
				)
			)
			return min(bow_error, stern_error)

		for physics_mode in (game.DYNAMIC_PHYSICS, game.COUPLED_PHYSICS):
			with self.subTest(physics_mode=physics_mode):
				state = game.create_simulation()
				initial_error_rad = nearest_end_error(state.boat.angle)
				for _ in range(round(20 / game.DT_SECONDS)):
					intent = arcade.HeadingIntent(
						world_direction=tuple(target),
					)
					game.advance_surface_game(
						state,
						intent,
						game.ARCADE_STERNWAY_CONTROL,
						physics_mode,
					)
				final_error_rad = nearest_end_error(state.boat.angle)
				self.assertLess(final_error_rad, initial_error_rad / 2)
				self.assertGreater(state.boat.velocity.dot(target), 0)


if __name__ == "__main__":
	unittest.main()
