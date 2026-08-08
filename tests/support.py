import math

import pymunk

from simple_sailing_game import boat, game
from simple_sailing_game.controllers import assisted as controller


def foil_control_effect(
	boat_angle_rad: float,
	apparent_flow_m_s: pymunk.Vec2d,
	hinge_position_local_m: tuple[float, float],
	center_distance_m: float,
	angle_rad: float,
	fluid_density_kg_m3: float,
	area_m2: float,
	boat_angular_velocity_rad_s: float = 0,
) -> controller.ControlEffect:
	center_local_m = (
		pymunk.Vec2d(*hinge_position_local_m)
		+ pymunk.Vec2d(-center_distance_m, 0).rotated(angle_rad)
	)
	center_world_m = center_local_m.rotated(boat_angle_rad)
	center_flow_m_s = (
		apparent_flow_m_s
		- center_world_m.perpendicular() * boat_angular_velocity_rad_s
	)
	force_n = boat.force_on_foil(
		center_flow_m_s,
		boat_angle_rad + angle_rad,
		fluid_density_kg_m3,
		area_m2,
		game.BOAT_PARAMETERS.underwater_foil_base_drag_coefficient,
	)
	forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)
	return controller.ControlEffect(
		angle_rad,
		center_world_m.cross(force_n),
		force_n.dot(forward),
		force_n.length,
	)


def controller_input_from_flows(
	boat_angle_rad: float,
	boat_angular_velocity_rad_s: float,
	apparent_wind_m_s: pymunk.Vec2d,
	apparent_water_m_s: pymunk.Vec2d,
	sail_angle_rad: float = 0,
	rudder_angle_rad: float = 0,
) -> controller.ControlInput:
	parameters = game.BOAT_PARAMETERS

	def foil_effect(
		apparent_flow_m_s: pymunk.Vec2d,
		hinge_position_local_m: tuple[float, float],
		center_distance_m: float,
		angle_rad: float,
		fluid_density_kg_m3: float,
		area_m2: float,
		base_drag_coefficient: float,
	) -> controller.ControlEffect:
		center_local_m = (
			pymunk.Vec2d(*hinge_position_local_m)
			+ pymunk.Vec2d(-center_distance_m, 0).rotated(angle_rad)
		)
		center_world_m = center_local_m.rotated(boat_angle_rad)
		center_flow_m_s = (
			apparent_flow_m_s
			- center_world_m.perpendicular() * boat_angular_velocity_rad_s
		)
		force_n = boat.force_on_foil(
			center_flow_m_s,
			boat_angle_rad + angle_rad,
			fluid_density_kg_m3,
			area_m2,
			base_drag_coefficient,
		)
		forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)
		return controller.ControlEffect(
			angle_rad,
			center_world_m.cross(force_n),
			force_n.dot(forward),
			force_n.length,
		)

	def sail_effect_at(angle_rad: float) -> controller.ControlEffect:
		return foil_effect(
			apparent_wind_m_s,
			parameters.mast_position_m,
			parameters.sail_center_of_effort_from_mast_m,
			angle_rad,
			parameters.air_density_kg_m3,
			parameters.sail_area_m2,
			parameters.sail_base_drag_coefficient,
		)

	def rudder_effect_at(angle_rad: float) -> controller.ControlEffect:
		return foil_effect(
			apparent_water_m_s,
			parameters.rudder_position_m,
			parameters.rudder_draw_length_m / 2,
			angle_rad,
			parameters.water_density_kg_m3,
			parameters.rudder_area_m2,
			parameters.underwater_foil_base_drag_coefficient,
		)

	def samples(effect_at, maximum_angle_rad):
		maximum_angle_deg = round(math.degrees(maximum_angle_rad))
		return tuple(
			effect_at(math.radians(angle_deg))
			for angle_deg in range(-maximum_angle_deg, maximum_angle_deg + 1)
		)

	return controller.ControlInput(
		boat_angular_velocity_rad_s,
		boat.lumped_moment_of_inertia_kg_m2(parameters),
		controller.ActuatorState(
			sail_angle_rad,
			0,
			0,
			boat.sail_moment_of_inertia_kg_m2(parameters),
			parameters.sail_bearing_damping_n_m_s_rad,
			game.SAILOR_MAX_SHEET_FORCE_N * parameters.sail_sheet_lever_arm_m,
			parameters.max_sail_angle_rad,
		),
		controller.ActuatorState(
			rudder_angle_rad,
			0,
			0,
			boat.rudder_moment_of_inertia_kg_m2(parameters),
			parameters.rudder_bearing_damping_n_m_s_rad,
			game.SAILOR_MAX_TILLER_FORCE_N * parameters.tiller_length_m,
			parameters.max_rudder_angle_rad,
		),
		samples(sail_effect_at, parameters.max_sail_angle_rad),
		samples(rudder_effect_at, parameters.max_rudder_angle_rad),
		sail_effect_at,
		rudder_effect_at,
		boat.luffing_sail_angle(boat_angle_rad, apparent_wind_m_s),
	)


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
	settings = controller.ControllerSettings(
		mode=controller.OPTIMIZED_TARGET_CONTROL,
		use_global_slope=False,
		angle_resolution_deg=game.CONTROL_ANGLE_RESOLUTION_DEG,
	)
	decision = controller.control_decision(control_input, intent, settings)
	assert decision.target_angles_rad is not None
	return decision.target_angles_rad
