import math
from dataclasses import replace

import pygame

from . import course, input as player_input, render, simulation
from .controllers import arcade, assisted as controller, manual


MAXIMUM_FRAME_TIME_SECONDS = 0.25
ANALOG_STICK_DEADZONE = 0.15
COURSE_BUOYS_M = ((0, 0), (-15, 15), (-30, 0), (-15, -15))
BUOY_RADIUS_M = 2

GUIDED_GRADIENT_CONTROL = simulation.GUIDED_GRADIENT_CONTROL
GLOBAL_OPTIMUM_CONTROL = simulation.GLOBAL_OPTIMUM_CONTROL
RAW_GRADIENT_CONTROL = simulation.RAW_GRADIENT_CONTROL
MANUAL_CONTROL = simulation.MANUAL_CONTROL
AUTO_SHEET_CONTROL = simulation.AUTO_SHEET_CONTROL
ARCADE_CONTROL = simulation.ARCADE_CONTROL
ARCADE_STERNWAY_CONTROL = simulation.ARCADE_STERNWAY_CONTROL
ARCADE_MOVE_CONTROL = simulation.ARCADE_MOVE_CONTROL
IMPROVED_ARCADE_CONTROL = simulation.IMPROVED_ARCADE_CONTROL
LUMPED_PHYSICS = simulation.LUMPED_PHYSICS
ARTICULATED_PHYSICS = simulation.ARTICULATED_PHYSICS
DEFAULT_PHYSICS_MODE = ARTICULATED_PHYSICS
DYNAMIC_PHYSICS = LUMPED_PHYSICS
COUPLED_PHYSICS = ARTICULATED_PHYSICS

DT_SECONDS = simulation.DT_SECONDS
INITIAL_BOAT_ANGLE_RAD = simulation.INITIAL_BOAT_ANGLE_RAD
CONTROL_ANGLE_RESOLUTION_DEG = simulation.CONTROL_ANGLE_RESOLUTION_DEG
MAXIMUM_YAW_RATE_RAD_S = simulation.MAXIMUM_YAW_RATE_RAD_S
BOAT_PARAMETERS = simulation.BOAT_PARAMETERS
ENVIRONMENT = simulation.ENVIRONMENT
CONTROLLER_SETTINGS = simulation.CONTROLLER_SETTINGS
ASSISTED_CONTROL_SETTINGS = simulation.ASSISTED_CONTROL_SETTINGS
SimulationTelemetry = simulation.SimulationTelemetry
ControllerComponent = simulation.ControllerComponent

WIDTH = render.WIDTH
HEIGHT = render.HEIGHT
CONTROL_LANDSCAPE_RECT = render.CONTROL_LANDSCAPE_RECT
CONTROL_GRADIENT_PIXELS_PER_ERROR = render.CONTROL_GRADIENT_PIXELS_PER_ERROR
CONTROL_MOTION_PREVIEW_SECONDS = render.CONTROL_MOTION_PREVIEW_SECONDS
draw = render.draw
draw_intent = render.draw_intent
intent_vector = render.intent_vector
heading_intent_vector = render.heading_intent_vector
control_gradient_vector_px = render.control_gradient_vector_px
control_motion_vector_px = render.control_motion_vector_px

create_simulation = simulation.create_simulation
configured_assisted_controller = simulation.configured_assisted_controller
configured_manual_controller = simulation.configured_manual_controller
advance_game = simulation.advance_game
advance_surface_game = simulation.advance_surface_game
controller_input = simulation.controller_input
surface_controller_input = simulation.surface_controller_input
foil_control_effect = simulation.foil_control_effect
sample_control_effects = simulation.sample_control_effects
step = simulation.step

CONTROL_MODES_BY_KEY = {
	pygame.K_1: MANUAL_CONTROL,
	pygame.K_2: AUTO_SHEET_CONTROL,
	pygame.K_3: IMPROVED_ARCADE_CONTROL,
	pygame.K_4: ARCADE_MOVE_CONTROL,
	pygame.K_5: ARCADE_CONTROL,
	pygame.K_6: ARCADE_STERNWAY_CONTROL,
	pygame.K_7: GUIDED_GRADIENT_CONTROL,
	pygame.K_8: GLOBAL_OPTIMUM_CONTROL,
	pygame.K_9: RAW_GRADIENT_CONTROL,
}
DEFAULT_CONTROL_MODE = IMPROVED_ARCADE_CONTROL


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


def next_physics_mode(physics_mode: str) -> str:
	modes = (LUMPED_PHYSICS, ARTICULATED_PHYSICS)
	return modes[(modes.index(physics_mode) + 1) % len(modes)]


def keyboard_motion_intent(
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
) -> controller.MotionIntent:
	return player_input.keyboard_motion(forward, backward, left, right)


def keyboard_heading_intent(
	boat_heading_rad: float,
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
	explicit_reverse: bool,
) -> arcade.HeadingIntent:
	return player_input.keyboard_heading(
		boat_heading_rad, forward, backward, left, right, explicit_reverse
	)


def keyboard_auto_sheet_intent(
	forward: bool,
	left: bool,
	right: bool,
) -> arcade.AutoSheetIntent:
	return player_input.keyboard_auto_sheet(forward, left, right)


def keyboard_manual_input(
	rudder_forward: bool,
	rudder_backward: bool,
	rudder_left: bool,
	rudder_right: bool,
	sail_forward: bool,
	sail_backward: bool,
	sail_left: bool,
	sail_right: bool,
) -> manual.DirectionInput:
	return player_input.keyboard_manual(
		rudder_forward,
		rudder_backward,
		rudder_left,
		rudder_right,
		sail_forward,
		sail_backward,
		sail_left,
		sail_right,
	)


def analog_manual_input(
	boat_heading_rad: float,
	rudder_stick_x: float,
	rudder_stick_y: float,
	sail_stick_x: float,
	sail_stick_y: float,
) -> manual.DirectionInput:
	return player_input.analog_manual(
		boat_heading_rad,
		rudder_stick_x,
		rudder_stick_y,
		sail_stick_x,
		sail_stick_y,
		ANALOG_STICK_DEADZONE,
	)


def toggled_control_mode(mode: str) -> str:
	if mode in ASSISTED_CONTROL_SETTINGS:
		return MANUAL_CONTROL
	return GUIDED_GRADIENT_CONTROL


def analog_motion_intent(
	boat_heading_rad: float,
	stick_x: float,
	stick_y: float,
) -> controller.MotionIntent:
	return player_input.analog_motion(
		boat_heading_rad, stick_x, stick_y, ANALOG_STICK_DEADZONE
	)


def analog_heading_intent(
	stick_x: float,
	stick_y: float,
	explicit_reverse: bool,
) -> arcade.HeadingIntent:
	return player_input.analog_heading(
		stick_x, stick_y, explicit_reverse, ANALOG_STICK_DEADZONE
	)


def analog_auto_sheet_intent(
	power_axis: float,
	rudder_axis: float,
) -> arcade.AutoSheetIntent:
	return player_input.analog_auto_sheet(power_axis, rudder_axis)


def main() -> None:
	pygame.init()
	screen = pygame.display.set_mode((WIDTH, HEIGHT))
	pygame.display.set_caption(
		"Sailboat — primary: 1 manual, 2 auto-sheet, 3 improved arcade; "
		"experiments: 4–9; P physics, R reset; WASD / gamepad"
	)
	font = pygame.font.Font(None, 24)
	clock = pygame.time.Clock()
	state = create_simulation()
	time_trial = course.TimeTrial(COURSE_BUOYS_M, BUOY_RADIUS_M)
	input_mode = DEFAULT_CONTROL_MODE
	physics_mode = DEFAULT_PHYSICS_MODE
	controller_enabled = True
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
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_p:
				physics_mode = next_physics_mode(physics_mode)
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_r:
				state = create_simulation()
				time_trial.reset()
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_c:
				controller_enabled = not controller_enabled
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_m:
				input_mode = toggled_control_mode(input_mode)
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
		intent = controller.MotionIntent()
		display_intent = intent
		surface_intent = arcade.HeadingIntent()
		component = None
		if input_mode in ASSISTED_CONTROL_SETTINGS:
			keyboard_intent = keyboard_motion_intent(
				keys[pygame.K_w],
				keys[pygame.K_s],
				keys[pygame.K_a],
				keys[pygame.K_d],
			)
			joystick_intent = controller.MotionIntent()
			if joystick is not None:
				joystick_intent = analog_motion_intent(
					state.boat.angle,
					joystick.get_axis(0),
					joystick.get_axis(1),
				)
			intent = (
				joystick_intent
				if joystick_intent != controller.MotionIntent()
				else keyboard_intent
			)
			display_intent = intent
			component = configured_assisted_controller(
				ASSISTED_CONTROL_SETTINGS[input_mode]
			)
		elif input_mode == MANUAL_CONTROL:
			keyboard_input = keyboard_manual_input(
				rudder_forward=keys[pygame.K_w],
				rudder_backward=keys[pygame.K_s],
				rudder_left=keys[pygame.K_a],
				rudder_right=keys[pygame.K_d],
				sail_forward=keys[pygame.K_UP],
				sail_backward=keys[pygame.K_DOWN],
				sail_left=keys[pygame.K_LEFT],
				sail_right=keys[pygame.K_RIGHT],
			)
			joystick_input = manual.DirectionInput()
			if joystick is not None:
				right_stick_x = (
					joystick.get_axis(2) if joystick.get_numaxes() > 2 else 0
				)
				right_stick_y = (
					joystick.get_axis(3) if joystick.get_numaxes() > 3 else 0
				)
				joystick_input = analog_manual_input(
					state.boat.angle,
					joystick.get_axis(0),
					joystick.get_axis(1),
					right_stick_x,
					right_stick_y,
				)
			manual_input = manual.DirectionInput(
				sail_direction=(
					joystick_input.sail_direction
					if joystick_input.sail_direction != (0, 0)
					else keyboard_input.sail_direction
				),
				rudder_direction=(
					joystick_input.rudder_direction
					if joystick_input.rudder_direction != (0, 0)
					else keyboard_input.rudder_direction
				),
			)
			display_intent = manual_input
			component = configured_manual_controller(manual_input)
		else:
			if input_mode in (ARCADE_CONTROL, ARCADE_STERNWAY_CONTROL):
				explicit_reverse = input_mode == ARCADE_CONTROL and keys[pygame.K_SPACE]
				surface_intent = keyboard_heading_intent(
					state.boat.angle,
					keys[pygame.K_w],
					keys[pygame.K_s],
					keys[pygame.K_a],
					keys[pygame.K_d],
					explicit_reverse,
				)
			elif input_mode in (ARCADE_MOVE_CONTROL, IMPROVED_ARCADE_CONTROL):
				surface_intent = keyboard_motion_intent(
					keys[pygame.K_w],
					keys[pygame.K_s],
					keys[pygame.K_a],
					keys[pygame.K_d],
				)
			else:
				surface_intent = keyboard_auto_sheet_intent(
					keys[pygame.K_w], keys[pygame.K_a], keys[pygame.K_d]
				)
			if joystick is not None:
				if input_mode in (ARCADE_CONTROL, ARCADE_STERNWAY_CONTROL):
					if input_mode == ARCADE_CONTROL and joystick.get_numbuttons():
						explicit_reverse = explicit_reverse or joystick.get_button(0)
					joystick_intent = analog_heading_intent(
						joystick.get_axis(0),
						joystick.get_axis(1),
						explicit_reverse,
					)
					if joystick_intent.world_direction != (0, 0):
						surface_intent = joystick_intent
					elif explicit_reverse:
						surface_intent = arcade.HeadingIntent(
							surface_intent.world_direction, True
						)
				elif input_mode in (ARCADE_MOVE_CONTROL, IMPROVED_ARCADE_CONTROL):
					joystick_intent = analog_motion_intent(
						state.boat.angle,
						joystick.get_axis(0),
						joystick.get_axis(1),
					)
					if joystick_intent != controller.MotionIntent():
						surface_intent = joystick_intent
				else:
					joystick_intent = analog_auto_sheet_intent(
						joystick.get_axis(1), joystick.get_axis(0)
					)
					if joystick_intent != arcade.AutoSheetIntent():
						surface_intent = joystick_intent
			display_intent = surface_intent

		steps, accumulated_seconds = fixed_step_schedule(
			accumulated_seconds, elapsed_seconds
		)
		for _ in range(steps):
			if input_mode in (
				AUTO_SHEET_CONTROL,
				ARCADE_CONTROL,
				ARCADE_STERNWAY_CONTROL,
				ARCADE_MOVE_CONTROL,
				IMPROVED_ARCADE_CONTROL,
			):
				telemetry = advance_surface_game(
					state, surface_intent, input_mode, physics_mode
				)
			else:
				active_component = component if controller_enabled else None
				telemetry = advance_game(
					state,
					intent,
					active_component,
					physics_mode=physics_mode,
				)
				if controller_enabled:
					telemetry = replace(telemetry, control_mode=input_mode)
			time_trial.advance(state.boat.position, DT_SECONDS)
		if telemetry is not None:
			draw(
				screen,
				font,
				state,
				ENVIRONMENT,
				display_intent,
				telemetry,
				time_trial,
			)
			pygame.display.flip()

	pygame.quit()


if __name__ == "__main__":
	main()
