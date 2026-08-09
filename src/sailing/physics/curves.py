import math
from collections.abc import Callable


LiftCurve = Callable[[float], float]
DragCurve = Callable[[float, float], float]


def flat_plate_lift(angle_of_attack_rad: float) -> float:
	"""Approximate a symmetric flat plate, including loss past 45 degrees."""
	return math.sin(2 * angle_of_attack_rad)


def flat_plate_drag(
	angle_of_attack_rad: float,
	base_drag_coefficient: float,
) -> float:
	"""Combine profile drag with flat-plate pressure drag."""
	return base_drag_coefficient + 1 - math.cos(2 * angle_of_attack_rad)


def streamlined_lift(
	stall_angle_rad: float,
	stall_lift_coefficient: float,
) -> LiftCurve:
	"""Build an attached-flow curve that transitions to separated cross-flow."""

	def lift(angle_of_attack_rad: float) -> float:
		angle_of_attack_rad = math.remainder(angle_of_attack_rad, math.pi)
		abs_angle_rad = abs(angle_of_attack_rad)
		if abs_angle_rad <= stall_angle_rad:
			return (
				stall_lift_coefficient
				* angle_of_attack_rad
				/ stall_angle_rad
			)
		post_stall_lift = stall_lift_coefficient * (
			(math.pi / 2 - abs_angle_rad)
			/ (math.pi / 2 - stall_angle_rad)
		)
		post_stall_lift = max(
			post_stall_lift,
			abs(flat_plate_lift(angle_of_attack_rad)),
		)
		return math.copysign(post_stall_lift, angle_of_attack_rad)

	return lift
