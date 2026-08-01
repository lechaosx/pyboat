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
KEEL_AREA_M2 = 0.6
RUDDER_AREA_M2 = 0.12
RUDDER_DRAW_LENGTH_M = 0.5
HULL_REFERENCE_AREA_M2 = 0.3
HULL_DRAG_COEFFICIENT = 0.08
FOIL_BASE_DRAG_COEFFICIENT = 0.02
SAIL_TRIM_RESOLUTION_DEG = 1

RUDDER_RATE_RAD_S = math.radians(60)
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
	rudder_angle_rad: float = 0


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


def recentered_rudder_angle(angle_rad: float, direction: int, dt_seconds: float) -> float:
	if direction:
		angle_rad += direction * RUDDER_RATE_RAD_S * dt_seconds
	else:
		recenter_rad = min(abs(angle_rad), RUDDER_RATE_RAD_S * dt_seconds)
		angle_rad -= math.copysign(recenter_rad, angle_rad)
	return max(-MAX_RUDDER_ANGLE_RAD, min(MAX_RUDDER_ANGLE_RAD, angle_rad))


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
	trim: bool,
	rudder_direction: int,
	dt_seconds: float = DT_SECONDS,
) -> None:
	state.rudder_angle_rad = recentered_rudder_angle(
		state.rudder_angle_rad,
		rudder_direction,
		dt_seconds,
	)

	apparent_water_m_s = apparent_flow(
		environment.true_water_velocity_m_s,
		state.boat.velocity,
	)
	apparent_wind_m_s = apparent_flow(
		environment.true_wind_velocity_m_s,
		state.boat.velocity,
	)
	if trim:
		state.sail_angle_rad = optimal_sail_angle(state.boat.angle, apparent_wind_m_s)

	sail_force_n = force_on_foil(
		apparent_wind_m_s,
		state.boat.angle + state.sail_angle_rad,
		AIR_DENSITY_KG_M3,
		SAIL_AREA_M2,
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
	rudder_velocity_m_s = state.boat.velocity_at_local_point(RUDDER_POSITION_M)
	apparent_rudder_water_m_s = apparent_flow(
		environment.true_water_velocity_m_s,
		rudder_velocity_m_s,
	)
	rudder_force_n = force_on_foil(
		apparent_rudder_water_m_s,
		state.boat.angle + state.rudder_angle_rad,
		WATER_DENSITY_KG_M3,
		RUDDER_AREA_M2,
	)

	state.boat.apply_force_at_world_point(
		sail_force_n + keel_force_n + hull_drag_n,
		state.boat.position,
	)
	state.boat.apply_force_at_local_point(
		rudder_force_n.rotated(-state.boat.angle),
		RUDDER_POSITION_M,
	)
	state.space.step(dt_seconds)


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
	pygame.display.set_caption("Sailboat — hold W to trim, steer with A/D")
	font = pygame.font.Font(None, 24)
	clock = pygame.time.Clock()
	state = create_simulation()
	running = True

	while running:
		for event in pygame.event.get():
			if event.type == pygame.QUIT:
				running = False

		keys = pygame.key.get_pressed()
		rudder_direction = int(keys[pygame.K_a]) - int(keys[pygame.K_d])
		step(state, DEFAULT_ENVIRONMENT, keys[pygame.K_w], rudder_direction)
		draw(screen, font, state, DEFAULT_ENVIRONMENT)
		pygame.display.flip()
		clock.tick(round(1 / DT_SECONDS))

	pygame.quit()


if __name__ == "__main__":
	main()
