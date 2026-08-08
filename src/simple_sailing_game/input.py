import math

import pymunk

from .controllers import arcade


def keyboard_motion(
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
) -> arcade.MotionIntent:
	longitudinal = int(forward) - int(backward)
	turn = int(left) - int(right)
	return arcade.MotionIntent(longitudinal, -turn)


def keyboard_auto_sheet(
	forward: bool,
	left: bool,
	right: bool,
) -> arcade.AutoSheetIntent:
	return arcade.AutoSheetIntent(
		power=int(forward),
		rudder=int(left) - int(right),
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


def analog_auto_sheet(
	power_axis: float,
	rudder_axis: float,
) -> arcade.AutoSheetIntent:
	return arcade.AutoSheetIntent(
		power=max(0, min(1, -power_axis)),
		rudder=max(-1, min(1, rudder_axis)),
	)
