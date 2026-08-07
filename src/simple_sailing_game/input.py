import math

import pymunk

from .controllers import arcade, assisted, manual


def keyboard_motion(
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
) -> assisted.MotionIntent:
	longitudinal = int(forward) - int(backward)
	turn = int(left) - int(right)
	return assisted.MotionIntent(longitudinal, -turn)


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
	rudder_forward: bool,
	rudder_backward: bool,
	rudder_left: bool,
	rudder_right: bool,
	sail_forward: bool,
	sail_backward: bool,
	sail_left: bool,
	sail_right: bool,
) -> manual.DirectionInput:
	def direction(
		forward: bool,
		backward: bool,
		left: bool,
		right: bool,
	) -> tuple[float, float]:
		vector = pymunk.Vec2d(
			int(forward) - int(backward),
			int(right) - int(left),
		)
		return (0, 0) if not vector.length else tuple(vector.normalized())

	return manual.DirectionInput(
		sail_direction=direction(
			sail_forward, sail_backward, sail_left, sail_right
		),
		rudder_direction=direction(
			rudder_forward, rudder_backward, rudder_left, rudder_right
		),
	)


def analog_manual(
	boat_heading_rad: float,
	rudder_stick_x: float,
	rudder_stick_y: float,
	sail_stick_x: float,
	sail_stick_y: float,
	deadzone: float,
) -> manual.DirectionInput:
	def direction(x: float, y: float) -> tuple[float, float]:
		vector = pymunk.Vec2d(x, y)
		if vector.length <= deadzone:
			return (0, 0)
		return tuple(vector.normalized().rotated(-boat_heading_rad))

	return manual.DirectionInput(
		sail_direction=direction(sail_stick_x, sail_stick_y),
		rudder_direction=direction(rudder_stick_x, rudder_stick_y),
	)


def analog_motion(
	boat_heading_rad: float,
	stick_x: float,
	stick_y: float,
	deadzone: float,
) -> assisted.MotionIntent:
	world_vector = pymunk.Vec2d(stick_x, stick_y)
	magnitude = min(1.0, world_vector.length)
	if magnitude <= deadzone:
		return assisted.MotionIntent()
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
	return assisted.MotionIntent(forward, turn)


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
