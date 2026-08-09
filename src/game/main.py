import math

import pygame
from sailing import simulation
from sailing.controllers import arcade

from . import config, course, input as player_input, render


MAXIMUM_FRAME_TIME_SECONDS = 0.25
ANALOG_STICK_DEADZONE = 0.15

COMPUTED_YAW_RATE_ARCADE_CONTROL = simulation.COMPUTED_YAW_RATE_ARCADE_CONTROL
AUTO_SHEET_CONTROL = simulation.AUTO_SHEET_CONTROL
YAW_RATE_ARCADE_CONTROL = simulation.YAW_RATE_ARCADE_CONTROL

CONTROL_MODES_BY_KEY = {
	pygame.K_1: COMPUTED_YAW_RATE_ARCADE_CONTROL,
	pygame.K_2: AUTO_SHEET_CONTROL,
	pygame.K_3: YAW_RATE_ARCADE_CONTROL,
}
DEFAULT_CONTROL_MODE = COMPUTED_YAW_RATE_ARCADE_CONTROL


def fixed_step_schedule(
	accumulated_seconds: float,
	elapsed_seconds: float,
	dt_seconds: float = config.DT_SECONDS,
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
	state = simulation.create_simulation(config.SIMULATION_SETTINGS)
	time_trial = course.TimeTrial(config.COURSE_BUOYS_M, config.BUOY_RADIUS_M)
	input_mode = DEFAULT_CONTROL_MODE
	accumulated_seconds = 0.0
	telemetry = None
	joystick = None
	if pygame.joystick.get_count():
		joystick = pygame.joystick.Joystick(0)
		joystick.init()
	running = True

	while running:
		elapsed_seconds = clock.tick(round(1 / config.DT_SECONDS)) / 1000
		for event in pygame.event.get():
			if event.type == pygame.QUIT:
				running = False
			elif event.type == pygame.KEYDOWN and event.key in CONTROL_MODES_BY_KEY:
				input_mode = CONTROL_MODES_BY_KEY[event.key]
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_r:
				state = simulation.create_simulation(config.SIMULATION_SETTINGS)
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
				config.SIMULATION_SETTINGS,
				config.ENVIRONMENT,
				config.DT_SECONDS,
			)
			time_trial.advance(state.boat.position, config.DT_SECONDS)
		if telemetry is not None:
			render.draw(
				screen,
				font,
				state,
				config.ENVIRONMENT,
				intent,
				telemetry,
				config.BOAT_PARAMETERS,
				time_trial,
			)
			pygame.display.flip()

	pygame.quit()


if __name__ == "__main__":
	main()
