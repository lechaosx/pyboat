import math

import pygame
import pymunk

from . import boat, course, input as player_input, render, simulation
from .controllers import arcade
from .physics import curves


MAXIMUM_FRAME_TIME_SECONDS = 0.25
ANALOG_STICK_DEADZONE = 0.15
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

COMPUTED_YAW_RATE_ARCADE_CONTROL = simulation.COMPUTED_YAW_RATE_ARCADE_CONTROL
AUTO_SHEET_CONTROL = simulation.AUTO_SHEET_CONTROL
YAW_RATE_ARCADE_CONTROL = simulation.YAW_RATE_ARCADE_CONTROL

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

CONTROL_MODES_BY_KEY = {
	pygame.K_1: COMPUTED_YAW_RATE_ARCADE_CONTROL,
	pygame.K_2: AUTO_SHEET_CONTROL,
	pygame.K_3: YAW_RATE_ARCADE_CONTROL,
}
DEFAULT_CONTROL_MODE = COMPUTED_YAW_RATE_ARCADE_CONTROL


def fixed_step_schedule(
	accumulated_seconds: float,
	elapsed_seconds: float,
	dt_seconds: float = DT_SECONDS,
) -> tuple[int, float]:
	total_seconds = accumulated_seconds + min(
		elapsed_seconds,
		MAXIMUM_FRAME_TIME_SECONDS,
	)
	steps = int(total_seconds / dt_seconds)
	remaining_seconds = total_seconds - steps * dt_seconds
	if math.isclose(remaining_seconds, 0, abs_tol=math.ulp(dt_seconds)):
		remaining_seconds = 0
	return steps, remaining_seconds


def main() -> None:
	pygame.init()
	screen = pygame.display.set_mode((render.WIDTH, render.HEIGHT))
	pygame.display.set_caption(
		"Sailboat — 1 computed yaw-rate arcade, 2 auto-sheet + rudder, "
		"3 yaw-rate arcade; R reset; WASD / gamepad"
	)
	font = pygame.font.Font(None, 24)
	clock = pygame.time.Clock()
	state = simulation.create_simulation(SIMULATION_SETTINGS)
	time_trial = course.TimeTrial(COURSE_BUOYS_M, BUOY_RADIUS_M)
	input_mode = DEFAULT_CONTROL_MODE
	accumulated_seconds = 0.0
	telemetry = None
	joystick = None
	if pygame.joystick.get_count():
		joystick = pygame.joystick.Joystick(0)
		joystick.init()
	running = True

	while running:
		elapsed_seconds = clock.tick(round(1 / DT_SECONDS)) / 1000
		for event in pygame.event.get():
			if event.type == pygame.QUIT:
				running = False
			elif event.type == pygame.KEYDOWN and event.key in CONTROL_MODES_BY_KEY:
				input_mode = CONTROL_MODES_BY_KEY[event.key]
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_r:
				state = simulation.create_simulation(SIMULATION_SETTINGS)
				time_trial.reset()
			elif event.type == pygame.JOYDEVICEADDED and joystick is None:
				joystick = pygame.joystick.Joystick(event.device_index)
				joystick.init()
			elif (
				event.type == pygame.JOYDEVICEREMOVED
				and joystick is not None
				and event.instance_id == joystick.get_instance_id()
			):
				joystick = None

		keys = pygame.key.get_pressed()
		if input_mode in (
			COMPUTED_YAW_RATE_ARCADE_CONTROL,
			YAW_RATE_ARCADE_CONTROL,
		):
			intent = player_input.keyboard_motion(
				keys[pygame.K_w],
				keys[pygame.K_s],
				keys[pygame.K_a],
				keys[pygame.K_d],
			)
			if joystick is not None:
				joystick_intent = player_input.analog_motion(
					state.boat.angle,
					joystick.get_axis(0),
					joystick.get_axis(1),
					ANALOG_STICK_DEADZONE,
				)
				if joystick_intent != arcade.MotionIntent():
					intent = joystick_intent
		else:
			intent = player_input.keyboard_auto_sheet(
				keys[pygame.K_w],
				keys[pygame.K_a],
				keys[pygame.K_d],
			)
			if joystick is not None:
				joystick_intent = player_input.analog_auto_sheet(
					joystick.get_axis(1),
					joystick.get_axis(0),
				)
				if joystick_intent != arcade.AutoSheetIntent():
					intent = joystick_intent

		steps, accumulated_seconds = fixed_step_schedule(
			accumulated_seconds,
			elapsed_seconds,
		)
		for _ in range(steps):
			telemetry = simulation.advance_surface_game(
				state,
				intent,
				input_mode,
				SIMULATION_SETTINGS,
				ENVIRONMENT,
				DT_SECONDS,
			)
			time_trial.advance(state.boat.position, DT_SECONDS)
		if telemetry is not None:
			render.draw(
				screen,
				font,
				state,
				ENVIRONMENT,
				intent,
				telemetry,
				BOAT_PARAMETERS,
				time_trial,
			)
			pygame.display.flip()

	pygame.quit()


if __name__ == "__main__":
	main()
