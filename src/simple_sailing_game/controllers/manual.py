import math
from dataclasses import dataclass

from .actuator import (
	ActuatorState,
	ActuatorTorques,
	damped_control_torque,
)


@dataclass(frozen=True)
class DirectionInput:
	sail_direction: tuple[float, float] = (0, 0)
	rudder_direction: tuple[float, float] = (0, 0)


def direction_angle_rad(
	direction: tuple[float, float],
	maximum_angle_rad: float,
) -> float | None:
	x, y = direction
	if not math.hypot(x, y):
		return None
	angle_rad = math.remainder(math.atan2(y, x), 2 * math.pi)
	return max(-maximum_angle_rad, min(maximum_angle_rad, angle_rad))


def control(
	sail: ActuatorState,
	rudder: ActuatorState,
	manual_input: DirectionInput,
) -> ActuatorTorques:
	torques_n_m = []
	for actuator, direction in zip(
		(sail, rudder),
		(manual_input.sail_direction, manual_input.rudder_direction),
	):
		target_angle_rad = direction_angle_rad(
			direction,
			actuator.maximum_angle_rad,
		)
		torque_n_m = (
			0
			if target_angle_rad is None
			else damped_control_torque(actuator, target_angle_rad)
		)
		torques_n_m.append(torque_n_m)
	return ActuatorTorques(*torques_n_m)
