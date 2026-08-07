import math
from dataclasses import dataclass

import pymunk


@dataclass(frozen=True)
class BoatParameters:
	air_density_kg_m3: float
	water_density_kg_m3: float
	hull_mass_kg: float
	length_m: float
	beam_m: float
	bow_length_m: float
	sail_area_m2: float
	boom_length_m: float
	boom_mass_kg: float
	mast_position_m: tuple[float, float]
	sail_center_of_effort_from_mast_m: float
	sail_mass_kg: float
	keel_area_m2: float
	rudder_area_m2: float
	rudder_draw_length_m: float
	rudder_mass_kg: float
	tiller_length_m: float
	tiller_mass_kg: float
	rudder_position_m: tuple[float, float]
	hull_reference_area_m2: float
	hull_drag_coefficient: float
	foil_base_drag_coefficient: float
	sail_bearing_damping_n_m_s_rad: float
	rudder_bearing_damping_n_m_s_rad: float
	sail_sheet_lever_arm_m: float
	max_sail_angle_rad: float
	max_rudder_angle_rad: float


@dataclass(frozen=True)
class Environment:
	true_wind_velocity_m_s: pymunk.Vec2d
	true_water_velocity_m_s: pymunk.Vec2d


@dataclass
class SailingState:
	space: pymunk.Space
	boat: pymunk.Body
	sail_angle_rad: float = 0
	sail_angular_velocity_rad_s: float = 0
	rudder_angle_rad: float = 0
	rudder_angular_velocity_rad_s: float = 0
	coupled_assembly: object | None = None


@dataclass(frozen=True)
class ActuatorTorques:
	sail_n_m: float = 0
	rudder_n_m: float = 0


@dataclass(frozen=True)
class HingeLoad:
	force_n: pymunk.Vec2d
	center_local_m: pymunk.Vec2d
	torque_n_m: float


@dataclass(frozen=True)
class BoatObservation:
	boat_angle_rad: float
	boat_angular_velocity_rad_s: float
	boat_velocity_m_s: pymunk.Vec2d
	sail_angle_rad: float
	sail_angular_velocity_rad_s: float
	rudder_angle_rad: float
	rudder_angular_velocity_rad_s: float
	apparent_wind_m_s: pymunk.Vec2d
	apparent_water_m_s: pymunk.Vec2d
	sail_load: HingeLoad
	rudder_load: HingeLoad


@dataclass(frozen=True)
class PhysicsTelemetry:
	"""Loads and observation used to advance the most recent physics step."""

	observation: BoatObservation
	keel_force_n: pymunk.Vec2d
	hull_drag_n: pymunk.Vec2d


def total_mass_kg(parameters: BoatParameters) -> float:
	return (
		parameters.hull_mass_kg
		+ parameters.sail_mass_kg
		+ parameters.boom_mass_kg
		+ parameters.rudder_mass_kg
		+ parameters.tiller_mass_kg
	)


def hull_moment_of_inertia_kg_m2(parameters: BoatParameters) -> float:
	return (
		parameters.hull_mass_kg
		* (parameters.length_m**2 + parameters.beam_m**2)
		/ 12
	)


def sail_moment_of_inertia_kg_m2(parameters: BoatParameters) -> float:
	return (
		(parameters.sail_mass_kg + parameters.boom_mass_kg)
		* parameters.boom_length_m**2
		/ 3
	)


def rudder_moment_of_inertia_kg_m2(parameters: BoatParameters) -> float:
	return (
		parameters.rudder_mass_kg * parameters.rudder_draw_length_m**2 / 3
		+ parameters.tiller_mass_kg * parameters.tiller_length_m**2 / 3
	)


def sail_center_from_mast_m(parameters: BoatParameters) -> pymunk.Vec2d:
	return pymunk.Vec2d(-parameters.boom_length_m / 2, 0)


def rudder_center_from_hinge_m(parameters: BoatParameters) -> pymunk.Vec2d:
	mass_kg = parameters.rudder_mass_kg + parameters.tiller_mass_kg
	return pymunk.Vec2d(
		(
			-parameters.rudder_mass_kg * parameters.rudder_draw_length_m / 2
			+ parameters.tiller_mass_kg * parameters.tiller_length_m / 2
		)
		/ mass_kg,
		0,
	)


def sail_center_moment_of_inertia_kg_m2(parameters: BoatParameters) -> float:
	mass_kg = parameters.sail_mass_kg + parameters.boom_mass_kg
	return sail_moment_of_inertia_kg_m2(parameters) - mass_kg * (
		sail_center_from_mast_m(parameters).length**2
	)


def rudder_center_moment_of_inertia_kg_m2(parameters: BoatParameters) -> float:
	mass_kg = parameters.rudder_mass_kg + parameters.tiller_mass_kg
	return rudder_moment_of_inertia_kg_m2(parameters) - mass_kg * (
		rudder_center_from_hinge_m(parameters).length**2
	)


def lumped_moment_of_inertia_kg_m2(parameters: BoatParameters) -> float:
	return (
		total_mass_kg(parameters)
		* (parameters.length_m**2 + parameters.beam_m**2)
		/ 12
	)


def apparent_flow(
	medium_velocity_m_s: pymunk.Vec2d,
	point_velocity_m_s: pymunk.Vec2d,
) -> pymunk.Vec2d:
	return medium_velocity_m_s - point_velocity_m_s


def luffing_sail_angle(
	boat_angle_rad: float,
	apparent_wind_m_s: pymunk.Vec2d,
) -> float:
	if not apparent_wind_m_s.length:
		return 0
	upstream_angle_rad = apparent_wind_m_s.angle + math.pi
	return math.remainder(upstream_angle_rad - boat_angle_rad, math.pi)


def free_sail_angle(
	boat_angle_rad: float,
	apparent_wind_m_s: pymunk.Vec2d,
	maximum_angle_rad: float,
) -> float:
	if not apparent_wind_m_s.length:
		return 0
	boom_angle_rad = math.remainder(
		apparent_wind_m_s.angle - boat_angle_rad - math.pi,
		2 * math.pi,
	)
	return max(-maximum_angle_rad, min(maximum_angle_rad, boom_angle_rad))


def lift_coefficient(angle_of_attack_rad: float) -> float:
	"""Approximate a symmetric flat plate, including loss of lift past 45 degrees."""
	return math.sin(2 * angle_of_attack_rad)


def drag_coefficient(
	angle_of_attack_rad: float,
	base_drag_coefficient: float,
) -> float:
	"""Combine profile drag with flat-plate pressure drag."""
	return base_drag_coefficient + 1 - math.cos(2 * angle_of_attack_rad)


def force_on_foil(
	apparent_flow_m_s: pymunk.Vec2d,
	foil_angle_rad: float,
	fluid_density_kg_m3: float,
	area_m2: float,
	base_drag_coefficient: float,
) -> pymunk.Vec2d:
	angle_of_attack_rad = foil_angle_rad - math.pi - apparent_flow_m_s.angle
	dynamic_pressure_pa = 0.5 * fluid_density_kg_m3 * apparent_flow_m_s.length**2
	lift_n = dynamic_pressure_pa * area_m2 * lift_coefficient(angle_of_attack_rad)
	drag_n = (
		dynamic_pressure_pa
		* area_m2
		* drag_coefficient(angle_of_attack_rad, base_drag_coefficient)
	)
	return pymunk.Vec2d(drag_n, -lift_n).rotated(apparent_flow_m_s.angle)


def drag_force(
	apparent_flow_m_s: pymunk.Vec2d,
	fluid_density_kg_m3: float,
	reference_area_m2: float,
	coefficient: float,
) -> pymunk.Vec2d:
	return (
		apparent_flow_m_s
		* apparent_flow_m_s.length
		* 0.5
		* fluid_density_kg_m3
		* reference_area_m2
		* coefficient
	)


def hinged_foil_load(
	body: pymunk.Body,
	medium_velocity_m_s: pymunk.Vec2d,
	hinge_position_local_m: tuple[float, float],
	center_distance_m: float,
	angle_rad: float,
	angular_velocity_rad_s: float,
	fluid_density_kg_m3: float,
	area_m2: float,
	base_drag_coefficient: float,
) -> HingeLoad:
	center_from_hinge_local_m = pymunk.Vec2d(-center_distance_m, 0).rotated(angle_rad)
	center_local_m = pymunk.Vec2d(*hinge_position_local_m) + center_from_hinge_local_m
	center_from_hinge_world_m = center_from_hinge_local_m.rotated(body.angle)
	center_velocity_m_s = (
		body.velocity_at_local_point(center_local_m)
		+ center_from_hinge_world_m.perpendicular() * angular_velocity_rad_s
	)
	apparent_flow_m_s = apparent_flow(medium_velocity_m_s, center_velocity_m_s)
	force_n = force_on_foil(
		apparent_flow_m_s,
		body.angle + angle_rad,
		fluid_density_kg_m3,
		area_m2,
		base_drag_coefficient,
	)
	return HingeLoad(
		force_n,
		center_local_m,
		center_from_hinge_world_m.cross(force_n),
	)


def integrate_hinge(
	angle_rad: float,
	angular_velocity_rad_s: float,
	torque_n_m: float,
	moment_kg_m2: float,
	bearing_damping_n_m_s_rad: float,
	max_angle_rad: float,
	dt_seconds: float,
) -> tuple[float, float]:
	damping_torque_n_m = angular_velocity_rad_s * bearing_damping_n_m_s_rad
	angular_acceleration_rad_s2 = (torque_n_m - damping_torque_n_m) / moment_kg_m2
	angular_velocity_rad_s += angular_acceleration_rad_s2 * dt_seconds
	angle_rad += angular_velocity_rad_s * dt_seconds

	limited_angle_rad = max(-max_angle_rad, min(max_angle_rad, angle_rad))
	if limited_angle_rad != angle_rad:
		angular_velocity_rad_s = 0
	return limited_angle_rad, angular_velocity_rad_s


def create_simulation(
	parameters: BoatParameters,
	initial_angle_rad: float,
) -> SailingState:
	space = pymunk.Space()
	body = pymunk.Body(
		total_mass_kg(parameters),
		lumped_moment_of_inertia_kg_m2(parameters),
	)
	body.angle = initial_angle_rad
	shape = pymunk.Poly.create_box(body, (parameters.length_m, parameters.beam_m))
	space.add(body, shape)
	return SailingState(space, body)


def observe(
	state: SailingState,
	environment: Environment,
	parameters: BoatParameters,
) -> BoatObservation:
	apparent_water_m_s = apparent_flow(
		environment.true_water_velocity_m_s,
		state.boat.velocity,
	)
	apparent_wind_m_s = apparent_flow(
		environment.true_wind_velocity_m_s,
		state.boat.velocity,
	)
	sail_load = hinged_foil_load(
		state.boat,
		environment.true_wind_velocity_m_s,
		parameters.mast_position_m,
		parameters.sail_center_of_effort_from_mast_m,
		state.sail_angle_rad,
		state.sail_angular_velocity_rad_s,
		parameters.air_density_kg_m3,
		parameters.sail_area_m2,
		parameters.foil_base_drag_coefficient,
	)
	rudder_load = hinged_foil_load(
		state.boat,
		environment.true_water_velocity_m_s,
		parameters.rudder_position_m,
		parameters.rudder_draw_length_m / 2,
		state.rudder_angle_rad,
		state.rudder_angular_velocity_rad_s,
		parameters.water_density_kg_m3,
		parameters.rudder_area_m2,
		parameters.foil_base_drag_coefficient,
	)
	return BoatObservation(
		state.boat.angle,
		state.boat.angular_velocity,
		pymunk.Vec2d(*state.boat.velocity),
		state.sail_angle_rad,
		state.sail_angular_velocity_rad_s,
		state.rudder_angle_rad,
		state.rudder_angular_velocity_rad_s,
		apparent_wind_m_s,
		apparent_water_m_s,
		sail_load,
		rudder_load,
	)
