from dataclasses import dataclass

from . import arcade
from .assisted import MotionIntent


@dataclass(frozen=True)
class Settings:
	turn_power_reduction: float
	steering_speed_m_s: float
	minimum_yaw_rate_scale: float


def control(
	control_input: arcade.ControlInput,
	intent: MotionIntent,
	speed_through_water_m_s: float,
	maximum_yaw_rate_rad_s: float,
	settings: Settings,
) -> arcade.SurfaceTargets:
	turn = max(-1, min(1, intent.turn))
	power = max(-1, min(1, intent.forward))
	power *= 1 - settings.turn_power_reduction * abs(turn)
	yaw_rate_scale = max(
		settings.minimum_yaw_rate_scale,
		min(1, speed_through_water_m_s / settings.steering_speed_m_s),
	)
	return arcade.move_control(
		control_input,
		MotionIntent(power, turn * yaw_rate_scale),
		maximum_yaw_rate_rad_s,
	)
