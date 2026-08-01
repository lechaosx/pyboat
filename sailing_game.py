import math
from dataclasses import dataclass

import pygame
import pymunk


WIDTH, HEIGHT = 1280, 720
PIXELS_PER_METER = 60
FLOW_PIXELS_PER_METER_SECOND = 20
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
	held_heading_rad: float | None = None


@dataclass(frozen=True)
class SailorIntent:
	forward: bool = False
	turn: int = 0
	stop: bool = False


@dataclass(frozen=True)
class HingeLoad:
	force_n: pymunk.Vec2d
	center_local_m: pymunk.Vec2d
	torque_n_m: float


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
) -> float:
	torque_n_m = (
		(target_angle_rad - angle_rad) * stiffness_n_m_rad
		- angular_velocity_rad_s * damping_n_m_s_rad
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


def optimal_sail_angle(boat_angle_rad: float, apparent_wind_m_s: pymunk.Vec2d) -> float:
	boat_forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)

	def forward_force(sail_angle_rad: float) -> float:
		force = force_on_foil(
			apparent_wind_m_s,
			boat_angle_rad + sail_angle_rad,
			AIR_DENSITY_KG_M3,
			SAIL_AREA_M2,
		)
		return force.dot(boat_forward)

	candidates_rad = (
		math.radians(degrees)
		for degrees in range(-90, 91, SAIL_TRIM_RESOLUTION_DEG)
	)
	return max(candidates_rad, key=forward_force)


def luffing_sail_angle(boat_angle_rad: float, apparent_wind_m_s: pymunk.Vec2d) -> float:
	if not apparent_wind_m_s.length:
		return 0
	upstream_angle_rad = apparent_wind_m_s.angle + math.pi
	return math.remainder(upstream_angle_rad - boat_angle_rad, math.pi)


def intended_actuator_angles(
	boat_angle_rad: float,
	boat_angular_velocity_rad_s: float,
	held_heading_rad: float | None,
	intent: SailorIntent,
	apparent_wind_m_s: pymunk.Vec2d,
) -> tuple[float | None, float | None, float | None]:
	controlled = intent.forward or intent.stop or bool(intent.turn)
	if not controlled:
		return None, None, None

	if held_heading_rad is None:
		held_heading_rad = boat_angle_rad
	if intent.turn:
		held_heading_rad = boat_angle_rad
		target_yaw_rate_rad_s = -intent.turn * TARGET_TURN_RATE_RAD_S
	else:
		heading_error_rad = math.remainder(
			held_heading_rad - boat_angle_rad,
			2 * math.pi,
		)
		target_yaw_rate_rad_s = heading_error_rad * HEADING_TO_YAW_RATE_GAIN_PER_S
	yaw_rate_error_rad_s = target_yaw_rate_rad_s - boat_angular_velocity_rad_s
	target_rudder_angle_rad = max(
		-MAX_RUDDER_ANGLE_RAD,
		min(
			MAX_RUDDER_ANGLE_RAD,
			-yaw_rate_error_rad_s * YAW_RATE_TO_RUDDER_GAIN_S,
		),
	)

	if intent.stop:
		target_sail_angle_rad = luffing_sail_angle(boat_angle_rad, apparent_wind_m_s)
	else:
		target_sail_angle_rad = optimal_sail_angle(boat_angle_rad, apparent_wind_m_s)
	return held_heading_rad, target_sail_angle_rad, target_rudder_angle_rad


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
	intent: SailorIntent,
	dt_seconds: float = DT_SECONDS,
) -> None:
	apparent_water_m_s = apparent_flow(
		environment.true_water_velocity_m_s,
		state.boat.velocity,
	)
	apparent_wind_m_s = apparent_flow(
		environment.true_wind_velocity_m_s,
		state.boat.velocity,
	)
	state.held_heading_rad, target_sail_angle_rad, target_rudder_angle_rad = (
		intended_actuator_angles(
			state.boat.angle,
			state.boat.angular_velocity,
			state.held_heading_rad,
			intent,
			apparent_wind_m_s,
		)
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
	if target_sail_angle_rad is not None:
		sailor_sail_torque_n_m = actuator_control_torque(
			state.sail_angle_rad,
			state.sail_angular_velocity_rad_s,
			target_sail_angle_rad,
			SAIL_CONTROL_STIFFNESS_N_M_RAD,
			SAIL_CONTROL_DAMPING_N_M_S_RAD,
			SAILOR_MAX_SHEET_FORCE_N * SAIL_SHEET_LEVER_ARM_M,
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


def draw_arrow(
	screen: pygame.Surface,
	font: pygame.font.Font,
	start: pymunk.Vec2d,
	vector: pymunk.Vec2d,
	color: tuple[int, int, int],
	label: str,
) -> None:
	end = start + vector * FLOW_PIXELS_PER_METER_SECOND
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
	text = font.render(f"{label}: {vector.length:.1f} m/s", True, color)
	screen.blit(text, (30, start.y - 10))


def draw(
	screen: pygame.Surface,
	font: pygame.font.Font,
	state: SailingState,
	environment: Environment,
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

	apparent_wind_m_s = apparent_flow(
		environment.true_wind_velocity_m_s,
		boat.velocity,
	)
	apparent_water_m_s = apparent_flow(
		environment.true_water_velocity_m_s,
		boat.velocity,
	)
	flows = (
		("true wind", environment.true_wind_velocity_m_s, (80, 180, 255)),
		("apparent wind", apparent_wind_m_s, (220, 220, 255)),
		("true water", environment.true_water_velocity_m_s, (80, 110, 230)),
		("apparent water", apparent_water_m_s, (80, 220, 180)),
	)
	for row, (label, vector, color) in enumerate(flows):
		draw_arrow(
			screen,
			font,
			pymunk.Vec2d(240, 45 + row * 55),
			vector,
			color,
			label,
		)

	status = font.render(
		f"speed through water {apparent_water_m_s.length:.1f} m/s    "
		f"sail {math.degrees(state.sail_angle_rad):+.0f}°    "
		f"rudder {math.degrees(state.rudder_angle_rad):+.0f}°",
		True,
		(220, 220, 230),
	)
	screen.blit(status, (30, HEIGHT - 35))


def main() -> None:
	pygame.init()
	screen = pygame.display.set_mode((WIDTH, HEIGHT))
	pygame.display.set_caption("Sailboat — W go, A/D turn, S stop")
	font = pygame.font.Font(None, 24)
	clock = pygame.time.Clock()
	state = create_simulation()
	running = True

	while running:
		for event in pygame.event.get():
			if event.type == pygame.QUIT:
				running = False

		keys = pygame.key.get_pressed()
		intent = SailorIntent(
			forward=keys[pygame.K_w],
			turn=int(keys[pygame.K_a]) - int(keys[pygame.K_d]),
			stop=keys[pygame.K_s],
		)
		step(state, DEFAULT_ENVIRONMENT, intent)
		draw(screen, font, state, DEFAULT_ENVIRONMENT)
		pygame.display.flip()
		clock.tick(round(1 / DT_SECONDS))

	pygame.quit()


if __name__ == "__main__":
	main()
