import math

import pygame
import pymunk

from . import boat, course
from .controllers import arcade, manual
from .simulation import SimulationTelemetry


WIDTH, HEIGHT = 1280, 720
PIXELS_PER_METER = 60
FLOW_PIXELS_PER_METER_SECOND = 20
FORCE_PIXELS_PER_NEWTON = 0.35
INTENT_VECTOR_LENGTH_PX = 100

PlayerIntent = (
	arcade.MotionIntent
	| manual.Input
	| arcade.AutoSheetIntent
	| arcade.HeadingIntent
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
	pygame.draw.polygon(
		screen,
		color,
		[
			end,
			end - direction * 10 + normal * 5,
			end - direction * 10 - normal * 5,
		],
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
	pygame.draw.circle(screen, color, start, 4)
	draw_vector(screen, start, vector, color, scale, 3)
	text = font.render(f"{label}: {vector.length:.1f} {unit}", True, color)
	screen.blit(text, (label_x, start.y - 10))


def intent_vector(
	intent: arcade.MotionIntent,
	boat_heading_rad: float,
) -> pymunk.Vec2d:
	return pymunk.Vec2d(intent.forward, intent.turn).rotated(boat_heading_rad)


def heading_intent_vector(intent: arcade.HeadingIntent) -> pymunk.Vec2d:
	return pymunk.Vec2d(*intent.world_direction)


def draw_intent(
	screen: pygame.Surface,
	font: pygame.font.Font,
	start: pymunk.Vec2d,
	boat_heading_rad: float,
	intent: PlayerIntent,
) -> None:
	if isinstance(intent, arcade.HeadingIntent):
		world_direction = heading_intent_vector(intent)
		if world_direction.length > 1:
			world_direction = world_direction.normalized()
		draw_vector(
			screen,
			start,
			world_direction,
			(240, 100, 220),
			INTENT_VECTOR_LENGTH_PX,
			3,
		)
		reverse = "  reverse modifier" if intent.explicit_reverse else ""
		text = font.render(
			f"heading input: {world_direction.x:+.0%}, {world_direction.y:+.0%}{reverse}",
			True,
			(240, 100, 220),
		)
		screen.blit(text, (30, start.y - 10))
		return

	if isinstance(intent, arcade.AutoSheetIntent):
		power = pymunk.Vec2d(intent.power, 0).rotated(boat_heading_rad)
		rudder = pymunk.Vec2d(0, intent.rudder).rotated(boat_heading_rad)
		draw_vector(
			screen, start, power, (100, 220, 140), INTENT_VECTOR_LENGTH_PX, 3
		)
		draw_vector(
			screen, start, rudder, (255, 160, 80), INTENT_VECTOR_LENGTH_PX, 3
		)
		text = font.render(
			f"auto-sheet: power {intent.power:+.0%}  rudder {intent.rudder:+.0%}",
			True,
			(240, 100, 220),
		)
		screen.blit(text, (30, start.y - 10))
		return

	if isinstance(intent, manual.Input):
		sheet = pymunk.Vec2d(intent.sheet, 0).rotated(boat_heading_rad)
		rudder = pymunk.Vec2d(0, intent.rudder).rotated(boat_heading_rad)
		draw_vector(
			screen, start, sheet, (235, 205, 100), INTENT_VECTOR_LENGTH_PX, 3
		)
		draw_vector(
			screen, start, rudder, (220, 220, 230), INTENT_VECTOR_LENGTH_PX, 3
		)
		text = font.render(
			f"manual: sheet {intent.sheet:+.0%}  rudder {intent.rudder:+.0%}",
			True,
			(240, 100, 220),
		)
		screen.blit(text, (30, start.y - 10))
		return

	forward_vector = pymunk.Vec2d(intent.forward, 0).rotated(boat_heading_rad)
	turn_vector = pymunk.Vec2d(0, intent.turn).rotated(boat_heading_rad)
	forward_end = start + forward_vector * INTENT_VECTOR_LENGTH_PX
	draw_vector(
		screen, start, forward_vector, (100, 220, 140), INTENT_VECTOR_LENGTH_PX, 2
	)
	draw_vector(
		screen, forward_end, turn_vector, (255, 160, 80), INTENT_VECTOR_LENGTH_PX, 2
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
	state: boat.SailingState,
	environment: boat.Environment,
	intent: PlayerIntent,
	telemetry: SimulationTelemetry,
	parameters: boat.BoatParameters,
	time_trial: course.TimeTrial | None = None,
) -> None:
	center = pymunk.Vec2d(WIDTH / 2, HEIGHT / 2)
	body = state.boat
	observation = telemetry.physics.observation
	screen.fill((30, 30, 40))

	grid_spacing_px = PIXELS_PER_METER
	x_offset = -body.position.x * PIXELS_PER_METER % grid_spacing_px
	y_offset = -body.position.y * PIXELS_PER_METER % grid_spacing_px
	for x in range(round(x_offset), WIDTH, grid_spacing_px):
		pygame.draw.line(screen, (38, 46, 58), (x, 0), (x, HEIGHT))
	for y in range(round(y_offset), HEIGHT, grid_spacing_px):
		pygame.draw.line(screen, (38, 46, 58), (0, y), (WIDTH, y))
	if time_trial is not None:
		for index, buoy_m in enumerate(time_trial.buoys_m):
			position = (
				pymunk.Vec2d(*buoy_m) - body.position
			) * PIXELS_PER_METER + center
			color = (
				(255, 100, 220)
				if index == time_trial.next_buoy_index
				else (240, 180, 70)
			)
			pygame.draw.circle(
				screen,
				color,
				position,
				round(time_trial.buoy_radius_m * PIXELS_PER_METER),
				3,
			)
			screen.blit(font.render(str(index + 1), True, color), position + (5, 5))

	half_length_px = parameters.length_m * PIXELS_PER_METER / 2
	half_beam_px = parameters.beam_m * PIXELS_PER_METER / 2
	bow_base_px = (
		parameters.length_m / 2 - parameters.bow_length_m
	) * PIXELS_PER_METER
	hull_points = [
		pymunk.Vec2d(half_length_px, 0),
		pymunk.Vec2d(bow_base_px, -half_beam_px),
		pymunk.Vec2d(-half_length_px, -half_beam_px),
		pymunk.Vec2d(-half_length_px, half_beam_px),
		pymunk.Vec2d(bow_base_px, half_beam_px),
	]
	pygame.draw.polygon(
		screen,
		(205, 205, 220),
		[point.rotated(body.angle) + center for point in hull_points],
		3,
	)

	sail_direction = pymunk.Vec2d(-1, 0).rotated(
		body.angle + state.sail_angle_rad
	)
	pygame.draw.line(
		screen,
		(235, 205, 100),
		center,
		center + sail_direction * parameters.boom_length_m * PIXELS_PER_METER,
		4,
	)
	rudder_position = (
		pymunk.Vec2d(*parameters.rudder_position_m).rotated(body.angle)
		* PIXELS_PER_METER
		+ center
	)
	rudder_direction = pymunk.Vec2d(-1, 0).rotated(
		body.angle + state.rudder_angle_rad
	)
	pygame.draw.line(
		screen,
		(220, 220, 230),
		rudder_position,
		rudder_position
		+ rudder_direction
		* parameters.rudder_draw_length_m
		* PIXELS_PER_METER,
		4,
	)

	flows = (
		("true wind", environment.true_wind_velocity_m_s, (80, 180, 255)),
		("apparent wind", observation.apparent_wind_m_s, (220, 220, 255)),
		("true water", environment.true_water_velocity_m_s, (80, 110, 230)),
		("apparent water", observation.apparent_water_m_s, (80, 220, 180)),
		("boat velocity", body.velocity, (130, 230, 130)),
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
		("sail force", observation.sail_load.force_n, (235, 205, 100)),
		("keel force", telemetry.physics.keel_force_n, (80, 220, 180)),
		("hull drag", telemetry.physics.hull_drag_n, (180, 150, 120)),
		("rudder force", observation.rudder_load.force_n, (220, 220, 230)),
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

	draw_intent(screen, font, center, body.angle, intent)
	status = font.render(
		f"{telemetry.control_mode} / articulated physics    "
		f"water speed {observation.apparent_water_m_s.length:.1f} m/s    "
		f"sail {math.degrees(state.sail_angle_rad):+.0f}°    "
		f"rudder {math.degrees(state.rudder_angle_rad):+.0f}°",
		True,
		(220, 220, 230),
	)
	screen.blit(status, (30, HEIGHT - 35))
	upwind_angle_rad = observation.apparent_wind_m_s.angle + math.pi
	point_of_sail = course.point_of_sail(body.angle - upwind_angle_rad)
	feedback = f"point of sail: {point_of_sail}"
	free_sail_angle_rad = boat.free_sail_angle(
		body.angle,
		observation.apparent_wind_m_s,
		parameters.max_sail_angle_rad,
	)
	if telemetry.surface_targets.sail_target_angle_rad is None:
		target_sail_angle_rad = max(
			-telemetry.surface_targets.sheet_limit_angle_rad,
			min(
				telemetry.surface_targets.sheet_limit_angle_rad,
				free_sail_angle_rad,
			),
		)
	else:
		target_sail_angle_rad = telemetry.surface_targets.sail_target_angle_rad
	feedback += (
		f"    trim: {course.trim_feedback(state.sail_angle_rad, target_sail_angle_rad)}"
	)
	if time_trial is not None:
		best = (
			"--"
			if time_trial.best_time_seconds is None
			else f"{time_trial.best_time_seconds:.1f} s"
		)
		feedback += (
			f"    mark {time_trial.next_buoy_index + 1}/{len(time_trial.buoys_m)}"
			f"    time {time_trial.elapsed_seconds:.1f} s    best {best}"
		)
	screen.blit(
		font.render(feedback, True, (220, 220, 230)), (30, HEIGHT - 60)
	)
