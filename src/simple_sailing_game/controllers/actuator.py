from dataclasses import dataclass


@dataclass(frozen=True)
class ActuatorState:
	angle_rad: float
	angular_velocity_rad_s: float
	external_torque_n_m: float
	moment_of_inertia_kg_m2: float
	bearing_damping_n_m_s_rad: float
	maximum_torque_n_m: float
	maximum_angle_rad: float


@dataclass(frozen=True)
class ActuatorTorques:
	sail_n_m: float = 0
	rudder_n_m: float = 0
