from .. import boat
from . import common


def step(
	state: boat.SailingState,
	environment: boat.Environment,
	parameters: boat.BoatParameters,
	actuator_torques: boat.ActuatorTorques,
	dt_seconds: float,
) -> boat.PhysicsTelemetry:
	telemetry = common.step_hull(state, environment, parameters, dt_seconds)
	observation = telemetry.observation
	state.sail_angle_rad, state.sail_angular_velocity_rad_s = boat.integrate_hinge(
		state.sail_angle_rad,
		state.sail_angular_velocity_rad_s,
		observation.sail_load.torque_n_m + actuator_torques.sail_n_m,
		boat.sail_moment_of_inertia_kg_m2(parameters),
		parameters.sail_bearing_damping_n_m_s_rad,
		parameters.max_sail_angle_rad,
		dt_seconds,
	)
	state.rudder_angle_rad, state.rudder_angular_velocity_rad_s = boat.integrate_hinge(
		state.rudder_angle_rad,
		state.rudder_angular_velocity_rad_s,
		observation.rudder_load.torque_n_m + actuator_torques.rudder_n_m,
		boat.rudder_moment_of_inertia_kg_m2(parameters),
		parameters.rudder_bearing_damping_n_m_s_rad,
		parameters.max_rudder_angle_rad,
		dt_seconds,
	)
	return telemetry
