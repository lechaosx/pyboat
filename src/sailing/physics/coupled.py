import math
from dataclasses import dataclass

import pymunk

from .. import boat


@dataclass(frozen=True)
class Assembly:
	sail: pymunk.Body
	rudder: pymunk.Body


def create_simulation(
	parameters: boat.BoatParameters,
	initial_angle_rad: float,
	space: pymunk.Space | None = None,
	initial_position_m: tuple[float, float] = (0, 0),
) -> boat.SailingState:
	if space is None:
		space = pymunk.Space()
	hull = pymunk.Body(
		parameters.hull_mass_kg,
		boat.hull_moment_of_inertia_kg_m2(parameters),
	)
	hull.angle = initial_angle_rad
	hull.position = initial_position_m
	shape = pymunk.Poly.create_box(hull, (parameters.length_m, parameters.beam_m))
	space.add(hull, shape)
	state = boat.SailingState(space, hull)

	sail_mass_kg = parameters.sail_mass_kg + parameters.boom_mass_kg
	sail = pymunk.Body(
		sail_mass_kg,
		boat.sail_center_moment_of_inertia_kg_m2(parameters),
	)
	sail.angle = hull.angle + state.sail_angle_rad
	sail_hinge_world_m = hull.local_to_world(parameters.mast_position_m)
	sail_center_from_hinge_world_m = boat.sail_center_from_mast_m(
		parameters
	).rotated(sail.angle)
	sail.position = sail_hinge_world_m + sail_center_from_hinge_world_m
	sail.angular_velocity = (
		hull.angular_velocity + state.sail_angular_velocity_rad_s
	)
	sail.velocity = (
		hull.velocity_at_world_point(sail_hinge_world_m)
		+ sail_center_from_hinge_world_m.perpendicular()
		* sail.angular_velocity
	)

	rudder_mass_kg = parameters.rudder_mass_kg + parameters.tiller_mass_kg
	rudder = pymunk.Body(
		rudder_mass_kg,
		boat.rudder_center_moment_of_inertia_kg_m2(parameters),
	)
	rudder.angle = hull.angle + state.rudder_angle_rad
	rudder_hinge_world_m = hull.local_to_world(parameters.rudder_position_m)
	rudder_center_from_hinge_world_m = boat.rudder_center_from_hinge_m(
		parameters
	).rotated(rudder.angle)
	rudder.position = rudder_hinge_world_m + rudder_center_from_hinge_world_m
	rudder.angular_velocity = (
		hull.angular_velocity + state.rudder_angular_velocity_rad_s
	)
	rudder.velocity = (
		hull.velocity_at_world_point(rudder_hinge_world_m)
		+ rudder_center_from_hinge_world_m.perpendicular()
		* rudder.angular_velocity
	)

	constraints = (
		pymunk.PivotJoint(
			hull,
			sail,
			parameters.mast_position_m,
			-boat.sail_center_from_mast_m(parameters),
		),
		pymunk.RotaryLimitJoint(
			hull,
			sail,
			-parameters.max_sail_angle_rad,
			parameters.max_sail_angle_rad,
		),
		pymunk.DampedRotarySpring(
			hull,
			sail,
			0,
			0,
			parameters.sail_bearing_damping_n_m_s_rad,
		),
		pymunk.PivotJoint(
			hull,
			rudder,
			parameters.rudder_position_m,
			-boat.rudder_center_from_hinge_m(parameters),
		),
		pymunk.RotaryLimitJoint(
			hull,
			rudder,
			-parameters.max_rudder_angle_rad,
			parameters.max_rudder_angle_rad,
		),
		pymunk.DampedRotarySpring(
			hull,
			rudder,
			0,
			0,
			parameters.rudder_bearing_damping_n_m_s_rad,
		),
	)
	state.space.add(sail, rudder, *constraints)
	state.coupled_assembly = Assembly(sail, rudder)
	return state


def remove_simulation(state: boat.SailingState) -> None:
	assembly = state.coupled_assembly
	assert isinstance(assembly, Assembly)
	bodies = (state.boat, assembly.sail, assembly.rudder)
	constraints = tuple(
		constraint
		for constraint in state.space.constraints
		if constraint.a in bodies or constraint.b in bodies
	)
	shapes = tuple(shape for body in bodies for shape in body.shapes)
	state.space.remove(*constraints, *shapes, *bodies)


def _sync_surface_state(state: boat.SailingState, assembly: Assembly) -> None:
	state.sail_angle_rad = math.remainder(
		assembly.sail.angle - state.boat.angle,
		2 * math.pi,
	)
	state.sail_angular_velocity_rad_s = (
		assembly.sail.angular_velocity - state.boat.angular_velocity
	)
	state.rudder_angle_rad = math.remainder(
		assembly.rudder.angle - state.boat.angle,
		2 * math.pi,
	)
	state.rudder_angular_velocity_rad_s = (
		assembly.rudder.angular_velocity - state.boat.angular_velocity
	)


def synchronize(state: boat.SailingState) -> None:
	assembly = state.coupled_assembly
	assert isinstance(assembly, Assembly)
	_sync_surface_state(state, assembly)


def _surface_load(
	hull: pymunk.Body,
	surface: pymunk.Body,
	medium_velocity_m_s: pymunk.Vec2d,
	hinge_position_local_m: tuple[float, float],
	center_distance_m: float,
	fluid_density_kg_m3: float,
	foil: boat.FoilProperties,
) -> boat.HingeLoad:
	hinge_world_m = hull.local_to_world(hinge_position_local_m)
	center_world_m = hinge_world_m + pymunk.Vec2d(
		-center_distance_m,
		0,
	).rotated(surface.angle)
	apparent_flow_m_s = boat.apparent_flow(
		medium_velocity_m_s,
		surface.velocity_at_world_point(center_world_m),
	)
	force_n = boat.force_on_foil(
		apparent_flow_m_s,
		surface.angle,
		fluid_density_kg_m3,
		foil,
	)
	return boat.HingeLoad(
		force_n,
		hull.world_to_local(center_world_m),
		(center_world_m - hinge_world_m).cross(force_n),
	)


def observe(
	state: boat.SailingState,
	environment: boat.Environment,
	parameters: boat.BoatParameters,
) -> boat.BoatObservation:
	assembly = state.coupled_assembly
	assert isinstance(assembly, Assembly)
	_sync_surface_state(state, assembly)
	hull = state.boat
	return boat.BoatObservation(
		hull.angle,
		hull.angular_velocity,
		pymunk.Vec2d(*hull.velocity),
		state.sail_angle_rad,
		state.sail_angular_velocity_rad_s,
		state.rudder_angle_rad,
		state.rudder_angular_velocity_rad_s,
		boat.apparent_flow(environment.true_wind_velocity_m_s, hull.velocity),
		boat.apparent_flow(environment.true_water_velocity_m_s, hull.velocity),
		_surface_load(
			hull,
			assembly.sail,
			environment.true_wind_velocity_m_s,
			parameters.mast_position_m,
			parameters.sail_center_of_effort_from_mast_m,
			parameters.air_density_kg_m3,
			parameters.sail_foil,
		),
		_surface_load(
			hull,
			assembly.rudder,
			environment.true_water_velocity_m_s,
			parameters.rudder_position_m,
			parameters.rudder_draw_length_m / 2,
			parameters.water_density_kg_m3,
			parameters.rudder_foil,
		),
	)


def apply_forces(
	state: boat.SailingState,
	environment: boat.Environment,
	parameters: boat.BoatParameters,
	actuator_torques: boat.ActuatorTorques,
) -> boat.PhysicsTelemetry:
	assembly = state.coupled_assembly
	assert isinstance(assembly, Assembly)
	observation = observe(state, environment, parameters)
	hull = state.boat
	sail_load = observation.sail_load
	rudder_load = observation.rudder_load
	keel_apparent_water_m_s = boat.apparent_flow(
		environment.true_water_velocity_m_s,
		hull.velocity_at_local_point(parameters.keel_position_m),
	)
	keel_force_n = boat.force_on_foil(
		keel_apparent_water_m_s,
		hull.angle,
		parameters.water_density_kg_m3,
		parameters.keel_foil,
	)
	hull_drag_n = pymunk.Vec2d(0, 0)
	for position_m in (
		(-parameters.length_m / 2, 0),
		(parameters.length_m / 2, 0),
	):
		point_apparent_water_m_s = boat.apparent_flow(
			environment.true_water_velocity_m_s,
			hull.velocity_at_local_point(position_m),
		)
		force_n = boat.hull_resistance_force(
			point_apparent_water_m_s,
			hull.angle,
			parameters.water_density_kg_m3,
			parameters.hull_reference_area_m2 / 2,
			parameters.hull_drag_coefficient,
			parameters.hull_lateral_drag_coefficient,
		)
		hull.apply_force_at_local_point(
			force_n.rotated(-hull.angle),
			position_m,
		)
		hull_drag_n += force_n

	sail_center_world_m = hull.local_to_world(sail_load.center_local_m)
	rudder_center_world_m = hull.local_to_world(rudder_load.center_local_m)
	assembly.sail.apply_force_at_world_point(sail_load.force_n, sail_center_world_m)
	assembly.rudder.apply_force_at_world_point(
		rudder_load.force_n,
		rudder_center_world_m,
	)
	hull.apply_force_at_local_point(
		keel_force_n.rotated(-hull.angle),
		parameters.keel_position_m,
	)
	assembly.sail.torque += actuator_torques.sail_n_m
	hull.torque -= actuator_torques.sail_n_m
	assembly.rudder.torque += actuator_torques.rudder_n_m
	hull.torque -= actuator_torques.rudder_n_m
	return boat.PhysicsTelemetry(observation, keel_force_n, hull_drag_n)


def step(
	state: boat.SailingState,
	environment: boat.Environment,
	parameters: boat.BoatParameters,
	actuator_torques: boat.ActuatorTorques,
	dt_seconds: float,
) -> boat.PhysicsTelemetry:
	telemetry = apply_forces(
		state,
		environment,
		parameters,
		actuator_torques,
	)
	state.space.step(dt_seconds)
	synchronize(state)
	return telemetry
