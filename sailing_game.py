import math
from dataclasses import dataclass

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
SAIL_TRIM_RESOLUTION_DEG = 1

SAIL_BEARING_DAMPING_N_M_S_RAD = 20
RUDDER_BEARING_DAMPING_N_M_S_RAD = 2
SAIL_CONTROL_STIFFNESS_N_M_RAD = 500
SAIL_CONTROL_DAMPING_N_M_S_RAD = 80
RUDDER_CONTROL_STIFFNESS_N_M_RAD = 180
RUDDER_CONTROL_DAMPING_N_M_S_RAD = 30
SAILOR_MAX_SHEET_FORCE_N = 250
SAIL_SHEET_LEVER_ARM_M = 1.5
SAILOR_MAX_TILLER_FORCE_N = 60
TILLER_LENGTH_M = 1

TARGET_TURN_RATE_RAD_S = math.radians(20)
HEADING_TO_YAW_RATE_GAIN_PER_S = 1.5
YAW_RATE_TO_RUDDER_GAIN_S = 1.5
ANALOG_STICK_DEADZONE = 0.15

MAX_SAIL_ANGLE_RAD = math.radians(90)
MAX_RUDDER_ANGLE_RAD = math.radians(30)
RUDDER_POSITION_M = (-1.8, 0)


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
	direction_world_rad: float | None = None
	intensity: float = 0
	brake: bool = False


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
	target_sail_angle_rad: float | None
	target_rudder_angle_rad: float | None


def intent_vector(intent: MotionIntent) -> pymunk.Vec2d:
	if intent.direction_world_rad is None:
		return pymunk.Vec2d(0, 0)
	return pymunk.Vec2d(intent.intensity, 0).rotated(intent.direction_world_rad)


def resolve_motion_intent(
	boat_heading_rad: float,
	intent: MotionIntent,
) -> tuple[float | None, int]:
	if intent.direction_world_rad is None:
		return None, 0
	heading_error_rad = math.remainder(
		intent.direction_world_rad - boat_heading_rad,
		2 * math.pi,
	)
	if abs(heading_error_rad) <= math.pi / 2:
		return intent.direction_world_rad, 1
	return math.remainder(intent.direction_world_rad + math.pi, 2 * math.pi), -1


def keyboard_motion_intent(
	boat_heading_rad: float,
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
	brake: bool,
) -> MotionIntent:
	if brake:
		return MotionIntent(boat_heading_rad, brake=True)

	longitudinal = int(forward) - int(backward)
	turn = int(left) - int(right)
	motion = pymunk.Vec2d(longitudinal, -turn).rotated(boat_heading_rad)
	if not motion.length:
		return MotionIntent()
	return MotionIntent(motion.angle, min(1, motion.length))


def analog_motion_intent(
	boat_heading_rad: float,
	stick_x: float,
	stick_y: float,
	brake: bool,
) -> MotionIntent:
	if brake:
		return MotionIntent(boat_heading_rad, brake=True)

	magnitude = min(1.0, math.hypot(stick_x, stick_y))
	if magnitude <= ANALOG_STICK_DEADZONE:
		return MotionIntent()

	intensity = (magnitude - ANALOG_STICK_DEADZONE) / (1 - ANALOG_STICK_DEADZONE)
	desired_heading_rad = math.atan2(stick_y, stick_x)
	return MotionIntent(desired_heading_rad, intensity)


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
	stiffness_n_m_rad: float,
	damping_n_m_s_rad: float,
	max_torque_n_m: float,
	intensity: float = 1,
) -> float:
	torque_n_m = (
		(target_angle_rad - angle_rad) * stiffness_n_m_rad
		- angular_velocity_rad_s * damping_n_m_s_rad
	) * intensity
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


def optimal_sail_angle_for_drive(
	boat_angle_rad: float,
	apparent_wind_m_s: pymunk.Vec2d,
	drive_direction: int,
) -> float:
	boat_forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)

	def desired_force(sail_angle_rad: float) -> float:
		force = force_on_foil(
			apparent_wind_m_s,
			boat_angle_rad + sail_angle_rad,
			AIR_DENSITY_KG_M3,
			SAIL_AREA_M2,
		)
		return force.dot(boat_forward) * drive_direction

	candidates_rad = (
		math.radians(degrees)
		for degrees in range(-90, 91, SAIL_TRIM_RESOLUTION_DEG)
	)
	return max(candidates_rad, key=desired_force)


def optimal_sail_angle(boat_angle_rad: float, apparent_wind_m_s: pymunk.Vec2d) -> float:
	return optimal_sail_angle_for_drive(boat_angle_rad, apparent_wind_m_s, 1)


def luffing_sail_angle(boat_angle_rad: float, apparent_wind_m_s: pymunk.Vec2d) -> float:
	if not apparent_wind_m_s.length:
		return 0
	upstream_angle_rad = apparent_wind_m_s.angle + math.pi
	return math.remainder(upstream_angle_rad - boat_angle_rad, math.pi)


def intended_actuator_angles(
	boat_angle_rad: float,
	boat_angular_velocity_rad_s: float,
	intent: MotionIntent,
	apparent_wind_m_s: pymunk.Vec2d,
	apparent_water_m_s: pymunk.Vec2d,
) -> tuple[float | None, float | None]:
	desired_boat_heading_rad, drive_direction = resolve_motion_intent(
		boat_angle_rad,
		intent,
	)
	if desired_boat_heading_rad is None:
		return None, None

	heading_error_rad = math.remainder(
		desired_boat_heading_rad - boat_angle_rad,
		2 * math.pi,
	)
	maximum_yaw_rate_rad_s = TARGET_TURN_RATE_RAD_S
	target_yaw_rate_rad_s = max(
		-maximum_yaw_rate_rad_s,
		min(
			maximum_yaw_rate_rad_s,
			heading_error_rad * HEADING_TO_YAW_RATE_GAIN_PER_S,
		),
	)
	yaw_rate_error_rad_s = target_yaw_rate_rad_s - boat_angular_velocity_rad_s
	water_flow_along_boat_m_s = apparent_water_m_s.rotated(-boat_angle_rad).x
	rudder_direction = (
		int(water_flow_along_boat_m_s > 0)
		- int(water_flow_along_boat_m_s < 0)
	)
	target_rudder_angle_rad = max(
		-MAX_RUDDER_ANGLE_RAD,
		min(
			MAX_RUDDER_ANGLE_RAD,
			yaw_rate_error_rad_s * YAW_RATE_TO_RUDDER_GAIN_S * rudder_direction,
		),
	)

	if intent.brake:
		target_sail_angle_rad = luffing_sail_angle(boat_angle_rad, apparent_wind_m_s)
	else:
		target_sail_angle_rad = optimal_sail_angle_for_drive(
			boat_angle_rad,
			apparent_wind_m_s,
			drive_direction,
		)
	target_sail_angle_rad = max(
		-MAX_SAIL_ANGLE_RAD,
		min(MAX_SAIL_ANGLE_RAD, target_sail_angle_rad),
	)
	return target_sail_angle_rad, target_rudder_angle_rad


def create_simulation() -> SailingState:
	space = pymunk.Space()
	moment_kg_m2 = pymunk.moment_for_box(
		BOAT_MASS_KG,
		(BOAT_LENGTH_M, BOAT_BEAM_M),
	)
	boat = pymunk.Body(BOAT_MASS_KG, moment_kg_m2)
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

	sailor_sail_torque_n_m = 0.0
	control_intensity = 1 if intent.brake else intent.intensity
	if target_sail_angle_rad is not None:
		sailor_sail_torque_n_m = actuator_control_torque(
			state.sail_angle_rad,
			state.sail_angular_velocity_rad_s,
			target_sail_angle_rad,
			SAIL_CONTROL_STIFFNESS_N_M_RAD,
			SAIL_CONTROL_DAMPING_N_M_S_RAD,
			SAILOR_MAX_SHEET_FORCE_N * SAIL_SHEET_LEVER_ARM_M,
			control_intensity,
		)
	sailor_rudder_torque_n_m = 0.0
	if target_rudder_angle_rad is not None:
		sailor_rudder_torque_n_m = actuator_control_torque(
			state.rudder_angle_rad,
			state.rudder_angular_velocity_rad_s,
			target_rudder_angle_rad,
			RUDDER_CONTROL_STIFFNESS_N_M_RAD,
			RUDDER_CONTROL_DAMPING_N_M_S_RAD,
			SAILOR_MAX_TILLER_FORCE_N * TILLER_LENGTH_M,
			control_intensity,
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

	sail_moment_kg_m2 = (SAIL_MASS_KG + BOOM_MASS_KG) * BOOM_LENGTH_M**2 / 3
	state.sail_angle_rad, state.sail_angular_velocity_rad_s = integrate_hinge(
		state.sail_angle_rad,
		state.sail_angular_velocity_rad_s,
		sail_load.torque_n_m + sailor_sail_torque_n_m,
		sail_moment_kg_m2,
		SAIL_BEARING_DAMPING_N_M_S_RAD,
		MAX_SAIL_ANGLE_RAD,
		dt_seconds,
	)
	rudder_moment_kg_m2 = (
		RUDDER_MASS_KG * RUDDER_DRAW_LENGTH_M**2 / 3
		+ TILLER_MASS_KG * TILLER_LENGTH_M**2 / 3
	)
	state.rudder_angle_rad, state.rudder_angular_velocity_rad_s = integrate_hinge(
		state.rudder_angle_rad,
		state.rudder_angular_velocity_rad_s,
		rudder_load.torque_n_m + sailor_rudder_torque_n_m,
		rudder_moment_kg_m2,
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
	end = start + vector * scale
	pygame.draw.circle(screen, color, start, 4)
	pygame.draw.line(screen, color, start, end, 3)
	if vector.length:
		direction = vector.normalized()
		normal = direction.perpendicular()
		arrowhead = [
			end,
			end - direction * 10 + normal * 5,
			end - direction * 10 - normal * 5,
		]
		pygame.draw.polygon(screen, color, arrowhead)
	text = font.render(f"{label}: {vector.length:.1f} {unit}", True, color)
	screen.blit(text, (label_x, start.y - 10))


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

	intent_direction = intent_vector(intent)
	draw_arrow(
		screen,
		font,
		center,
		intent_direction,
		(240, 100, 220),
		"intent",
		INTENT_VECTOR_LENGTH_PX,
		"intensity",
		30,
	)

	control_status = "brake" if intent.brake else "neutral"
	if intent.direction_world_rad is not None and not intent.brake:
		_, drive_direction = resolve_motion_intent(boat.angle, intent)
		drive_name = "drive" if drive_direction > 0 else "backwind"
		control_status = f"{drive_name} {intent.intensity:.0%}"
	target_sail = "free"
	if telemetry.target_sail_angle_rad is not None:
		target_sail = f"{math.degrees(telemetry.target_sail_angle_rad):+.0f}°"
	target_rudder = "free"
	if telemetry.target_rudder_angle_rad is not None:
		target_rudder = f"{math.degrees(telemetry.target_rudder_angle_rad):+.0f}°"

	status = font.render(
		f"{control_status}    "
		f"speed through water {telemetry.apparent_water_m_s.length:.1f} m/s    "
		f"sail {math.degrees(state.sail_angle_rad):+.0f}° target {target_sail}    "
		f"rudder {math.degrees(state.rudder_angle_rad):+.0f}° target {target_rudder}",
		True,
		(220, 220, 230),
	)
	screen.blit(status, (30, HEIGHT - 35))


def main() -> None:
	pygame.init()
	screen = pygame.display.set_mode((WIDTH, HEIGHT))
	pygame.display.set_caption(
		"Sailboat — WASD motion, Space brake; left stick moves, button 0 brakes"
	)
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
			state.boat.angle,
			forward=keys[pygame.K_w],
			backward=keys[pygame.K_s],
			left=keys[pygame.K_a],
			right=keys[pygame.K_d],
			brake=keys[pygame.K_SPACE],
		)
		controller_intent = MotionIntent()
		if joystick is not None:
			controller_intent = analog_motion_intent(
				state.boat.angle,
				joystick.get_axis(0),
				joystick.get_axis(1),
				joystick.get_numbuttons() > 0 and joystick.get_button(0),
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
