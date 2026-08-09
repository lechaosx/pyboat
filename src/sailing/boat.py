import math
from dataclasses import dataclass

import pymunk

from .physics import curves


@dataclass(frozen=True)
class FoilProperties:
	area_m2: float
	lift_coefficient: curves.LiftCurve
	drag_coefficient: curves.DragCurve
	base_drag_coefficient: float


@dataclass(frozen=True)
class BoatParameters:
	air_density_kg_m3: float
	water_density_kg_m3: float
	hull_mass_kg: float
	length_m: float
	beam_m: float
	bow_length_m: float
	sail_foil: FoilProperties
	boom_length_m: float
	boom_mass_kg: float
	mast_position_m: tuple[float, float]
	sail_center_of_effort_from_mast_m: float
	sail_mass_kg: float
	keel_foil: FoilProperties
	keel_position_m: tuple[float, float]
	rudder_foil: FoilProperties
	rudder_draw_length_m: float
	rudder_mass_kg: float
	tiller_length_m: float
	tiller_mass_kg: float
	rudder_position_m: tuple[float, float]
	hull_reference_area_m2: float
	hull_drag_coefficient: float
	hull_lateral_drag_coefficient: float
	sail_bearing_damping_n_m_s_rad: float
	rudder_bearing_damping_n_m_s_rad: float
	sail_sheet_lever_arm_m: float
	max_sail_angle_rad: float
	max_rudder_angle_rad: float


@dataclass(frozen=True)
class Environment:
	true_wind_velocity_m_s: pymunk.Vec2d
	true_water_velocity_m_s: pymunk.Vec2d


@dataclass(frozen=True)
class BoatMotion:
	position_m: tuple[float, float]
	velocity_m_s: tuple[float, float]
	angle_rad: float
	angular_velocity_rad_s: float
	sail_angle_rad: float
	sail_angular_velocity_rad_s: float
	rudder_angle_rad: float
	rudder_angular_velocity_rad_s: float


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


def apparent_flow(
	medium_velocity_m_s: pymunk.Vec2d,
	point_velocity_m_s: pymunk.Vec2d,
) -> pymunk.Vec2d:
	return medium_velocity_m_s - point_velocity_m_s


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


def force_on_foil(
	apparent_flow_m_s: pymunk.Vec2d,
	foil_angle_rad: float,
	fluid_density_kg_m3: float,
	foil: FoilProperties,
) -> pymunk.Vec2d:
	angle_of_attack_rad = foil_angle_rad - math.pi - apparent_flow_m_s.angle
	dynamic_pressure_pa = 0.5 * fluid_density_kg_m3 * apparent_flow_m_s.length**2
	lift_n = (
		dynamic_pressure_pa
		* foil.area_m2
		* foil.lift_coefficient(angle_of_attack_rad)
	)
	drag_n = (
		dynamic_pressure_pa
		* foil.area_m2
		* foil.drag_coefficient(angle_of_attack_rad, foil.base_drag_coefficient)
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


def hull_resistance_force(
	apparent_water_m_s: pymunk.Vec2d,
	boat_angle_rad: float,
	fluid_density_kg_m3: float,
	reference_area_m2: float,
	coefficient: float,
	lateral_coefficient: float = 0,
) -> pymunk.Vec2d:
	local_flow_m_s = apparent_water_m_s.rotated(-boat_angle_rad)
	longitudinal_flow_m_s = pymunk.Vec2d(local_flow_m_s.x, 0)
	lateral_flow_m_s = pymunk.Vec2d(0, local_flow_m_s.y)
	longitudinal_force_n = drag_force(
		longitudinal_flow_m_s,
		fluid_density_kg_m3,
		reference_area_m2,
		coefficient,
	)
	lateral_force_n = drag_force(
		lateral_flow_m_s,
		fluid_density_kg_m3,
		reference_area_m2,
		lateral_coefficient,
	)
	return (longitudinal_force_n + lateral_force_n).rotated(boat_angle_rad)
