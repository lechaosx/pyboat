import math

import pymunk

from sailing import boat, simulation
from sailing.physics import curves


DT_SECONDS = 1 / 60
INITIAL_BOAT_ANGLE_RAD = math.radians(135)
CONTROL_ANGLE_RESOLUTION_DEG = 1
SAILOR_MAX_SHEET_FORCE_N = 250
SAILOR_MAX_TILLER_FORCE_N = 60
FIXED_MAXIMUM_YAW_RATE_RAD_S = math.radians(35)
SAIL_SIDE_HYSTERESIS_ANGLE_RAD = math.radians(10)
COURSE_BUOYS_M = ((0, 0), (-15, 15), (-30, 0), (-15, -15))
BUOY_RADIUS_M = 2
UNDERWATER_FOIL_LIFT_CURVE = curves.streamlined_lift(
	stall_angle_rad=math.radians(15),
	stall_lift_coefficient=0.8,
)

BOAT_PARAMETERS = boat.BoatParameters(
	air_density_kg_m3=1.225,
	water_density_kg_m3=1025,
	hull_mass_kg=222,
	length_m=4,
	beam_m=1.5,
	bow_length_m=0.7,
	sail_foil=boat.FoilProperties(
		area_m2=8,
		lift_coefficient=curves.flat_plate_lift,
		drag_coefficient=curves.flat_plate_drag,
		base_drag_coefficient=0.02,
	),
	boom_length_m=2,
	boom_mass_kg=10,
	mast_position_m=(0.4, 0),
	sail_center_of_effort_from_mast_m=0.7,
	sail_mass_kg=12,
	keel_foil=boat.FoilProperties(
		area_m2=0.6,
		lift_coefficient=UNDERWATER_FOIL_LIFT_CURVE,
		drag_coefficient=curves.flat_plate_drag,
		base_drag_coefficient=0.02,
	),
	keel_position_m=(0, 0),
	rudder_foil=boat.FoilProperties(
		area_m2=0.12,
		lift_coefficient=UNDERWATER_FOIL_LIFT_CURVE,
		drag_coefficient=curves.flat_plate_drag,
		base_drag_coefficient=0.02,
	),
	rudder_draw_length_m=0.5,
	rudder_mass_kg=4,
	tiller_length_m=1,
	tiller_mass_kg=2,
	rudder_position_m=(-1.8, 0),
	hull_reference_area_m2=0.3,
	hull_drag_coefficient=0.08,
	hull_lateral_drag_coefficient=0.02,
	sail_bearing_damping_n_m_s_rad=20,
	rudder_bearing_damping_n_m_s_rad=2,
	sail_sheet_lever_arm_m=1.5,
	max_sail_angle_rad=math.radians(90),
	max_rudder_angle_rad=math.radians(30),
)
ENVIRONMENT = boat.Environment(
	true_wind_velocity_m_s=pymunk.Vec2d(6, 0),
	true_water_velocity_m_s=pymunk.Vec2d(0, 0),
)
SIMULATION_SETTINGS = simulation.SimulationSettings(
	parameters=BOAT_PARAMETERS,
	initial_boat_angle_rad=INITIAL_BOAT_ANGLE_RAD,
	control_angle_resolution_deg=CONTROL_ANGLE_RESOLUTION_DEG,
	sailor_max_sheet_force_n=SAILOR_MAX_SHEET_FORCE_N,
	sailor_max_tiller_force_n=SAILOR_MAX_TILLER_FORCE_N,
	fixed_maximum_yaw_rate_rad_s=FIXED_MAXIMUM_YAW_RATE_RAD_S,
	sail_side_hysteresis_angle_rad=SAIL_SIDE_HYSTERESIS_ANGLE_RAD,
)
