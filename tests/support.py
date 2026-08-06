from dataclasses import replace

import pymunk

from simple_sailing_game import boat, controller, game


def controller_input_from_flows(
	boat_angle_rad: float,
	boat_angular_velocity_rad_s: float,
	apparent_wind_m_s: pymunk.Vec2d,
	apparent_water_m_s: pymunk.Vec2d,
	sail_angle_rad: float = 0,
	rudder_angle_rad: float = 0,
) -> controller.ControlInput:
	state = game.create_simulation()
	state.boat.angle = boat_angle_rad
	state.boat.angular_velocity = boat_angular_velocity_rad_s
	state.sail_angle_rad = sail_angle_rad
	state.rudder_angle_rad = rudder_angle_rad
	environment = boat.Environment(apparent_wind_m_s, apparent_water_m_s)
	observation = boat.observe(state, environment, game.BOAT_PARAMETERS)
	return game.controller_input(observation)


def intended_actuator_angles(
	boat_angle_rad: float,
	boat_angular_velocity_rad_s: float,
	intent: controller.MotionIntent,
	apparent_wind_m_s: pymunk.Vec2d,
	apparent_water_m_s: pymunk.Vec2d,
) -> tuple[float, float]:
	control_input = controller_input_from_flows(
		boat_angle_rad,
		boat_angular_velocity_rad_s,
		apparent_wind_m_s,
		apparent_water_m_s,
	)
	settings = replace(
		game.CONTROLLER_SETTINGS,
		mode=controller.OPTIMIZED_TARGET_CONTROL,
	)
	decision = controller.control_decision(control_input, intent, settings)
	assert decision.target_angles_rad is not None
	return decision.target_angles_rad
