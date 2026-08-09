import math
from dataclasses import dataclass, field


@dataclass
class TimeTrial:
	buoys_m: tuple[tuple[float, float], ...]
	buoy_radius_m: float
	next_buoy_index: int = 0
	elapsed_seconds: float = 0
	best_time_seconds: float | None = None
	completed_laps: int = 0
	_inside_next_buoy: bool = field(default=False, init=False, repr=False)

	def advance(
		self,
		boat_position_m: tuple[float, float],
		dt_seconds: float,
	) -> None:
		self.elapsed_seconds += dt_seconds
		target = self.buoys_m[self.next_buoy_index]
		inside = math.dist(boat_position_m, target) <= self.buoy_radius_m
		if inside and not self._inside_next_buoy:
			self.next_buoy_index += 1
			if self.next_buoy_index == len(self.buoys_m):
				self.completed_laps += 1
				if (
					self.best_time_seconds is None
					or self.elapsed_seconds < self.best_time_seconds
				):
					self.best_time_seconds = self.elapsed_seconds
				self.elapsed_seconds = 0
				self.next_buoy_index = 0
			next_target = self.buoys_m[self.next_buoy_index]
			self._inside_next_buoy = (
				math.dist(boat_position_m, next_target) <= self.buoy_radius_m
			)
		else:
			self._inside_next_buoy = inside

	def reset(self) -> None:
		self.next_buoy_index = 0
		self.elapsed_seconds = 0
		self._inside_next_buoy = False


def point_of_sail(angle_from_upwind_rad: float) -> str:
	angle_rad = abs(math.remainder(angle_from_upwind_rad, 2 * math.pi))
	angle_rad = min(angle_rad, 2 * math.pi - angle_rad)
	if angle_rad < math.radians(25):
		return "in irons"
	if angle_rad < math.radians(65):
		return "close-hauled"
	if angle_rad < math.radians(120):
		return "beam reach"
	if angle_rad < math.radians(155):
		return "broad reach"
	return "run"


def trim_feedback(
	actual_sail_angle_rad: float,
	target_sail_angle_rad: float,
) -> str:
	error_rad = abs(actual_sail_angle_rad) - abs(target_sail_angle_rad)
	if abs(error_rad) <= math.radians(5):
		return "good"
	if error_rad < 0:
		return "too tight"
	return "too loose"
