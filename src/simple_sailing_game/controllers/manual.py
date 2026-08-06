from dataclasses import dataclass

from .actuator import (
	ActuatorState,
	ActuatorTorques,
)


@dataclass(frozen=True)
class ManualInput:
	sail: float = 0
	rudder: float = 0


def control(
	sail: ActuatorState,
	rudder: ActuatorState,
	manual_input: ManualInput,
	dt_seconds: float,
) -> ActuatorTorques:
	torques_n_m = []
	for actuator, command in zip(
		(sail, rudder),
		(manual_input.sail, manual_input.rudder),
	):
		command = max(-1, min(1, command))
		if command:
			torque_n_m = command * actuator.maximum_torque_n_m
		else:
			torque_n_m = (
				-actuator.moment_of_inertia_kg_m2
				* actuator.angular_velocity_rad_s
				/ dt_seconds
				- actuator.external_torque_n_m
				+ actuator.angular_velocity_rad_s
				* actuator.bearing_damping_n_m_s_rad
			)
		torques_n_m.append(
			max(
				-actuator.maximum_torque_n_m,
				min(actuator.maximum_torque_n_m, torque_n_m),
			)
		)
	return ActuatorTorques(*torques_n_m)
