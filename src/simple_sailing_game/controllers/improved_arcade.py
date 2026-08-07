import math

from . import arcade
from .assisted import MotionIntent


TURN_POWER_REDUCTION = 0.35
STEERING_SPEED_M_S = 1.5
MINIMUM_YAW_RATE_SCALE = 0.3
GAMEPAD_SMOOTHING_SECONDS = 0.15


def control(
	control_input: arcade.ControlInput,
	intent: MotionIntent,
	speed_through_water_m_s: float,
	maximum_yaw_rate_rad_s: float,
) -> arcade.SurfaceTargets:
	"""Arcade movement with maneuver trim and speed-aware steering authority."""
	turn = max(-1, min(1, intent.turn))
	power = max(-1, min(1, intent.forward))
	power *= 1 - TURN_POWER_REDUCTION * abs(turn)
	yaw_rate_scale = max(
		MINIMUM_YAW_RATE_SCALE,
		min(1, speed_through_water_m_s / STEERING_SPEED_M_S),
	)
	return arcade.move_control(
		control_input,
		MotionIntent(power, turn),
		maximum_yaw_rate_rad_s * yaw_rate_scale,
	)


def smooth_gamepad_intent(
	previous: MotionIntent,
	target: MotionIntent,
	dt_seconds: float,
) -> MotionIntent:
	"""Low-pass analog input without delaying immediate keyboard commands."""
	if dt_seconds <= 0:
		return previous
	blend = 1 - math.exp(-dt_seconds / GAMEPAD_SMOOTHING_SECONDS)
	return MotionIntent(
		previous.forward + blend * (target.forward - previous.forward),
		previous.turn + blend * (target.turn - previous.turn),
	)
