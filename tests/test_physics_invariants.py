import math
import unittest
from dataclasses import replace

import pymunk

from game import config
from sailing import boat, simulation
from sailing.controllers import arcade
from sailing.physics import coupled, curves


class ForceInvariantTest(unittest.TestCase):
	def test_each_foil_uses_its_own_caller_selected_properties(self):
		zero_lift = lambda angle_of_attack_rad: 0
		zero_drag = lambda angle_of_attack_rad, base_drag_coefficient: 0
		for foil_name, force_index in (
			("sail_foil", 0),
			("rudder_foil", 1),
			("keel_foil", 2),
		):
			with self.subTest(foil_name=foil_name):
				foil = replace(
					getattr(config.BOAT_PARAMETERS, foil_name),
					lift_coefficient=zero_lift,
					drag_coefficient=zero_drag,
				)
				parameters = replace(
					config.BOAT_PARAMETERS,
					**{foil_name: foil},
					hull_drag_coefficient=0,
					hull_lateral_drag_coefficient=0,
				)
				state = coupled.create_simulation(parameters, 0)
				for body in state.space.bodies:
					body.velocity = pymunk.Vec2d(1, 1)
				telemetry = coupled.step(
					state,
					boat.Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0)),
					parameters,
					boat.ActuatorTorques(),
					config.DT_SECONDS,
				)
				forces = (
					telemetry.observation.sail_load.force_n,
					telemetry.observation.rudder_load.force_n,
					telemetry.keel_force_n,
				)

				self.assertEqual(forces[force_index], (0, 0))
				self.assertTrue(
					all(
						force.length > 0
						for index, force in enumerate(forces)
						if index != force_index
					)
				)

	def test_coupled_observation_uses_the_physical_surface_body(self):
		state = coupled.create_simulation(config.BOAT_PARAMETERS, 0)
		assembly = state.coupled_assembly
		self.assertIsInstance(assembly, coupled.Assembly)
		assembly.sail.velocity = pymunk.Vec2d(3, 2)
		hinge_world_m = state.boat.local_to_world(
			config.BOAT_PARAMETERS.mast_position_m
		)
		center_world_m = hinge_world_m + pymunk.Vec2d(
			-config.BOAT_PARAMETERS.sail_center_of_effort_from_mast_m,
			0,
		).rotated(assembly.sail.angle)
		expected_force_n = boat.force_on_foil(
			boat.apparent_flow(
				config.ENVIRONMENT.true_wind_velocity_m_s,
				assembly.sail.velocity_at_world_point(center_world_m),
			),
			assembly.sail.angle,
			config.BOAT_PARAMETERS.air_density_kg_m3,
			config.BOAT_PARAMETERS.sail_foil,
		)

		observation = coupled.observe(
			state,
			config.ENVIRONMENT,
			config.BOAT_PARAMETERS,
		)

		self.assertAlmostEqual(
			(observation.sail_load.force_n - expected_force_n).length,
			0,
		)

	def test_underwater_foil_has_stronger_small_angle_lift_than_a_flat_plate(self):
		angle_rad = math.radians(5)

		self.assertGreater(
			config.BOAT_PARAMETERS.keel_foil.lift_coefficient(angle_rad),
			curves.flat_plate_lift(angle_rad),
		)

	def test_underwater_foil_loses_lift_after_stalling(self):
		lift = config.BOAT_PARAMETERS.keel_foil.lift_coefficient
		at_stall = lift(math.radians(15))
		past_stall = lift(math.radians(20))
		side_on = lift(math.radians(90))

		self.assertGreater(at_stall, past_stall)
		self.assertGreater(past_stall, side_on)
		self.assertAlmostEqual(side_on, 0)

	def test_offset_keel_uses_rotational_velocity(self):
		parameters = replace(
			config.BOAT_PARAMETERS,
			air_density_kg_m3=0,
			keel_position_m=(-1, 0),
			rudder_foil=replace(config.BOAT_PARAMETERS.rudder_foil, area_m2=0),
			hull_drag_coefficient=0,
			hull_lateral_drag_coefficient=0,
		)
		environment = boat.Environment(
			pymunk.Vec2d(0, 0),
			pymunk.Vec2d(0, 0),
		)
		state = coupled.create_simulation(parameters, 0)
		for body in state.space.bodies:
			body.angular_velocity = 1
		keel_velocity_m_s = state.boat.velocity_at_local_point(
			parameters.keel_position_m
		)
		expected_force_n = boat.force_on_foil(
			boat.apparent_flow(
				environment.true_water_velocity_m_s,
				keel_velocity_m_s,
			),
			state.boat.angle,
			parameters.water_density_kg_m3,
			parameters.keel_foil,
		)

		telemetry = coupled.step(
			state,
			environment,
			parameters,
			boat.ActuatorTorques(),
			config.DT_SECONDS,
		)

		self.assertAlmostEqual(
			(telemetry.keel_force_n - expected_force_n).length,
			0,
		)

	def test_configured_hull_resistance_opposes_motion_on_both_axes(self):
		force = boat.hull_resistance_force(
			pymunk.Vec2d(1, 1),
			boat_angle_rad=0,
			fluid_density_kg_m3=config.BOAT_PARAMETERS.water_density_kg_m3,
			reference_area_m2=config.BOAT_PARAMETERS.hull_reference_area_m2,
			coefficient=config.BOAT_PARAMETERS.hull_drag_coefficient,
			lateral_coefficient=(
				config.BOAT_PARAMETERS.hull_lateral_drag_coefficient
			),
		)

		self.assertGreater(force.y, 0)
		self.assertGreater(force.x, 0)

	def test_hull_resistance_is_longitudinal_in_boat_coordinates(self):
		force = boat.hull_resistance_force(
			pymunk.Vec2d(3, 4),
			boat_angle_rad=0,
			fluid_density_kg_m3=config.BOAT_PARAMETERS.water_density_kg_m3,
			reference_area_m2=config.BOAT_PARAMETERS.hull_reference_area_m2,
			coefficient=config.BOAT_PARAMETERS.hull_drag_coefficient,
		)

		self.assertGreater(force.x, 0)
		self.assertEqual(force.y, 0)

	def test_offset_keel_force_generates_hull_yaw(self):
		parameters = replace(
			config.BOAT_PARAMETERS,
			keel_position_m=(-1, 0),
			air_density_kg_m3=0,
			hull_drag_coefficient=0,
		)
		state = coupled.create_simulation(parameters, 0)
		for body in state.space.bodies:
			body.velocity = pymunk.Vec2d(0, 1)

		coupled.step(
			state,
			boat.Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0)),
			parameters,
			boat.ActuatorTorques(),
			config.DT_SECONDS,
		)

		self.assertGreater(state.boat.angular_velocity, 0)

	def test_zero_flow_produces_zero_foil_force(self):
		for foil in (
			config.BOAT_PARAMETERS.sail_foil,
			config.BOAT_PARAMETERS.keel_foil,
			config.BOAT_PARAMETERS.rudder_foil,
		):
			with self.subTest(lift_coefficient=foil.lift_coefficient.__name__):
				force = boat.force_on_foil(
					pymunk.Vec2d(0, 0),
					foil_angle_rad=1,
					fluid_density_kg_m3=config.BOAT_PARAMETERS.water_density_kg_m3,
					foil=foil,
				)

				self.assertEqual(force, (0, 0))

	def test_foil_force_never_adds_energy_relative_to_the_fluid(self):
		flow = pymunk.Vec2d(3, -2)
		for foil in (
			config.BOAT_PARAMETERS.sail_foil,
			config.BOAT_PARAMETERS.keel_foil,
			config.BOAT_PARAMETERS.rudder_foil,
		):
			for angle_deg in range(-180, 181, 15):
				with self.subTest(
					lift_coefficient=foil.lift_coefficient.__name__,
					angle_deg=angle_deg,
				):
					force = boat.force_on_foil(
						flow,
						math.radians(angle_deg),
						config.BOAT_PARAMETERS.water_density_kg_m3,
						foil,
					)
					self.assertGreaterEqual(force.dot(flow), -1e-9)

	def test_keel_force_opposes_sideways_boat_motion(self):
		state = coupled.create_simulation(
			config.BOAT_PARAMETERS,
			config.INITIAL_BOAT_ANGLE_RAD,
		)
		state.boat.angle = 0
		for body in state.space.bodies:
			body.velocity = pymunk.Vec2d(0, 1)
		velocity = pymunk.Vec2d(*state.boat.velocity)

		telemetry = coupled.step(
			state,
			boat.Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0)),
			config.BOAT_PARAMETERS,
			boat.ActuatorTorques(),
			config.DT_SECONDS,
		)

		self.assertLess(telemetry.keel_force_n.dot(velocity), 0)


class IntegrationInvariantTest(unittest.TestCase):
	def test_unforced_yaw_decay_is_timestep_stable(self):
		parameters = replace(
			config.BOAT_PARAMETERS,
			air_density_kg_m3=0,
			sail_foil=replace(config.BOAT_PARAMETERS.sail_foil, area_m2=0),
			keel_foil=replace(config.BOAT_PARAMETERS.keel_foil, area_m2=0),
			rudder_foil=replace(config.BOAT_PARAMETERS.rudder_foil, area_m2=0),
			hull_drag_coefficient=0,
		)
		environment = boat.Environment(
			pymunk.Vec2d(0, 0),
			pymunk.Vec2d(0, 0),
		)
		final_rates_rad_s = []
		for dt_seconds in (1 / 30, 1 / 60, 1 / 120):
			state = coupled.create_simulation(parameters, 0)
			for body in state.space.bodies:
				body.angular_velocity = 2
			for _ in range(round(5 / dt_seconds)):
				coupled.step(
					state,
					environment,
					parameters,
					boat.ActuatorTorques(),
					dt_seconds,
				)
			final_rates_rad_s.append(abs(state.boat.angular_velocity))

		self.assertLess(max(final_rates_rad_s), 1.95)
		self.assertLess(
			max(final_rates_rad_s) - min(final_rates_rad_s),
			0.05,
		)

	def test_coupled_actuator_torque_preserves_total_angular_momentum(self):
		parameters = replace(
			config.BOAT_PARAMETERS,
			air_density_kg_m3=0,
			water_density_kg_m3=0,
			hull_drag_coefficient=0,
			sail_bearing_damping_n_m_s_rad=0,
			rudder_bearing_damping_n_m_s_rad=0,
		)
		state = coupled.create_simulation(parameters, 0)

		coupled.step(
			state,
			boat.Environment(pymunk.Vec2d(0, 0), pymunk.Vec2d(0, 0)),
			parameters,
			boat.ActuatorTorques(sail_n_m=100),
			config.DT_SECONDS,
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
		state = simulation.create_simulation(config.SIMULATION_SETTINGS)
		for _ in range(round(5 / dt_seconds)):
			simulation.advance_surface_game(
				state,
				arcade.MotionIntent(forward=1),
				simulation.YAW_RATE_ARCADE_CONTROL,
				config.SIMULATION_SETTINGS,
				config.ENVIRONMENT,
				dt_seconds,
			)
		return state

	def test_simulation_is_similar_at_30_and_120_hz(self):
		coarse = self.simulate(1 / 30)
		fine = self.simulate(1 / 120)

		self.assertLess((coarse.boat.position - fine.boat.position).length, 0.2)
		self.assertLess((coarse.boat.velocity - fine.boat.velocity).length, 0.1)

if __name__ == "__main__":
	unittest.main()
