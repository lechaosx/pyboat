from .. import boat


def step_hull(
	state: boat.SailingState,
	environment: boat.Environment,
	parameters: boat.BoatParameters,
	dt_seconds: float,
) -> boat.PhysicsTelemetry:
	observation = boat.observe(state, environment, parameters)
	keel_force_n = boat.force_on_foil(
		observation.apparent_water_m_s,
		state.boat.angle,
		parameters.water_density_kg_m3,
		parameters.keel_area_m2,
		parameters.underwater_foil_base_drag_coefficient,
	)
	hull_drag_n = boat.hull_resistance_force(
		observation.apparent_water_m_s,
		state.boat.angle,
		parameters.water_density_kg_m3,
		parameters.hull_reference_area_m2,
		parameters.hull_drag_coefficient,
		parameters.hull_lateral_drag_coefficient,
	)

	state.boat.apply_force_at_local_point(
		observation.sail_load.force_n.rotated(-state.boat.angle),
		observation.sail_load.center_local_m,
	)
	state.boat.apply_force_at_local_point(
		keel_force_n.rotated(-state.boat.angle),
		parameters.keel_position_m,
	)
	state.boat.apply_force_at_local_point(
		hull_drag_n.rotated(-state.boat.angle),
		(0, 0),
	)
	state.boat.apply_force_at_local_point(
		observation.rudder_load.force_n.rotated(-state.boat.angle),
		observation.rudder_load.center_local_m,
	)
	state.space.step(dt_seconds)
	return boat.PhysicsTelemetry(observation, keel_force_n, hull_drag_n)
