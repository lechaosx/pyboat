import math
from dataclasses import dataclass

import matplotlib.pyplot
import pymunk

from . import game
from .controllers import arcade


@dataclass(frozen=True)
class PolarPoint:
	heading_from_upwind_deg: float
	speed_through_water_m_s: float
	forward_speed_m_s: float
	leeway_angle_deg: float


def sample(
	physics_mode: str,
	headings_from_upwind_deg: tuple[float, ...] = tuple(range(30, 181, 15)),
	settling_seconds: float = 20,
	sampling_seconds: float = 5,
	dt_seconds: float = game.DT_SECONDS,
) -> tuple[PolarPoint, ...]:
	points = []
	upwind_angle_rad = game.ENVIRONMENT.true_wind_velocity_m_s.angle + math.pi
	for heading_from_upwind_deg in headings_from_upwind_deg:
		target_heading_rad = upwind_angle_rad + math.radians(heading_from_upwind_deg)
		target_direction = pymunk.Vec2d(1, 0).rotated(target_heading_rad)
		state = game.create_simulation()
		state.boat.angle = target_heading_rad
		for _ in range(round(settling_seconds / dt_seconds)):
			intent = arcade.HeadingIntent(
				world_direction=tuple(target_direction),
			)
			game.advance_surface_game(
				state,
				intent,
				game.ARCADE_CONTROL,
				physics_mode,
				dt_seconds=dt_seconds,
			)

		speeds_m_s = []
		forward_speeds_m_s = []
		leeway_angles_deg = []
		for _ in range(round(sampling_seconds / dt_seconds)):
			intent = arcade.HeadingIntent(
				world_direction=tuple(target_direction),
			)
			game.advance_surface_game(
				state,
				intent,
				game.ARCADE_CONTROL,
				physics_mode,
				dt_seconds=dt_seconds,
			)
			through_water_m_s = (
				state.boat.velocity - game.ENVIRONMENT.true_water_velocity_m_s
			)
			local_velocity_m_s = through_water_m_s.rotated(-state.boat.angle)
			speeds_m_s.append(through_water_m_s.length)
			forward_speeds_m_s.append(local_velocity_m_s.x)
			leeway_angles_deg.append(math.degrees(local_velocity_m_s.angle))
		points.append(
			PolarPoint(
				heading_from_upwind_deg,
				sum(speeds_m_s) / len(speeds_m_s),
				sum(forward_speeds_m_s) / len(forward_speeds_m_s),
				sum(leeway_angles_deg) / len(leeway_angles_deg),
			)
		)
	return tuple(points)


def main() -> None:
	for physics_mode in (
		game.DYNAMIC_PHYSICS,
		game.COUPLED_PHYSICS,
	):
		points = sample(physics_mode)
		matplotlib.pyplot.plot(
			[point.heading_from_upwind_deg for point in points],
			[point.speed_through_water_m_s for point in points],
			label=physics_mode,
		)
	matplotlib.pyplot.xlabel("Heading from upwind (degrees)")
	matplotlib.pyplot.ylabel("Speed through water (m/s)")
	matplotlib.pyplot.title("Simulated sailing polar")
	matplotlib.pyplot.grid(True)
	matplotlib.pyplot.legend()
	matplotlib.pyplot.savefig("polar.png")


if __name__ == "__main__":
	main()
