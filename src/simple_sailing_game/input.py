import math

import pymunk

from .controllers import arcade, manual


def keyboard_motion(
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
) -> arcade.MotionIntent:
	longitudinal = int(forward) - int(backward)
	turn = int(left) - int(right)
	return arcade.MotionIntent(longitudinal, -turn)


def keyboard_heading(
	boat_heading_rad: float,
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
	explicit_reverse: bool,
) -> arcade.HeadingIntent:
	world_direction = pymunk.Vec2d(
		int(forward) - int(backward),
		int(right) - int(left),
	).rotated(boat_heading_rad)
	return arcade.HeadingIntent(tuple(world_direction), explicit_reverse)


def keyboard_auto_sheet(
	forward: bool,
	left: bool,
	right: bool,
) -> arcade.AutoSheetIntent:
	return arcade.AutoSheetIntent(
		power=int(forward),
		rudder=int(left) - int(right),
	)


def keyboard_manual(
	sheet_in: bool,
	sheet_out: bool,
	rudder_left: bool,
	rudder_right: bool,
) -> manual.Input:
	return manual.Input(
		sheet=int(sheet_in) - int(sheet_out),
		rudder=int(rudder_right) - int(rudder_left),
	)


def analog_manual(
	stick_x: float,
	stick_y: float,
	deadzone: float,
) -> manual.Input:
	def axis(value: float) -> float:
		if abs(value) <= deadzone:
			return 0
		return max(-1, min(1, value))

	return manual.Input(
		sheet=axis(-stick_y),
		rudder=axis(stick_x),
	)


def analog_motion(
	boat_heading_rad: float,
	stick_x: float,
	stick_y: float,
	deadzone: float,
) -> arcade.MotionIntent:
	world_vector = pymunk.Vec2d(stick_x, stick_y)
	magnitude = min(1.0, world_vector.length)
	if magnitude <= deadzone:
		return arcade.MotionIntent()
	if math.isclose(magnitude, 1):
		magnitude = 1
	local_direction = world_vector.normalized().rotated(-boat_heading_rad)
	square_scale = max(abs(local_direction.x), abs(local_direction.y))
	local_intent = local_direction / square_scale * magnitude
	zero_tolerance = math.ulp(2 * math.pi)
	forward = (
		0
		if math.isclose(local_intent.x, 0, abs_tol=zero_tolerance)
		else local_intent.x
	)
	turn = (
		0
		if math.isclose(local_intent.y, 0, abs_tol=zero_tolerance)
		else local_intent.y
	)
	return arcade.MotionIntent(forward, turn)


def analog_heading(
	stick_x: float,
	stick_y: float,
	explicit_reverse: bool,
	deadzone: float,
) -> arcade.HeadingIntent:
	world_vector = pymunk.Vec2d(stick_x, stick_y)
	world_direction = (0, 0)
	if world_vector.length > deadzone:
		world_direction = (stick_x, stick_y)
	return arcade.HeadingIntent(world_direction, explicit_reverse)


def analog_auto_sheet(
	power_axis: float,
	rudder_axis: float,
) -> arcade.AutoSheetIntent:
	return arcade.AutoSheetIntent(
		power=max(0, min(1, -power_axis)),
		rudder=max(-1, min(1, rudder_axis)),
	)
