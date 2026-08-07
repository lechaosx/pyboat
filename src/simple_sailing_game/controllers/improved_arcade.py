from . import arcade
from .assisted import MotionIntent


TURN_POWER_REDUCTION = 0.35
STEERING_SPEED_M_S = 1.5
MINIMUM_YAW_RATE_SCALE = 0.3


def control(
	control_input: arcade.ControlInput,
	intent: MotionIntent,
	speed_through_water_m_s: float,
	maximum_yaw_rate_rad_s: float,
) -> arcade.SurfaceTargets:
	turn = max(-1, min(1, intent.turn))
	power = max(-1, min(1, intent.forward))
	power *= 1 - TURN_POWER_REDUCTION * abs(turn)
	yaw_rate_scale = max(
		MINIMUM_YAW_RATE_SCALE,
		min(1, speed_through_water_m_s / STEERING_SPEED_M_S),
	)
	return arcade.move_control(
		control_input,
		MotionIntent(power, turn * yaw_rate_scale),
		maximum_yaw_rate_rad_s,
	)
