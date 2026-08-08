import math
from dataclasses import dataclass


@dataclass(frozen=True)
class ActuatorState:
	angle_rad: float
	angular_velocity_rad_s: float
	moment_of_inertia_kg_m2: float
	bearing_damping_n_m_s_rad: float
	maximum_torque_n_m: float
	maximum_angle_rad: float


def damped_control_torque(
	actuator: ActuatorState,
	target_angle_rad: float,
) -> float:
	angle_error_rad = target_angle_rad - actuator.angle_rad
	full_torque_error_rad = actuator.maximum_angle_rad / 10
	stiffness_n_m_rad = actuator.maximum_torque_n_m / full_torque_error_rad
	critical_damping_n_m_s_rad = 2 * math.sqrt(
		stiffness_n_m_rad * actuator.moment_of_inertia_kg_m2
	)
	torque_n_m = (
		stiffness_n_m_rad * angle_error_rad
		- critical_damping_n_m_s_rad * actuator.angular_velocity_rad_s
	)
	return max(
		-actuator.maximum_torque_n_m,
		min(actuator.maximum_torque_n_m, torque_n_m),
	)
