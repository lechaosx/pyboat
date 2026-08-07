import math
from collections.abc import Callable
from dataclasses import dataclass, replace

import numpy as np
import pygame
import pymunk

from . import boat, course
from .controllers import arcade, assisted as controller, manual
from .physics import coupled as coupled_model
from .physics import dynamic as dynamic_model


WIDTH, HEIGHT = 1280, 720
PIXELS_PER_METER = 60
FLOW_PIXELS_PER_METER_SECOND = 20
FORCE_PIXELS_PER_NEWTON = 0.35
INTENT_VECTOR_LENGTH_PX = 100
CONTROL_LANDSCAPE_RECT = pygame.Rect(800, 280, 320, 320)
CONTROL_GRADIENT_PIXELS_PER_ERROR = 8
CONTROL_MOTION_PREVIEW_SECONDS = 0.25

DT_SECONDS = 1 / 60
MAXIMUM_FRAME_TIME_SECONDS = 0.25
INITIAL_BOAT_ANGLE_RAD = math.radians(135)
CONTROL_ANGLE_RESOLUTION_DEG = 1
ANALOG_STICK_DEADZONE = 0.15
SAILOR_MAX_SHEET_FORCE_N = 250
SAILOR_MAX_TILLER_FORCE_N = 60
MAXIMUM_YAW_RATE_RAD_S = math.radians(35)
GUIDED_GRADIENT_CONTROL = "gradient with helper"
GLOBAL_OPTIMUM_CONTROL = "global optimum"
RAW_GRADIENT_CONTROL = "gradient without helper"
MANUAL_CONTROL = "manual"
AUTO_SHEET_CONTROL = "auto sheet"
ARCADE_CONTROL = "arcade"
ARCADE_STERNWAY_CONTROL = "arcade sternway"
ARCADE_MOVE_CONTROL = "arcade move intent"
DYNAMIC_PHYSICS = "dynamic"
COUPLED_PHYSICS = "coupled"
COURSE_BUOYS_M = ((0, 0), (-15, 15), (-30, 0), (-15, -15))
BUOY_RADIUS_M = 2

BOAT_PARAMETERS = boat.BoatParameters(
	air_density_kg_m3=1.225,
	water_density_kg_m3=1025,
	hull_mass_kg=222,
	length_m=4,
	beam_m=1.5,
	bow_length_m=0.7,
	sail_area_m2=8,
	boom_length_m=2,
	boom_mass_kg=10,
	mast_position_m=(0.4, 0),
	sail_center_of_effort_from_mast_m=0.7,
	sail_mass_kg=12,
	keel_area_m2=0.6,
	rudder_area_m2=0.12,
	rudder_draw_length_m=0.5,
	rudder_mass_kg=4,
	tiller_length_m=1,
	tiller_mass_kg=2,
	rudder_position_m=(-1.8, 0),
	hull_reference_area_m2=0.3,
	hull_drag_coefficient=0.08,
	foil_base_drag_coefficient=0.02,
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
CONTROLLER_SETTINGS = controller.ControllerSettings(
	mode=controller.DIRECT_GRADIENT_CONTROL,
	use_global_slope=True,
	angle_resolution_deg=CONTROL_ANGLE_RESOLUTION_DEG,
)
ASSISTED_CONTROL_SETTINGS = {
	GUIDED_GRADIENT_CONTROL: CONTROLLER_SETTINGS,
	GLOBAL_OPTIMUM_CONTROL: replace(
		CONTROLLER_SETTINGS,
		mode=controller.OPTIMIZED_TARGET_CONTROL,
		use_global_slope=False,
	),
	RAW_GRADIENT_CONTROL: replace(
		CONTROLLER_SETTINGS,
		use_global_slope=False,
	),
}

@dataclass(frozen=True)
class SimulationTelemetry:
	physics: boat.PhysicsTelemetry
	decision: controller.ControlDecision | None
	controller_active: bool
	control_mode: str | None = None
	physics_mode: str = DYNAMIC_PHYSICS
	surface_targets: arcade.SurfaceTargets | None = None


ControllerComponent = Callable[
	[controller.ControlInput, controller.MotionIntent, float],
	controller.ControlOutput,
]
PlayerIntent = (
	controller.MotionIntent
	| manual.DirectionInput
	| arcade.AutoSheetIntent
	| arcade.HeadingIntent
)


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
	modes = (DYNAMIC_PHYSICS, COUPLED_PHYSICS)
	return modes[(modes.index(physics_mode) + 1) % len(modes)]


def keyboard_motion_intent(
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
) -> controller.MotionIntent:
	longitudinal = int(forward) - int(backward)
	turn = int(left) - int(right)
	return controller.MotionIntent(longitudinal, -turn)


def keyboard_heading_intent(
	boat_heading_rad: float,
	forward: bool,
	backward: bool,
	left: bool,
	right: bool,
	explicit_reverse: bool,
) -> arcade.HeadingIntent:
	world_direction = pymunk.Vec2d(
		int(forward) - int(backward),
		int(right) - int(left),
	).rotated(boat_heading_rad)
	return arcade.HeadingIntent(
		world_direction=tuple(world_direction),
		explicit_reverse=explicit_reverse,
	)


def keyboard_auto_sheet_intent(
	forward: bool,
	left: bool,
	right: bool,
) -> arcade.AutoSheetIntent:
	return arcade.AutoSheetIntent(
		power=int(forward),
		rudder=int(left) - int(right),
	)


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
	def direction(
		forward: bool,
		backward: bool,
		left: bool,
		right: bool,
	) -> tuple[float, float]:
		vector = pymunk.Vec2d(
			int(forward) - int(backward),
			int(right) - int(left),
		)
		return (0, 0) if not vector.length else tuple(vector.normalized())

	return manual.DirectionInput(
		sail_direction=direction(
			sail_forward,
			sail_backward,
			sail_left,
			sail_right,
		),
		rudder_direction=direction(
			rudder_forward,
			rudder_backward,
			rudder_left,
			rudder_right,
		),
	)


def analog_manual_input(
	boat_heading_rad: float,
	rudder_stick_x: float,
	rudder_stick_y: float,
	sail_stick_x: float,
	sail_stick_y: float,
) -> manual.DirectionInput:
	def direction(x: float, y: float) -> tuple[float, float]:
		vector = pymunk.Vec2d(x, y)
		if vector.length <= ANALOG_STICK_DEADZONE:
			return (0, 0)
		return tuple(vector.normalized().rotated(-boat_heading_rad))

	return manual.DirectionInput(
		sail_direction=direction(sail_stick_x, sail_stick_y),
		rudder_direction=direction(rudder_stick_x, rudder_stick_y),
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
	world_vector = pymunk.Vec2d(stick_x, stick_y)
	magnitude = min(1.0, world_vector.length)
	if magnitude <= ANALOG_STICK_DEADZONE:
		return controller.MotionIntent()
	if math.isclose(magnitude, 1):
		magnitude = 1
	local_direction = world_vector.normalized().rotated(-boat_heading_rad)
	square_scale = max(abs(local_direction.x), abs(local_direction.y))
	local_intent = local_direction / square_scale * magnitude
	zero_tolerance = math.ulp(2 * math.pi)
	forward = 0 if math.isclose(local_intent.x, 0, abs_tol=zero_tolerance) else local_intent.x
	turn = 0 if math.isclose(local_intent.y, 0, abs_tol=zero_tolerance) else local_intent.y
	return controller.MotionIntent(forward, turn)


def analog_heading_intent(
	stick_x: float,
	stick_y: float,
	explicit_reverse: bool,
) -> arcade.HeadingIntent:
	world_vector = pymunk.Vec2d(stick_x, stick_y)
	world_direction = (0, 0)
	if world_vector.length > ANALOG_STICK_DEADZONE:
		world_direction = (stick_x, stick_y)
	return arcade.HeadingIntent(world_direction, explicit_reverse)


def analog_auto_sheet_intent(
	power_axis: float,
	rudder_axis: float,
) -> arcade.AutoSheetIntent:
	return arcade.AutoSheetIntent(
		power=max(0, min(1, -power_axis)),
		rudder=max(-1, min(1, rudder_axis)),
	)


def intent_vector(
	intent: controller.MotionIntent,
	boat_heading_rad: float,
) -> pymunk.Vec2d:
	return pymunk.Vec2d(intent.forward, intent.turn).rotated(boat_heading_rad)


def heading_intent_vector(intent: arcade.HeadingIntent) -> pymunk.Vec2d:
	return pymunk.Vec2d(*intent.world_direction)


def luffing_sail_angle(
	boat_angle_rad: float,
	apparent_wind_m_s: pymunk.Vec2d,
) -> float:
	return boat.luffing_sail_angle(boat_angle_rad, apparent_wind_m_s)


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
	center_apparent_flow_m_s = (
		apparent_flow_m_s
		- center_world_m.perpendicular() * boat_angular_velocity_rad_s
	)
	force_n = boat.force_on_foil(
		center_apparent_flow_m_s,
		boat_angle_rad + angle_rad,
		fluid_density_kg_m3,
		area_m2,
		BOAT_PARAMETERS.foil_base_drag_coefficient,
	)
	boat_forward = pymunk.Vec2d(1, 0).rotated(boat_angle_rad)
	return controller.ControlEffect(
		angle_rad,
		center_world_m.cross(force_n),
		force_n.dot(boat_forward),
		force_n.length,
	)


def sample_control_effects(
	effect_at: Callable[[float], controller.ControlEffect],
	maximum_angle_rad: float,
) -> tuple[controller.ControlEffect, ...]:
	maximum_angle_deg = round(math.degrees(maximum_angle_rad))
	return tuple(
		effect_at(math.radians(angle_deg))
		for angle_deg in range(
			-maximum_angle_deg,
			maximum_angle_deg + 1,
			CONTROL_ANGLE_RESOLUTION_DEG,
		)
	)


def controller_input(observation: boat.BoatObservation) -> controller.ControlInput:
	def sail_effect_at(angle_rad: float) -> controller.ControlEffect:
		return foil_control_effect(
			observation.boat_angle_rad,
			observation.apparent_wind_m_s,
			BOAT_PARAMETERS.mast_position_m,
			BOAT_PARAMETERS.sail_center_of_effort_from_mast_m,
			angle_rad,
			BOAT_PARAMETERS.air_density_kg_m3,
			BOAT_PARAMETERS.sail_area_m2,
			observation.boat_angular_velocity_rad_s,
		)

	def rudder_effect_at(angle_rad: float) -> controller.ControlEffect:
		return foil_control_effect(
			observation.boat_angle_rad,
			observation.apparent_water_m_s,
			BOAT_PARAMETERS.rudder_position_m,
			BOAT_PARAMETERS.rudder_draw_length_m / 2,
			angle_rad,
			BOAT_PARAMETERS.water_density_kg_m3,
			BOAT_PARAMETERS.rudder_area_m2,
			observation.boat_angular_velocity_rad_s,
		)

	sail_candidates = sample_control_effects(
		sail_effect_at,
		BOAT_PARAMETERS.max_sail_angle_rad,
	)
	rudder_candidates = sample_control_effects(
		rudder_effect_at,
		BOAT_PARAMETERS.max_rudder_angle_rad,
	)
	return controller.ControlInput(
		observation.boat_angular_velocity_rad_s,
		boat.lumped_moment_of_inertia_kg_m2(BOAT_PARAMETERS),
		controller.ActuatorState(
			observation.sail_angle_rad,
			observation.sail_angular_velocity_rad_s,
			observation.sail_load.torque_n_m,
			boat.sail_moment_of_inertia_kg_m2(BOAT_PARAMETERS),
			BOAT_PARAMETERS.sail_bearing_damping_n_m_s_rad,
			SAILOR_MAX_SHEET_FORCE_N * BOAT_PARAMETERS.sail_sheet_lever_arm_m,
			BOAT_PARAMETERS.max_sail_angle_rad,
		),
		controller.ActuatorState(
			observation.rudder_angle_rad,
			observation.rudder_angular_velocity_rad_s,
			observation.rudder_load.torque_n_m,
			boat.rudder_moment_of_inertia_kg_m2(BOAT_PARAMETERS),
			BOAT_PARAMETERS.rudder_bearing_damping_n_m_s_rad,
			SAILOR_MAX_TILLER_FORCE_N * BOAT_PARAMETERS.tiller_length_m,
			BOAT_PARAMETERS.max_rudder_angle_rad,
		),
		sail_candidates,
		rudder_candidates,
		sail_effect_at,
		rudder_effect_at,
		luffing_sail_angle(
			observation.boat_angle_rad,
			observation.apparent_wind_m_s,
		),
	)


def surface_controller_input(
	observation: boat.BoatObservation,
	actuator_input: controller.ControlInput | None = None,
) -> arcade.ControlInput:
	if actuator_input is None:
		actuator_input = controller_input(observation)
	return arcade.ControlInput(
		observation.boat_angle_rad,
		observation.boat_angular_velocity_rad_s,
		boat.free_sail_angle(
			observation.boat_angle_rad,
			observation.apparent_wind_m_s,
			BOAT_PARAMETERS.max_sail_angle_rad,
		),
		BOAT_PARAMETERS.max_sail_angle_rad,
		BOAT_PARAMETERS.max_rudder_angle_rad,
		tuple(
			arcade.SurfaceEffect(
				effect.angle_rad,
				effect.drive_force_n,
				effect.yaw_moment_n_m,
			)
			for effect in actuator_input.sail_candidates
		),
		tuple(
			arcade.SurfaceEffect(
				effect.angle_rad,
				effect.drive_force_n,
				effect.yaw_moment_n_m,
			)
			for effect in actuator_input.rudder_candidates
		),
	)


def create_simulation() -> boat.SailingState:
	return boat.create_simulation(BOAT_PARAMETERS, INITIAL_BOAT_ANGLE_RAD)


def configured_assisted_controller(
	settings: controller.ControllerSettings,
) -> ControllerComponent:
	def run_controller(
		control_input: controller.ControlInput,
		intent: controller.MotionIntent,
		dt_seconds: float,
	) -> controller.ControlOutput:
		return controller.control(control_input, intent, dt_seconds, settings)

	return run_controller


def configured_manual_controller(
	manual_input: manual.DirectionInput,
) -> ControllerComponent:
	def run_controller(
		control_input: controller.ControlInput,
		_intent: controller.MotionIntent,
		_dt_seconds: float,
	) -> controller.ControlOutput:
		torques = manual.control(
			control_input.sail,
			control_input.rudder,
			manual_input,
		)
		return controller.ControlOutput(torques, None)

	return run_controller


def advance_game(
	state: boat.SailingState,
	intent: controller.MotionIntent,
	controller_component: ControllerComponent | None,
	environment: boat.Environment = ENVIRONMENT,
	dt_seconds: float = DT_SECONDS,
	physics_mode: str = DYNAMIC_PHYSICS,
) -> SimulationTelemetry:
	if physics_mode != COUPLED_PHYSICS:
		coupled_model.deactivate(state, BOAT_PARAMETERS)
	observation = boat.observe(state, environment, BOAT_PARAMETERS)
	control_output = None
	actuator_input = None
	if controller_component is not None:
		actuator_input = controller_input(observation)
		control_output = controller_component(
			actuator_input,
			intent,
			dt_seconds,
		)
	torques = boat.ActuatorTorques()
	if control_output is not None:
		torques = boat.ActuatorTorques(
			control_output.torques.sail_n_m,
			control_output.torques.rudder_n_m,
		)
	if (
		physics_mode == COUPLED_PHYSICS
		and actuator_input is not None
		and control_output is not None
		and control_output.decision is not None
		and control_output.decision.target_angles_rad is not None
	):
		target_sail_angle_rad, target_rudder_angle_rad = (
			control_output.decision.target_angles_rad
		)
		torques = boat.ActuatorTorques(
			controller.damped_actuator_control_torque(
				actuator_input.sail,
				target_sail_angle_rad,
			),
			controller.damped_actuator_control_torque(
				actuator_input.rudder,
				target_rudder_angle_rad,
			),
		)
	if physics_mode == COUPLED_PHYSICS:
		physics = coupled_model.step(
			state,
			environment,
			BOAT_PARAMETERS,
			torques,
			dt_seconds,
		)
	else:
		physics = dynamic_model.step(
			state,
			environment,
			BOAT_PARAMETERS,
			torques,
			dt_seconds,
		)
	decision = control_output.decision if control_output is not None else None
	return SimulationTelemetry(
		physics,
		decision,
		controller_component is not None,
		physics_mode=physics_mode,
	)


def advance_surface_game(
	state: boat.SailingState,
	intent: arcade.AutoSheetIntent | arcade.HeadingIntent | controller.MotionIntent,
	control_mode: str,
	physics_mode: str,
	environment: boat.Environment = ENVIRONMENT,
	dt_seconds: float = DT_SECONDS,
) -> SimulationTelemetry:
	if physics_mode != COUPLED_PHYSICS:
		coupled_model.deactivate(state, BOAT_PARAMETERS)
	observation = boat.observe(state, environment, BOAT_PARAMETERS)
	actuator_input = controller_input(observation)
	surface_input = surface_controller_input(observation, actuator_input)
	if control_mode == AUTO_SHEET_CONTROL:
		targets = arcade.auto_sheet_manual_tiller(surface_input, intent)
	elif control_mode == ARCADE_CONTROL:
		targets = arcade.heading_control(
			surface_input,
			intent,
			MAXIMUM_YAW_RATE_RAD_S,
		)
	elif control_mode == ARCADE_STERNWAY_CONTROL:
		targets = arcade.sternway_control(
			surface_input,
			intent,
			MAXIMUM_YAW_RATE_RAD_S,
		)
	else:
		targets = arcade.move_control(
			surface_input,
			intent,
			MAXIMUM_YAW_RATE_RAD_S,
		)

	if targets.sail_target_angle_rad is None:
		sail_target_angle_rad = max(
			-targets.sheet_limit_angle_rad,
			min(
				targets.sheet_limit_angle_rad,
				surface_input.free_sail_angle_rad,
			),
		)
	else:
		sail_target_angle_rad = targets.sail_target_angle_rad
	if physics_mode == COUPLED_PHYSICS:
		torques = boat.ActuatorTorques(
			controller.damped_actuator_control_torque(
				actuator_input.sail,
				sail_target_angle_rad,
			),
			controller.damped_actuator_control_torque(
				actuator_input.rudder,
				targets.rudder_target_angle_rad,
			),
		)
		physics = coupled_model.step(
			state,
			environment,
			BOAT_PARAMETERS,
			torques,
			dt_seconds,
		)
	else:
		torques = boat.ActuatorTorques(
			controller.actuator_control_torque(
				actuator_input.sail,
				sail_target_angle_rad,
				dt_seconds,
			),
			controller.actuator_control_torque(
				actuator_input.rudder,
				targets.rudder_target_angle_rad,
				dt_seconds,
			),
		)
		physics = dynamic_model.step(
			state,
			environment,
			BOAT_PARAMETERS,
			torques,
			dt_seconds,
		)
	return SimulationTelemetry(
		physics,
		None,
		True,
		control_mode,
		physics_mode,
		targets,
	)


def step(
	state: boat.SailingState,
	environment: boat.Environment,
	intent: controller.MotionIntent,
	dt_seconds: float = DT_SECONDS,
	control_mode: str = controller.OPTIMIZED_TARGET_CONTROL,
	use_global_slope: bool = True,
) -> SimulationTelemetry:
	settings = replace(
		CONTROLLER_SETTINGS,
		mode=control_mode,
		use_global_slope=use_global_slope,
	)
	return advance_game(
		state,
		intent,
		configured_assisted_controller(settings),
		environment,
		dt_seconds,
	)


def draw_vector(
	screen: pygame.Surface,
	start: pymunk.Vec2d,
	vector: pymunk.Vec2d,
	color: tuple[int, int, int],
	scale: float,
	width: int,
) -> None:
	end = start + vector * scale
	pygame.draw.line(screen, color, start, end, width)
	if not vector.length:
		return
	direction = vector.normalized()
	normal = direction.perpendicular()
	pygame.draw.polygon(
		screen,
		color,
		[
			end,
			end - direction * 10 + normal * 5,
			end - direction * 10 - normal * 5,
		],
	)


def draw_arrow(
	screen: pygame.Surface,
	font: pygame.font.Font,
	start: pymunk.Vec2d,
	vector: pymunk.Vec2d,
	color: tuple[int, int, int],
	label: str,
	scale: float,
	unit: str,
	label_x: int,
) -> None:
	pygame.draw.circle(screen, color, start, 4)
	draw_vector(screen, start, vector, color, scale, 3)
	text = font.render(f"{label}: {vector.length:.1f} {unit}", True, color)
	screen.blit(text, (label_x, start.y - 10))


def draw_intent(
	screen: pygame.Surface,
	font: pygame.font.Font,
	start: pymunk.Vec2d,
	boat_heading_rad: float,
	intent: PlayerIntent,
) -> None:
	if isinstance(intent, arcade.HeadingIntent):
		world_direction = heading_intent_vector(intent)
		if world_direction.length > 1:
			world_direction = world_direction.normalized()
		draw_vector(
			screen,
			start,
			world_direction,
			(240, 100, 220),
			INTENT_VECTOR_LENGTH_PX,
			3,
		)
		reverse = "  reverse modifier" if intent.explicit_reverse else ""
		text = font.render(
			f"heading input: {world_direction.x:+.0%}, {world_direction.y:+.0%}{reverse}",
			True,
			(240, 100, 220),
		)
		screen.blit(text, (30, start.y - 10))
		return

	if isinstance(intent, arcade.AutoSheetIntent):
		power = pymunk.Vec2d(intent.power, 0).rotated(boat_heading_rad)
		rudder = pymunk.Vec2d(0, intent.rudder).rotated(boat_heading_rad)
		draw_vector(screen, start, power, (100, 220, 140), INTENT_VECTOR_LENGTH_PX, 3)
		draw_vector(screen, start, rudder, (255, 160, 80), INTENT_VECTOR_LENGTH_PX, 3)
		text = font.render(
			f"auto-sheet: power {intent.power:+.0%}  rudder {intent.rudder:+.0%}",
			True,
			(240, 100, 220),
		)
		screen.blit(text, (30, start.y - 10))
		return

	if isinstance(intent, manual.DirectionInput):
		sail = pymunk.Vec2d(*intent.sail_direction).rotated(boat_heading_rad)
		rudder = pymunk.Vec2d(*intent.rudder_direction).rotated(boat_heading_rad)
		draw_vector(screen, start, sail, (235, 205, 100), INTENT_VECTOR_LENGTH_PX, 3)
		draw_vector(screen, start, rudder, (220, 220, 230), INTENT_VECTOR_LENGTH_PX, 3)
		text = font.render(
			"manual directions: yellow sail  white rudder",
			True,
			(240, 100, 220),
		)
		screen.blit(text, (30, start.y - 10))
		return

	forward_vector = pymunk.Vec2d(intent.forward, 0).rotated(boat_heading_rad)
	turn_vector = pymunk.Vec2d(0, intent.turn).rotated(boat_heading_rad)
	forward_end = start + forward_vector * INTENT_VECTOR_LENGTH_PX
	draw_vector(screen, start, forward_vector, (100, 220, 140), INTENT_VECTOR_LENGTH_PX, 2)
	draw_vector(screen, forward_end, turn_vector, (255, 160, 80), INTENT_VECTOR_LENGTH_PX, 2)
	draw_vector(
		screen,
		start,
		intent_vector(intent, boat_heading_rad),
		(240, 100, 220),
		INTENT_VECTOR_LENGTH_PX,
		3,
	)
	text = font.render(
		f"intent: forward {intent.forward:+.0%}  turn {intent.turn:+.0%}",
		True,
		(240, 100, 220),
	)
	screen.blit(text, (30, start.y - 10))


def control_landscape_point(
	rect: pygame.Rect,
	sail_angle_rad: float,
	rudder_angle_rad: float,
) -> pymunk.Vec2d:
	return pymunk.Vec2d(
		rect.left
		+ (sail_angle_rad + BOAT_PARAMETERS.max_sail_angle_rad)
		/ (2 * BOAT_PARAMETERS.max_sail_angle_rad)
		* rect.width,
		rect.top
		+ (BOAT_PARAMETERS.max_rudder_angle_rad - rudder_angle_rad)
		/ (2 * BOAT_PARAMETERS.max_rudder_angle_rad)
		* rect.height,
	)


def control_gradient_vector_px(
	rect: pygame.Rect,
	gradient: tuple[float, float] | pymunk.Vec2d,
) -> pymunk.Vec2d:
	return pymunk.Vec2d(
		-gradient[0]
		* BOAT_PARAMETERS.max_sail_angle_rad
		* CONTROL_GRADIENT_PIXELS_PER_ERROR,
		gradient[1]
		* BOAT_PARAMETERS.max_rudder_angle_rad
		* CONTROL_GRADIENT_PIXELS_PER_ERROR,
	)


def control_motion_vector_px(
	rect: pygame.Rect,
	sail_angular_velocity_rad_s: float,
	rudder_angular_velocity_rad_s: float,
) -> pymunk.Vec2d:
	return pymunk.Vec2d(
		sail_angular_velocity_rad_s
		* CONTROL_MOTION_PREVIEW_SECONDS
		* rect.width
		/ (2 * BOAT_PARAMETERS.max_sail_angle_rad),
		-rudder_angular_velocity_rad_s
		* CONTROL_MOTION_PREVIEW_SECONDS
		* rect.height
		/ (2 * BOAT_PARAMETERS.max_rudder_angle_rad),
	)


def draw_control_quality_landscape(
	screen: pygame.Surface,
	font: pygame.font.Font,
	state: boat.SailingState,
	landscape: controller.ControlLandscape,
) -> None:
	rect = CONTROL_LANDSCAPE_RECT
	screen.blit(
		font.render("control quality for current intent", True, (220, 220, 230)),
		(rect.left, rect.top - 22),
	)
	errors = landscape.errors
	quality = 1 - np.clip(np.sqrt(errors / 8), 0, 1)
	colors = np.empty(errors.shape + (3,), dtype=np.uint8)
	colors[:, :, 0] = 40 + 190 * (1 - quality)
	colors[:, :, 1] = 40 + 190 * quality
	colors[:, :, 2] = 55
	surface = pygame.surfarray.make_surface(colors[:, ::-1])
	screen.blit(pygame.transform.smoothscale(surface, rect.size), rect)
	pygame.draw.rect(screen, (220, 220, 230), rect, 1)

	current = control_landscape_point(rect, state.sail_angle_rad, state.rudder_angle_rad)
	optimum = control_landscape_point(
		rect,
		landscape.optimum_sail_angle_rad,
		landscape.optimum_rudder_angle_rad,
	)
	pygame.draw.line(screen, (255, 100, 220), optimum + (-5, -5), optimum + (5, 5), 2)
	pygame.draw.line(screen, (255, 100, 220), optimum + (-5, 5), optimum + (5, -5), 2)
	downhill = control_gradient_vector_px(rect, landscape.gradient)
	if downhill.length:
		draw_vector(screen, current, downhill, (100, 220, 255), 1, 2)
	motion = control_motion_vector_px(
		rect,
		state.sail_angular_velocity_rad_s,
		state.rudder_angular_velocity_rad_s,
	)
	if motion.length:
		draw_vector(screen, current, motion, (255, 210, 90), 1, 2)
	pygame.draw.circle(screen, (245, 245, 245), current, 5)
	pygame.draw.circle(screen, (25, 25, 30), current, 5, 1)
	screen.blit(
		font.render(
			"sail -90° left / +90° right    rudder +30° top / -30° bottom",
			True,
			(190, 190, 205),
		),
		(rect.left, rect.bottom + 5),
	)
	screen.blit(
		font.render("green good   red poor   magenta global optimum", True, (210, 210, 220)),
		(rect.left, rect.bottom + 28),
	)
	screen.blit(
		font.render(
			"cyan normalized downhill gradient   yellow motion over 0.25 s",
			True,
			(210, 210, 220),
		),
		(rect.left, rect.bottom + 49),
	)


def draw(
	screen: pygame.Surface,
	font: pygame.font.Font,
	state: boat.SailingState,
	environment: boat.Environment,
	intent: PlayerIntent,
	telemetry: SimulationTelemetry,
	time_trial: course.TimeTrial | None = None,
) -> None:
	center = pymunk.Vec2d(WIDTH / 2, HEIGHT / 2)
	body = state.boat
	observation = telemetry.physics.observation
	decision = telemetry.decision
	screen.fill((30, 30, 40))

	grid_spacing_px = PIXELS_PER_METER
	x_offset = -body.position.x * PIXELS_PER_METER % grid_spacing_px
	y_offset = -body.position.y * PIXELS_PER_METER % grid_spacing_px
	for x in range(round(x_offset), WIDTH, grid_spacing_px):
		pygame.draw.line(screen, (38, 46, 58), (x, 0), (x, HEIGHT))
	for y in range(round(y_offset), HEIGHT, grid_spacing_px):
		pygame.draw.line(screen, (38, 46, 58), (0, y), (WIDTH, y))
	if time_trial is not None:
		for index, buoy_m in enumerate(time_trial.buoys_m):
			position = (
				pymunk.Vec2d(*buoy_m) - body.position
			) * PIXELS_PER_METER + center
			color = (
				(255, 100, 220)
				if index == time_trial.next_buoy_index
				else (240, 180, 70)
			)
			pygame.draw.circle(
				screen,
				color,
				position,
				round(time_trial.buoy_radius_m * PIXELS_PER_METER),
				3,
			)
			screen.blit(font.render(str(index + 1), True, color), position + (5, 5))

	half_length_px = BOAT_PARAMETERS.length_m * PIXELS_PER_METER / 2
	half_beam_px = BOAT_PARAMETERS.beam_m * PIXELS_PER_METER / 2
	bow_base_px = (
		BOAT_PARAMETERS.length_m / 2 - BOAT_PARAMETERS.bow_length_m
	) * PIXELS_PER_METER
	hull_points = [
		pymunk.Vec2d(half_length_px, 0),
		pymunk.Vec2d(bow_base_px, -half_beam_px),
		pymunk.Vec2d(-half_length_px, -half_beam_px),
		pymunk.Vec2d(-half_length_px, half_beam_px),
		pymunk.Vec2d(bow_base_px, half_beam_px),
	]
	pygame.draw.polygon(
		screen,
		(205, 205, 220),
		[point.rotated(body.angle) + center for point in hull_points],
		3,
	)

	sail_direction = pymunk.Vec2d(-1, 0).rotated(body.angle + state.sail_angle_rad)
	pygame.draw.line(
		screen,
		(235, 205, 100),
		center,
		center + sail_direction * BOAT_PARAMETERS.boom_length_m * PIXELS_PER_METER,
		4,
	)
	rudder_position = (
		pymunk.Vec2d(*BOAT_PARAMETERS.rudder_position_m).rotated(body.angle)
		* PIXELS_PER_METER
		+ center
	)
	rudder_direction = pymunk.Vec2d(-1, 0).rotated(body.angle + state.rudder_angle_rad)
	pygame.draw.line(
		screen,
		(220, 220, 230),
		rudder_position,
		rudder_position
		+ rudder_direction * BOAT_PARAMETERS.rudder_draw_length_m * PIXELS_PER_METER,
		4,
	)

	flows = (
		("true wind", environment.true_wind_velocity_m_s, (80, 180, 255)),
		("apparent wind", observation.apparent_wind_m_s, (220, 220, 255)),
		("true water", environment.true_water_velocity_m_s, (80, 110, 230)),
		("apparent water", observation.apparent_water_m_s, (80, 220, 180)),
		("boat velocity", body.velocity, (130, 230, 130)),
	)
	for row, (label, vector, color) in enumerate(flows):
		draw_arrow(
			screen,
			font,
			pymunk.Vec2d(240, 45 + row * 55),
			vector,
			color,
			label,
			FLOW_PIXELS_PER_METER_SECOND,
			"m/s",
			30,
		)

	forces = (
		("sail force", observation.sail_load.force_n, (235, 205, 100)),
		("keel force", telemetry.physics.keel_force_n, (80, 220, 180)),
		("hull drag", telemetry.physics.hull_drag_n, (180, 150, 120)),
		("rudder force", observation.rudder_load.force_n, (220, 220, 230)),
	)
	for row, (label, vector, color) in enumerate(forces):
		draw_arrow(
			screen,
			font,
			pymunk.Vec2d(1040, 45 + row * 55),
			vector,
			color,
			label,
			FORCE_PIXELS_PER_NEWTON,
			"N",
			810,
		)

	draw_intent(screen, font, center, body.angle, intent)
	if decision is not None:
		draw_control_quality_landscape(screen, font, state, decision.landscape)

	if decision is None:
		if telemetry.control_mode is not None:
			control_status = f"{telemetry.control_mode} / {telemetry.physics_mode} physics"
		else:
			control_status = "manual control" if telemetry.controller_active else "uncontrolled"
	else:
		landscape = decision.landscape
		control_mode = telemetry.control_mode or "controlled"
		control_status = (
			f"{control_mode} / {telemetry.physics_mode} physics    "
			f"optimal sail {math.degrees(landscape.optimum_sail_angle_rad):+.0f}°    "
			f"optimal rudder {math.degrees(landscape.optimum_rudder_angle_rad):+.0f}°"
		)
	status = font.render(
		f"{control_status}    water speed {observation.apparent_water_m_s.length:.1f} m/s    "
		f"sail {math.degrees(state.sail_angle_rad):+.0f}°    "
		f"rudder {math.degrees(state.rudder_angle_rad):+.0f}°",
		True,
		(220, 220, 230),
	)
	screen.blit(status, (30, HEIGHT - 35))
	upwind_angle_rad = observation.apparent_wind_m_s.angle + math.pi
	point_of_sail = course.point_of_sail(body.angle - upwind_angle_rad)
	feedback = f"point of sail: {point_of_sail}"
	if telemetry.surface_targets is not None:
		free_sail_angle_rad = boat.free_sail_angle(
			body.angle,
			observation.apparent_wind_m_s,
			BOAT_PARAMETERS.max_sail_angle_rad,
		)
		if telemetry.surface_targets.sail_target_angle_rad is None:
			target_sail_angle_rad = max(
				-telemetry.surface_targets.sheet_limit_angle_rad,
				min(
					telemetry.surface_targets.sheet_limit_angle_rad,
					free_sail_angle_rad,
				),
			)
		else:
			target_sail_angle_rad = telemetry.surface_targets.sail_target_angle_rad
		feedback += f"    trim: {course.trim_feedback(state.sail_angle_rad, target_sail_angle_rad)}"
	if time_trial is not None:
		best = (
			"--"
			if time_trial.best_time_seconds is None
			else f"{time_trial.best_time_seconds:.1f} s"
		)
		feedback += (
			f"    mark {time_trial.next_buoy_index + 1}/{len(time_trial.buoys_m)}"
			f"    time {time_trial.elapsed_seconds:.1f} s    best {best}"
		)
	screen.blit(font.render(feedback, True, (220, 220, 230)), (30, HEIGHT - 60))


def main() -> None:
	pygame.init()
	screen = pygame.display.set_mode((WIDTH, HEIGHT))
	pygame.display.set_caption(
		"Sailboat — 1 guided gradient, 2 manual, 3 auto-sheet, 4 arcade, "
		"5 sternway arcade, 6 global optimum, 7 raw gradient, 8 move arcade; "
		"P physics, R reset; WASD / gamepad, Space reverse modifier"
	)
	font = pygame.font.Font(None, 24)
	clock = pygame.time.Clock()
	state = create_simulation()
	time_trial = course.TimeTrial(COURSE_BUOYS_M, BUOY_RADIUS_M)
	input_mode = GUIDED_GRADIENT_CONTROL
	physics_mode = DYNAMIC_PHYSICS
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
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_1:
				input_mode = GUIDED_GRADIENT_CONTROL
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_2:
				input_mode = MANUAL_CONTROL
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_3:
				input_mode = AUTO_SHEET_CONTROL
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_4:
				input_mode = ARCADE_CONTROL
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_5:
				input_mode = ARCADE_STERNWAY_CONTROL
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_6:
				input_mode = GLOBAL_OPTIMUM_CONTROL
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_7:
				input_mode = RAW_GRADIENT_CONTROL
			elif event.type == pygame.KEYDOWN and event.key == pygame.K_8:
				input_mode = ARCADE_MOVE_CONTROL
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
		display_intent: PlayerIntent = intent
		surface_intent: (
			arcade.AutoSheetIntent | arcade.HeadingIntent | controller.MotionIntent
		) = arcade.HeadingIntent()
		component = None
		if input_mode in ASSISTED_CONTROL_SETTINGS:
			keyboard_intent = keyboard_motion_intent(
				forward=keys[pygame.K_w],
				backward=keys[pygame.K_s],
				left=keys[pygame.K_a],
				right=keys[pygame.K_d],
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
				explicit_reverse_active = (
					input_mode == ARCADE_CONTROL and keys[pygame.K_SPACE]
				)
				surface_intent = keyboard_heading_intent(
					boat_heading_rad=state.boat.angle,
					forward=keys[pygame.K_w],
					backward=keys[pygame.K_s],
					left=keys[pygame.K_a],
					right=keys[pygame.K_d],
					explicit_reverse=explicit_reverse_active,
				)
			elif input_mode == ARCADE_MOVE_CONTROL:
				surface_intent = keyboard_motion_intent(
					forward=keys[pygame.K_w],
					backward=keys[pygame.K_s],
					left=keys[pygame.K_a],
					right=keys[pygame.K_d],
				)
			else:
				surface_intent = keyboard_auto_sheet_intent(
					forward=keys[pygame.K_w],
					left=keys[pygame.K_a],
					right=keys[pygame.K_d],
				)
			if joystick is not None:
				if input_mode in (ARCADE_CONTROL, ARCADE_STERNWAY_CONTROL):
					if input_mode == ARCADE_CONTROL and joystick.get_numbuttons():
						explicit_reverse_active = (
							explicit_reverse_active or joystick.get_button(0)
						)
					joystick_intent = analog_heading_intent(
						joystick.get_axis(0),
						joystick.get_axis(1),
						explicit_reverse_active,
					)
					if joystick_intent.world_direction != (0, 0):
						surface_intent = joystick_intent
					elif explicit_reverse_active:
						surface_intent = arcade.HeadingIntent(
							surface_intent.world_direction,
							True,
						)
				elif input_mode == ARCADE_MOVE_CONTROL:
					joystick_intent = analog_motion_intent(
						state.boat.angle,
						joystick.get_axis(0),
						joystick.get_axis(1),
					)
					if joystick_intent != controller.MotionIntent():
						surface_intent = joystick_intent
				else:
					joystick_intent = analog_auto_sheet_intent(
						joystick.get_axis(1),
						joystick.get_axis(0),
					)
					if joystick_intent != arcade.AutoSheetIntent():
						surface_intent = joystick_intent
			display_intent = surface_intent

		steps, accumulated_seconds = fixed_step_schedule(
			accumulated_seconds,
			elapsed_seconds,
		)
		for _ in range(steps):
			if input_mode in (
				AUTO_SHEET_CONTROL,
				ARCADE_CONTROL,
				ARCADE_STERNWAY_CONTROL,
				ARCADE_MOVE_CONTROL,
			):
				telemetry = advance_surface_game(
					state,
					surface_intent,
					input_mode,
					physics_mode,
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
