from dataclasses import dataclass

@dataclass(frozen=True)
class Input:
	sheet: float = 0
	rudder: float = 0


def _clamp(value: float, minimum: float, maximum: float) -> float:
	return max(minimum, min(maximum, value))


def control(
	sheet_limit_angle_rad: float,
	manual_input: Input,
	maximum_sail_angle_rad: float,
	maximum_rudder_angle_rad: float,
	dt_seconds: float,
) -> tuple[float, float]:
	sheet_rate_rad_s = _clamp(manual_input.sheet, -1, 1) * maximum_sail_angle_rad
	sheet_limit_angle_rad = _clamp(
		sheet_limit_angle_rad - sheet_rate_rad_s * dt_seconds,
		0,
		maximum_sail_angle_rad,
	)
	rudder_target_angle_rad = (
		_clamp(manual_input.rudder, -1, 1) * maximum_rudder_angle_rad
	)
	return sheet_limit_angle_rad, rudder_target_angle_rad
