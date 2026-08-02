import math
from dataclasses import dataclass

import numpy as np
import pygame
import pymunk


WIDTH, HEIGHT = 1280, 720
PIXELS_PER_METER = 60
FLOW_PIXELS_PER_METER_SECOND = 20
FORCE_PIXELS_PER_NEWTON = 0.35
INTENT_VECTOR_LENGTH_PX = 100
DT_SECONDS = 1 / 60

AIR_DENSITY_KG_M3 = 1.225  # Dry air at sea level and 15 °C.
WATER_DENSITY_KG_M3 = 1025  # Representative seawater.

# Representative dimensions for a small centerboard dinghy.
BOAT_MASS_KG = 250
BOAT_LENGTH_M = 4
BOAT_BEAM_M = 1.5
BOAT_BOW_LENGTH_M = 0.7
SAIL_AREA_M2 = 8
BOOM_LENGTH_M = 2
BOOM_MASS_KG = 10
MAST_POSITION_M = (0.4, 0)
SAIL_CENTER_OF_EFFORT_FROM_MAST_M = 0.7
SAIL_MASS_KG = 12
KEEL_AREA_M2 = 0.6
RUDDER_AREA_M2 = 0.12
RUDDER_DRAW_LENGTH_M = 0.5
RUDDER_MASS_KG = 4
TILLER_MASS_KG = 2
HULL_REFERENCE_AREA_M2 = 0.3
HULL_DRAG_COEFFICIENT = 0.08
FOIL_BASE_DRAG_COEFFICIENT = 0.02
CONTROL_ANGLE_RESOLUTION_DEG = 1

SAIL_BEARING_DAMPING_N_M_S_RAD = 20
RUDDER_BEARING_DAMPING_N_M_S_RAD = 2
SAILOR_MAX_SHEET_FORCE_N = 250
SAIL_SHEET_LEVER_ARM_M = 1.5
SAILOR_MAX_TILLER_FORCE_N = 60
TILLER_LENGTH_M = 1

ANALOG_STICK_DEADZONE = 0.15

MAX_SAIL_ANGLE_RAD = math.radians(90)
MAX_RUDDER_ANGLE_RAD = math.radians(30)
RUDDER_POSITION_M = (-1.8, 0)
BOAT_MOMENT_OF_INERTIA_KG_M2 = (
	BOAT_MASS_KG * (BOAT_LENGTH_M**2 + BOAT_BEAM_M**2) / 12
)
SAIL_MOMENT_OF_INERTIA_KG_M2 = (
	(SAIL_MASS_KG + BOOM_MASS_KG) * BOOM_LENGTH_M**2 / 3
)
RUDDER_MOMENT_OF_INERTIA_KG_M2 = (
	RUDDER_MASS_KG * RUDDER_DRAW_LENGTH_M**2 / 3
	+ TILLER_MASS_KG * TILLER_LENGTH_M**2 / 3
)


@dataclass(frozen=True)
class Environment:
	true_wind_velocity_m_s: pymunk.Vec2d
	true_water_velocity_m_s: pymunk.Vec2d


DEFAULT_ENVIRONMENT = Environment(
	true_wind_velocity_m_s=pymunk.Vec2d(6, 0),
	true_water_velocity_m_s=pymunk.Vec2d(0, 0),
)


@dataclass
class SailingState:
	space: pymunk.Space
	boat: pymunk.Body
	sail_angle_rad: float = 0
	sail_angular_velocity_rad_s: float = 0
	rudder_angle_rad: float = 0
	rudder_angular_velocity_rad_s: float = 0


@dataclass(frozen=True)
class MotionIntent:
	forward: float = 0
	turn: float = 0


@dataclass(frozen=True)
class HingeLoad:
	force_n: pymunk.Vec2d
	center_local_m: pymunk.Vec2d
	torque_n_m: float


@dataclass(frozen=True)
class SimulationTelemetry:
	apparent_wind_m_s: pymunk.Vec2d
	apparent_water_m_s: pymunk.Vec2d
	sail_force_n: pymunk.Vec2d
	keel_force_n: pymunk.Vec2d
	hull_drag_n: pymunk.Vec2d
	rudder_force_n: pymunk.Vec2d
	target_sail_angle_rad: float
	target_rudder_angle_rad: float


@dataclass(frozen=True)
class ControlEffect:
	angle_rad: float
	yaw_moment_n_m: float
	drive_force_n: float
	force_magnitude_n: float


def intent_vector(intent: MotionIntent, boat_heading_rad: float) -> pymunk.Vec2d:
	return pymunk.Vec2d(intent.forward, intent.turn).rotated(boat_heading_rad)


def keyboard_motion_intent(
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
) -> MotionIntent:
	longitudinal = int(forward) - int(backward)
	turn = int(left) - int(right)
	return MotionIntent(longitudinal, -turn)


def analog_motion_intent(
	boat_heading_rad: float,
	stick_x: float,
	stick_y: float,
) -> MotionIntent:
	world_vector = pymunk.Vec2d(stick_x, stick_y)
	magnitude = min(1.0, world_vector.length)
	if magnitude <= ANALOG_STICK_DEADZONE:
		return MotionIntent()
	if math.isclose(magnitude, 1):
		magnitude = 1
	local_direction = world_vector.normalized().rotated(-boat_heading_rad)
	square_scale = max(abs(local_direction.x), abs(local_direction.y))
	local_intent = local_direction / square_scale * magnitude
	zero_tolerance = math.ulp(2 * math.pi)
	forward = 0 if math.isclose(local_intent.x, 0, abs_tol=zero_tolerance) else local_intent.x
	turn = 0 if math.isclose(local_intent.y, 0, abs_tol=zero_tolerance) else local_intent.y
	return MotionIntent(forward, turn)


def apparent_flow(
	medium_velocity_m_s: pymunk.Vec2d,
	point_velocity_m_s: pymunk.Vec2d,
) -> pymunk.Vec2d:
	return medium_velocity_m_s - point_velocity_m_s


def lift_coefficient(angle_of_attack_rad: float) -> float:
	"""Approximate a symmetric flat plate, including loss of lift past 45 degrees."""
	return math.sin(2 * angle_of_attack_rad)


def drag_coefficient(angle_of_attack_rad: float) -> float:
	"""Combine a small profile drag with flat-plate pressure drag."""
	return FOIL_BASE_DRAG_COEFFICIENT + 1 - math.cos(2 * angle_of_attack_rad)


def force_on_foil(
	apparent_flow_m_s: pymunk.Vec2d,
	foil_angle_rad: float,
	fluid_density_kg_m3: float,
	area_m2: float,
) -> pymunk.Vec2d:
	angle_of_attack_rad = foil_angle_rad - math.pi - apparent_flow_m_s.angle
	dynamic_pressure_pa = 0.5 * fluid_density_kg_m3 * apparent_flow_m_s.length**2
	lift_n = dynamic_pressure_pa * area_m2 * lift_coefficient(angle_of_attack_rad)
	drag_n = dynamic_pressure_pa * area_m2 * drag_coefficient(angle_of_attack_rad)
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
	)
	return HingeLoad(
		force_n,
		center_local_m,
		center_from_hinge_world_m.cross(force_n),
	)


def actuator_control_torque(
	angle_rad: float,
	angular_velocity_rad_s: float,
	target_angle_rad: float,
	external_torque_n_m: float,
	moment_of_inertia_kg_m2: float,
	bearing_damping_n_m_s_rad: float,
	max_torque_n_m: float,
	dt_seconds: float,
) -> float:
	angle_error_rad = target_angle_rad - angle_rad
	maximum_acceleration_rad_s2 = max_torque_n_m / moment_of_inertia_kg_m2
	target_speed_rad_s = math.copysign(
		min(
			math.sqrt(2 * maximum_acceleration_rad_s2 * abs(angle_error_rad)),
			abs(angle_error_rad) / dt_seconds,
		),
		angle_error_rad,
	)
	required_acceleration_rad_s2 = (
		target_speed_rad_s - angular_velocity_rad_s
	) / dt_seconds
	torque_n_m = (
		moment_of_inertia_kg_m2 * required_acceleration_rad_s2
		- external_torque_n_m
		+ angular_velocity_rad_s * bearing_damping_n_m_s_rad
	)
	return max(-max_torque_n_m, min(max_torque_n_m, torque_n_m))


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


def luffing_sail_angle(boat_angle_rad: float, apparent_wind_m_s: pymunk.Vec2d) -> float:
	if not apparent_wind_m_s.length:
		return 0
	upstream_angle_rad = apparent_wind_m_s.angle + math.pi
	return math.remainder(upstream_angle_rad - boat_angle_rad, math.pi)


def foil_control_effect(
	boat_angle_rad: float,
	apparent_flow_m_s: pymunk.Vec2d,
	hinge_position_local_m: tuple[float, float],
	center_distance_m: float,
	angle_rad: float,
	fluid_density_kg_m3: float,
	area_m2: float,
) -> ControlEffect:
	force_n = force_on_foil(
		apparent_flow_m_s,
		boat_angle_rad + angle_rad,
		fluid_density_kg_m3,
		area_m2,
	)
	center_local_m = (
		pymunk.Vec2d(*hinge_position_local_m)
		+ pymunk.Vec2d(-center_distance_m, 0).rotated(angle_rad)
	)
	center_world_m = center_local_m.rotated(boat_angle_rad)
	boat_forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)
	return ControlEffect(
		angle_rad,
		center_world_m.cross(force_n),
		force_n.dot(boat_forward),
		force_n.length,
	)


def foil_control_candidates(
	boat_angle_rad: float,
	apparent_flow_m_s: pymunk.Vec2d,
	hinge_position_local_m: tuple[float, float],
	center_distance_m: float,
	maximum_angle_rad: float,
	fluid_density_kg_m3: float,
	area_m2: float,
) -> tuple[ControlEffect, ...]:
	maximum_angle_deg = round(math.degrees(maximum_angle_rad))
	return tuple(
		foil_control_effect(
			boat_angle_rad,
			apparent_flow_m_s,
			hinge_position_local_m,
			center_distance_m,
			math.radians(angle_deg),
			fluid_density_kg_m3,
			area_m2,
		)
		for angle_deg in range(
			-maximum_angle_deg,
			maximum_angle_deg + 1,
			CONTROL_ANGLE_RESOLUTION_DEG,
		)
	)


def normalized_control_effect(
	value: float,
	negative_limit: float,
	positive_limit: float,
) -> float:
	if value > 0 and positive_limit:
		return value / positive_limit
	if value < 0 and negative_limit:
		return value / -negative_limit
	return 0


def control_limits(
	sails: tuple[ControlEffect, ...],
	rudders: tuple[ControlEffect, ...],
) -> tuple[float, float, float, float]:
	return (
		min(0, min(sail.drive_force_n for sail in sails) + min(rudder.drive_force_n for rudder in rudders)),
		max(0, max(sail.drive_force_n for sail in sails) + max(rudder.drive_force_n for rudder in rudders)),
		min(0, min(sail.yaw_moment_n_m for sail in sails) + min(rudder.yaw_moment_n_m for rudder in rudders)),
		max(0, max(sail.yaw_moment_n_m for sail in sails) + max(rudder.yaw_moment_n_m for rudder in rudders)),
	)


def stopping_turn_request(
	angular_velocity_rad_s: float,
	negative_moment_n_m: float,
	positive_moment_n_m: float,
) -> float:
	if angular_velocity_rad_s > 0:
		yaw_stiffness_n_m_rad = -negative_moment_n_m
	else:
		yaw_stiffness_n_m_rad = positive_moment_n_m
	damping_n_m_s_rad = 2 * math.sqrt(
		yaw_stiffness_n_m_rad * BOAT_MOMENT_OF_INERTIA_KG_M2
	)
	required_moment_n_m = -angular_velocity_rad_s * damping_n_m_s_rad
	return max(
		-1,
		min(
			1,
			normalized_control_effect(
				required_moment_n_m,
				negative_moment_n_m,
				positive_moment_n_m,
			),
		),
	)


def select_control_effects(
	sails: tuple[ControlEffect, ...],
	rudders: tuple[ControlEffect, ...],
	intent: MotionIntent,
	angular_velocity_rad_s: float,
	neutral_sail_angle_rad: float,
) -> tuple[ControlEffect, ControlEffect]:
	if not intent.forward and not intent.turn:
		sail = min(
			sails,
			key=lambda candidate: (
				candidate.force_magnitude_n,
				abs(candidate.angle_rad - neutral_sail_angle_rad),
			),
		)
		moments_n_m = tuple(
			sail.yaw_moment_n_m + rudder.yaw_moment_n_m for rudder in rudders
		)
		negative_moment_n_m = min(0, min(moments_n_m))
		positive_moment_n_m = max(0, max(moments_n_m))
		turn_request = stopping_turn_request(
			angular_velocity_rad_s,
			negative_moment_n_m,
			positive_moment_n_m,
		)
		if turn_request < 0:
			target_moment_n_m = turn_request * -negative_moment_n_m
		else:
			target_moment_n_m = turn_request * positive_moment_n_m
		rudder = min(
			rudders,
			key=lambda candidate: (
				abs(sail.yaw_moment_n_m + candidate.yaw_moment_n_m - target_moment_n_m),
				abs(candidate.angle_rad),
			),
		)
		return sail, rudder

	negative_drive, positive_drive, negative_moment, positive_moment = control_limits(
		sails,
		rudders,
	)
	desired_turn = intent.turn
	if not desired_turn:
		desired_turn = stopping_turn_request(
			angular_velocity_rad_s,
			negative_moment,
			positive_moment,
		)

	sail_drives = np.array([sail.drive_force_n for sail in sails])[:, np.newaxis]
	rudder_drives = np.array([rudder.drive_force_n for rudder in rudders])
	drives = sail_drives + rudder_drives
	normalized_drives = np.zeros_like(drives)
	if positive_drive:
		normalized_drives[drives > 0] = drives[drives > 0] / positive_drive
	if negative_drive:
		normalized_drives[drives < 0] = drives[drives < 0] / -negative_drive

	sail_moments = np.array([sail.yaw_moment_n_m for sail in sails])[:, np.newaxis]
	rudder_moments = np.array([rudder.yaw_moment_n_m for rudder in rudders])
	moments = sail_moments + rudder_moments
	normalized_moments = np.zeros_like(moments)
	if positive_moment:
		normalized_moments[moments > 0] = moments[moments > 0] / positive_moment
	if negative_moment:
		normalized_moments[moments < 0] = moments[moments < 0] / -negative_moment

	error = np.round(
		(normalized_drives - intent.forward) ** 2
		+ (normalized_moments - desired_turn) ** 2,
		12,
	)
	sail_angles = np.array([sail.angle_rad for sail in sails])[:, np.newaxis]
	rudder_angles = np.array([rudder.angle_rad for rudder in rudders])
	angle_travel = np.abs(sail_angles) + np.abs(rudder_angles)
	if desired_turn:
		tie_break = np.broadcast_to(-sail_angles * desired_turn, error.shape)
	else:
		tie_break = np.broadcast_to(
			np.abs(sail_angles - neutral_sail_angle_rad),
			error.shape,
		)
	flat_index = np.lexsort(
		(tie_break.ravel(), angle_travel.ravel(), error.ravel())
	)[0]
	sail_index, rudder_index = np.unravel_index(flat_index, error.shape)
	return sails[sail_index], rudders[rudder_index]


def intended_actuator_angles(
	boat_angle_rad: float,
	boat_angular_velocity_rad_s: float,
	intent: MotionIntent,
	apparent_wind_m_s: pymunk.Vec2d,
	apparent_water_m_s: pymunk.Vec2d,
) -> tuple[float, float]:
	sail_candidates = foil_control_candidates(
		boat_angle_rad,
		apparent_wind_m_s,
		MAST_POSITION_M,
		SAIL_CENTER_OF_EFFORT_FROM_MAST_M,
		MAX_SAIL_ANGLE_RAD,
		AIR_DENSITY_KG_M3,
		SAIL_AREA_M2,
	)
	rudder_candidates = foil_control_candidates(
		boat_angle_rad,
		apparent_water_m_s,
		RUDDER_POSITION_M,
		RUDDER_DRAW_LENGTH_M / 2,
		MAX_RUDDER_ANGLE_RAD,
		WATER_DENSITY_KG_M3,
		RUDDER_AREA_M2,
	)

	sail, rudder = select_control_effects(
		sail_candidates,
		rudder_candidates,
		intent,
		boat_angular_velocity_rad_s,
		luffing_sail_angle(boat_angle_rad, apparent_wind_m_s),
	)
	return sail.angle_rad, rudder.angle_rad


def create_simulation() -> SailingState:
	space = pymunk.Space()
	boat = pymunk.Body(BOAT_MASS_KG, BOAT_MOMENT_OF_INERTIA_KG_M2)
	boat.angle = math.radians(135)
	shape = pymunk.Poly.create_box(boat, (BOAT_LENGTH_M, BOAT_BEAM_M))
	space.add(boat, shape)
	return SailingState(space, boat)


def step(
	state: SailingState,
	environment: Environment,
	intent: MotionIntent,
	dt_seconds: float = DT_SECONDS,
) -> SimulationTelemetry:
	apparent_water_m_s = apparent_flow(
		environment.true_water_velocity_m_s,
		state.boat.velocity,
	)
	apparent_wind_m_s = apparent_flow(
		environment.true_wind_velocity_m_s,
		state.boat.velocity,
	)
	target_sail_angle_rad, target_rudder_angle_rad = intended_actuator_angles(
		state.boat.angle,
		state.boat.angular_velocity,
		intent,
		apparent_wind_m_s,
		apparent_water_m_s,
	)
	sail_load = hinged_foil_load(
		state.boat,
		environment.true_wind_velocity_m_s,
		MAST_POSITION_M,
		SAIL_CENTER_OF_EFFORT_FROM_MAST_M,
		state.sail_angle_rad,
		state.sail_angular_velocity_rad_s,
		AIR_DENSITY_KG_M3,
		SAIL_AREA_M2,
	)
	rudder_load = hinged_foil_load(
		state.boat,
		environment.true_water_velocity_m_s,
		RUDDER_POSITION_M,
		RUDDER_DRAW_LENGTH_M / 2,
		state.rudder_angle_rad,
		state.rudder_angular_velocity_rad_s,
		WATER_DENSITY_KG_M3,
		RUDDER_AREA_M2,
	)
	keel_force_n = force_on_foil(
		apparent_water_m_s,
		state.boat.angle,
		WATER_DENSITY_KG_M3,
		KEEL_AREA_M2,
	)
	hull_drag_n = drag_force(
		apparent_water_m_s,
		WATER_DENSITY_KG_M3,
		HULL_REFERENCE_AREA_M2,
		HULL_DRAG_COEFFICIENT,
	)

	sailor_sail_torque_n_m = actuator_control_torque(
		state.sail_angle_rad,
		state.sail_angular_velocity_rad_s,
		target_sail_angle_rad,
		sail_load.torque_n_m,
		SAIL_MOMENT_OF_INERTIA_KG_M2,
		SAIL_BEARING_DAMPING_N_M_S_RAD,
		SAILOR_MAX_SHEET_FORCE_N * SAIL_SHEET_LEVER_ARM_M,
		dt_seconds,
	)
	sailor_rudder_torque_n_m = actuator_control_torque(
		state.rudder_angle_rad,
		state.rudder_angular_velocity_rad_s,
		target_rudder_angle_rad,
		rudder_load.torque_n_m,
		RUDDER_MOMENT_OF_INERTIA_KG_M2,
		RUDDER_BEARING_DAMPING_N_M_S_RAD,
		SAILOR_MAX_TILLER_FORCE_N * TILLER_LENGTH_M,
		dt_seconds,
	)

	state.boat.apply_force_at_local_point(
		sail_load.force_n.rotated(-state.boat.angle),
		sail_load.center_local_m,
	)
	state.boat.apply_force_at_local_point(
		keel_force_n.rotated(-state.boat.angle) + hull_drag_n.rotated(-state.boat.angle),
		(0, 0),
	)
	state.boat.apply_force_at_local_point(
		rudder_load.force_n.rotated(-state.boat.angle),
		rudder_load.center_local_m,
	)
	state.space.step(dt_seconds)

	state.sail_angle_rad, state.sail_angular_velocity_rad_s = integrate_hinge(
		state.sail_angle_rad,
		state.sail_angular_velocity_rad_s,
		sail_load.torque_n_m + sailor_sail_torque_n_m,
		SAIL_MOMENT_OF_INERTIA_KG_M2,
		SAIL_BEARING_DAMPING_N_M_S_RAD,
		MAX_SAIL_ANGLE_RAD,
		dt_seconds,
	)
	state.rudder_angle_rad, state.rudder_angular_velocity_rad_s = integrate_hinge(
		state.rudder_angle_rad,
		state.rudder_angular_velocity_rad_s,
		rudder_load.torque_n_m + sailor_rudder_torque_n_m,
		RUDDER_MOMENT_OF_INERTIA_KG_M2,
		RUDDER_BEARING_DAMPING_N_M_S_RAD,
		MAX_RUDDER_ANGLE_RAD,
		dt_seconds,
	)
	return SimulationTelemetry(
		apparent_wind_m_s,
		apparent_water_m_s,
		sail_load.force_n,
		keel_force_n,
		hull_drag_n,
		rudder_load.force_n,
		target_sail_angle_rad,
		target_rudder_angle_rad,
	)


def draw_vector(
	screen: pygame.Surface,
	start: pymunk.Vec2d,
	vector: pymunk.Vec2d,
	color: tuple[int, int, int],
	scale: float,
	width: int,
) -> None:
	end = start + vector * scale
	pygame.draw.line(screen, color, start, end, width)
	if not vector.length:
		return
	direction = vector.normalized()
	normal = direction.perpendicular()
	arrowhead = [
		end,
		end - direction * 10 + normal * 5,
		end - direction * 10 - normal * 5,
	]
	pygame.draw.polygon(screen, color, arrowhead)


def draw_arrow(
	screen: pygame.Surface,
	font: pygame.font.Font,
	start: pymunk.Vec2d,
	vector: pymunk.Vec2d,
	color: tuple[int, int, int],
	label: str,
	scale: float,
	unit: str,
	label_x: int,
) -> None:
	pygame.draw.circle(screen, color, start, 4)
	draw_vector(screen, start, vector, color, scale, 3)
	text = font.render(f"{label}: {vector.length:.1f} {unit}", True, color)
	screen.blit(text, (label_x, start.y - 10))


def draw_intent(
	screen: pygame.Surface,
	font: pygame.font.Font,
	start: pymunk.Vec2d,
	boat_heading_rad: float,
	intent: MotionIntent,
) -> None:
	forward_vector = pymunk.Vec2d(intent.forward, 0).rotated(boat_heading_rad)
	turn_vector = pymunk.Vec2d(0, intent.turn).rotated(boat_heading_rad)
	forward_end = start + forward_vector * INTENT_VECTOR_LENGTH_PX
	draw_vector(
		screen,
		start,
		forward_vector,
		(100, 220, 140),
		INTENT_VECTOR_LENGTH_PX,
		2,
	)
	draw_vector(
		screen,
		forward_end,
		turn_vector,
		(255, 160, 80),
		INTENT_VECTOR_LENGTH_PX,
		2,
	)
	draw_vector(
		screen,
		start,
		intent_vector(intent, boat_heading_rad),
		(240, 100, 220),
		INTENT_VECTOR_LENGTH_PX,
		3,
	)
	text = font.render(
		f"intent: forward {intent.forward:+.0%}  turn {intent.turn:+.0%}",
		True,
		(240, 100, 220),
	)
	screen.blit(text, (30, start.y - 10))


def draw(
	screen: pygame.Surface,
	font: pygame.font.Font,
	state: SailingState,
	environment: Environment,
	intent: MotionIntent,
	telemetry: SimulationTelemetry,
) -> None:
	center = pymunk.Vec2d(WIDTH / 2, HEIGHT / 2)
	boat = state.boat
	screen.fill((30, 30, 40))

	grid_spacing_px = PIXELS_PER_METER
	x_offset = -boat.position.x * PIXELS_PER_METER % grid_spacing_px
	y_offset = -boat.position.y * PIXELS_PER_METER % grid_spacing_px
	for x in range(round(x_offset), WIDTH, grid_spacing_px):
		pygame.draw.line(screen, (38, 46, 58), (x, 0), (x, HEIGHT))
	for y in range(round(y_offset), HEIGHT, grid_spacing_px):
		pygame.draw.line(screen, (38, 46, 58), (0, y), (WIDTH, y))

	half_length_px = BOAT_LENGTH_M * PIXELS_PER_METER / 2
	half_beam_px = BOAT_BEAM_M * PIXELS_PER_METER / 2
	bow_base_px = (BOAT_LENGTH_M / 2 - BOAT_BOW_LENGTH_M) * PIXELS_PER_METER
	hull_points = [
		pymunk.Vec2d(half_length_px, 0),
		pymunk.Vec2d(bow_base_px, -half_beam_px),
		pymunk.Vec2d(-half_length_px, -half_beam_px),
		pymunk.Vec2d(-half_length_px, half_beam_px),
		pymunk.Vec2d(bow_base_px, half_beam_px),
	]
	hull_points = [point.rotated(boat.angle) + center for point in hull_points]
	pygame.draw.polygon(screen, (205, 205, 220), hull_points, 3)

	sail_direction = pymunk.Vec2d(-1, 0).rotated(boat.angle + state.sail_angle_rad)
	pygame.draw.line(
		screen,
		(235, 205, 100),
		center,
		center + sail_direction * BOOM_LENGTH_M * PIXELS_PER_METER,
		4,
	)
	rudder_position = (
		pymunk.Vec2d(*RUDDER_POSITION_M).rotated(boat.angle) * PIXELS_PER_METER
		+ center
	)
	rudder_direction = pymunk.Vec2d(-1, 0).rotated(boat.angle + state.rudder_angle_rad)
	pygame.draw.line(
		screen,
		(220, 220, 230),
		rudder_position,
		rudder_position + rudder_direction * RUDDER_DRAW_LENGTH_M * PIXELS_PER_METER,
		4,
	)

	flows = (
		("true wind", environment.true_wind_velocity_m_s, (80, 180, 255)),
		("apparent wind", telemetry.apparent_wind_m_s, (220, 220, 255)),
		("true water", environment.true_water_velocity_m_s, (80, 110, 230)),
		("apparent water", telemetry.apparent_water_m_s, (80, 220, 180)),
		("boat velocity", boat.velocity, (130, 230, 130)),
	)
	for row, (label, vector, color) in enumerate(flows):
		draw_arrow(
			screen,
			font,
			pymunk.Vec2d(240, 45 + row * 55),
			vector,
			color,
			label,
			FLOW_PIXELS_PER_METER_SECOND,
			"m/s",
			30,
		)

	forces = (
		("sail force", telemetry.sail_force_n, (235, 205, 100)),
		("keel force", telemetry.keel_force_n, (80, 220, 180)),
		("hull drag", telemetry.hull_drag_n, (180, 150, 120)),
		("rudder force", telemetry.rudder_force_n, (220, 220, 230)),
	)
	for row, (label, vector, color) in enumerate(forces):
		draw_arrow(
			screen,
			font,
			pymunk.Vec2d(1040, 45 + row * 55),
			vector,
			color,
			label,
			FORCE_PIXELS_PER_NEWTON,
			"N",
			810,
		)

	draw_intent(screen, font, center, boat.angle, intent)

	status = font.render(
		f"speed through water {telemetry.apparent_water_m_s.length:.1f} m/s    "
		f"sail {math.degrees(state.sail_angle_rad):+.0f}° "
		f"target {math.degrees(telemetry.target_sail_angle_rad):+.0f}°    "
		f"rudder {math.degrees(state.rudder_angle_rad):+.0f}° "
		f"target {math.degrees(telemetry.target_rudder_angle_rad):+.0f}°",
		True,
		(220, 220, 230),
	)
	screen.blit(status, (30, HEIGHT - 35))


def main() -> None:
	pygame.init()
	screen = pygame.display.set_mode((WIDTH, HEIGHT))
	pygame.display.set_caption("Sailboat — WASD motion; left stick moves")
	font = pygame.font.Font(None, 24)
	clock = pygame.time.Clock()
	state = create_simulation()
	joystick = None
	if pygame.joystick.get_count():
		joystick = pygame.joystick.Joystick(0)
		joystick.init()
	running = True

	while running:
		for event in pygame.event.get():
			if event.type == pygame.QUIT:
				running = False
			elif event.type == pygame.JOYDEVICEADDED and joystick is None:
				joystick = pygame.joystick.Joystick(event.device_index)
				joystick.init()
			elif (
				event.type == pygame.JOYDEVICEREMOVED
				and joystick is not None
				and event.instance_id == joystick.get_instance_id()
			):
				joystick = None

		keys = pygame.key.get_pressed()
		keyboard_intent = keyboard_motion_intent(
			forward=keys[pygame.K_w],
			backward=keys[pygame.K_s],
			left=keys[pygame.K_a],
			right=keys[pygame.K_d],
		)
		controller_intent = MotionIntent()
		if joystick is not None:
			controller_intent = analog_motion_intent(
				state.boat.angle,
				joystick.get_axis(0),
				joystick.get_axis(1),
			)
		intent = controller_intent
		if controller_intent == MotionIntent():
			intent = keyboard_intent
		telemetry = step(state, DEFAULT_ENVIRONMENT, intent)
		draw(screen, font, state, DEFAULT_ENVIRONMENT, intent, telemetry)
		pygame.display.flip()
		clock.tick(round(1 / DT_SECONDS))

	pygame.quit()


if __name__ == "__main__":
	main()
