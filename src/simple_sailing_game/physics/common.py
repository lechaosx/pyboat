import pymunk

from .. import boat


def step_hull(
	state: boat.SailingState,
	environment: boat.Environment,
	parameters: boat.BoatParameters,
	dt_seconds: float,
) -> boat.PhysicsTelemetry:
	observation = boat.observe(state, environment, parameters)
	keel_apparent_water_m_s = boat.apparent_flow(
		environment.true_water_velocity_m_s,
		state.boat.velocity_at_local_point(parameters.keel_position_m),
	)
	keel_force_n = boat.force_on_foil(
		keel_apparent_water_m_s,
		state.boat.angle,
		parameters.water_density_kg_m3,
		parameters.keel_area_m2,
		parameters.underwater_foil_base_drag_coefficient,
	)
	hull_drag_n = pymunk.Vec2d(0, 0)
	for position_m in (
		(-parameters.length_m / 2, 0),
		(parameters.length_m / 2, 0),
	):
		apparent_water_m_s = boat.apparent_flow(
			environment.true_water_velocity_m_s,
			state.boat.velocity_at_local_point(position_m),
		)
		force_n = boat.hull_resistance_force(
			apparent_water_m_s,
			state.boat.angle,
			parameters.water_density_kg_m3,
			parameters.hull_reference_area_m2 / 2,
			parameters.hull_drag_coefficient,
			parameters.hull_lateral_drag_coefficient,
		)
		state.boat.apply_force_at_local_point(
			force_n.rotated(-state.boat.angle),
			position_m,
		)
		hull_drag_n += force_n

	state.boat.apply_force_at_local_point(
		observation.sail_load.force_n.rotated(-state.boat.angle),
		observation.sail_load.center_local_m,
	)
	state.boat.apply_force_at_local_point(
		keel_force_n.rotated(-state.boat.angle),
		parameters.keel_position_m,
	)
	state.boat.apply_force_at_local_point(
		observation.rudder_load.force_n.rotated(-state.boat.angle),
		observation.rudder_load.center_local_m,
	)
	state.space.step(dt_seconds)
	return boat.PhysicsTelemetry(observation, keel_force_n, hull_drag_n)
