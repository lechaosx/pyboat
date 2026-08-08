import math
from dataclasses import dataclass

import pymunk

from .. import boat


@dataclass(frozen=True)
class Assembly:
	sail: pymunk.Body
	rudder: pymunk.Body
	constraints: tuple[pymunk.Constraint, ...]


def _angular_momentum_about(
	bodies: tuple[pymunk.Body, ...],
	point_m: pymunk.Vec2d,
) -> float:
	return sum(
		body.moment * body.angular_velocity
		+ (body.position - point_m).cross(body.mass * body.velocity)
		for body in bodies
	)


def _preserve_lumped_momentum(
	bodies: tuple[pymunk.Body, ...],
	point_m: pymunk.Vec2d,
	linear_momentum_kg_m_s: pymunk.Vec2d,
	angular_momentum_kg_m2_s: float,
) -> None:
	total_mass_kg = sum(body.mass for body in bodies)
	current_linear_momentum = sum(
		(body.mass * body.velocity for body in bodies),
		pymunk.Vec2d(0, 0),
	)
	velocity_correction_m_s = (
		linear_momentum_kg_m_s - current_linear_momentum
	) / total_mass_kg
	for body in bodies:
		body.velocity += velocity_correction_m_s

	current_angular_momentum = _angular_momentum_about(bodies, point_m)
	moment_about_point_kg_m2 = sum(
		body.moment + body.mass * (body.position - point_m).length**2
		for body in bodies
	)
	angular_velocity_correction_rad_s = (
		angular_momentum_kg_m2_s - current_angular_momentum
	) / moment_about_point_kg_m2
	for body in bodies:
		center_from_point_m = body.position - point_m
		body.velocity += (
			center_from_point_m.perpendicular()
			* angular_velocity_correction_rad_s
		)
		body.angular_velocity += angular_velocity_correction_rad_s


def activate(
	state: boat.SailingState,
	parameters: boat.BoatParameters,
) -> Assembly:
	if isinstance(state.coupled_assembly, Assembly):
		return state.coupled_assembly

	hull = state.boat
	linear_momentum = hull.mass * hull.velocity
	angular_momentum = hull.moment * hull.angular_velocity
	hull.mass = parameters.hull_mass_kg
	hull.moment = boat.hull_moment_of_inertia_kg_m2(parameters)

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
	assembly = Assembly(sail, rudder, constraints)
	state.coupled_assembly = assembly
	_preserve_lumped_momentum(
		(hull, sail, rudder),
		pymunk.Vec2d(*hull.position),
		pymunk.Vec2d(*linear_momentum),
		angular_momentum,
	)
	return assembly


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


def deactivate(
	state: boat.SailingState,
	parameters: boat.BoatParameters,
) -> None:
	if not isinstance(state.coupled_assembly, Assembly):
		return

	assembly = state.coupled_assembly
	_sync_surface_state(state, assembly)
	hull = state.boat
	bodies = (hull, assembly.sail, assembly.rudder)
	linear_momentum = sum(
		(body.mass * body.velocity for body in bodies),
		pymunk.Vec2d(0, 0),
	)
	angular_momentum = _angular_momentum_about(
		bodies,
		pymunk.Vec2d(*hull.position),
	)
	state.space.remove(*assembly.constraints, assembly.sail, assembly.rudder)
	state.coupled_assembly = None
	hull.mass = boat.total_mass_kg(parameters)
	hull.moment = boat.lumped_moment_of_inertia_kg_m2(parameters)
	hull.velocity = linear_momentum / hull.mass
	hull.angular_velocity = angular_momentum / hull.moment


def _surface_load(
	hull: pymunk.Body,
	surface: pymunk.Body,
	medium_velocity_m_s: pymunk.Vec2d,
	hinge_position_local_m: tuple[float, float],
	center_distance_m: float,
	fluid_density_kg_m3: float,
	area_m2: float,
	base_drag_coefficient: float,
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
		area_m2,
		base_drag_coefficient,
	)
	return boat.HingeLoad(
		force_n,
		hull.world_to_local(center_world_m),
		(center_world_m - hinge_world_m).cross(force_n),
	)


def step(
	state: boat.SailingState,
	environment: boat.Environment,
	parameters: boat.BoatParameters,
	actuator_torques: boat.ActuatorTorques,
	dt_seconds: float,
) -> boat.PhysicsTelemetry:
	assembly = activate(state, parameters)
	_sync_surface_state(state, assembly)
	hull = state.boat
	apparent_water_m_s = boat.apparent_flow(
		environment.true_water_velocity_m_s,
		hull.velocity,
	)
	apparent_wind_m_s = boat.apparent_flow(
		environment.true_wind_velocity_m_s,
		hull.velocity,
	)
	sail_load = _surface_load(
		hull,
		assembly.sail,
		environment.true_wind_velocity_m_s,
		parameters.mast_position_m,
		parameters.sail_center_of_effort_from_mast_m,
		parameters.air_density_kg_m3,
		parameters.sail_area_m2,
		parameters.sail_base_drag_coefficient,
	)
	rudder_load = _surface_load(
		hull,
		assembly.rudder,
		environment.true_water_velocity_m_s,
		parameters.rudder_position_m,
		parameters.rudder_draw_length_m / 2,
		parameters.water_density_kg_m3,
		parameters.rudder_area_m2,
		parameters.underwater_foil_base_drag_coefficient,
	)
	observation = boat.BoatObservation(
		hull.angle,
		hull.angular_velocity,
		pymunk.Vec2d(*hull.velocity),
		state.sail_angle_rad,
		state.sail_angular_velocity_rad_s,
		state.rudder_angle_rad,
		state.rudder_angular_velocity_rad_s,
		apparent_wind_m_s,
		apparent_water_m_s,
		sail_load,
		rudder_load,
	)
	keel_apparent_water_m_s = boat.apparent_flow(
		environment.true_water_velocity_m_s,
		hull.velocity_at_local_point(parameters.keel_position_m),
	)
	keel_force_n = boat.force_on_foil(
		keel_apparent_water_m_s,
		hull.angle,
		parameters.water_density_kg_m3,
		parameters.keel_area_m2,
		parameters.underwater_foil_base_drag_coefficient,
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
	state.space.step(dt_seconds)
	_sync_surface_state(state, assembly)
	return boat.PhysicsTelemetry(observation, keel_force_n, hull_drag_n)
