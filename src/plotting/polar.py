import math
from dataclasses import dataclass, replace

from sailing import boat, simulation
from sailing.controllers import arcade


@dataclass(frozen=True)
class PolarPoint:
	heading_from_upwind_deg: float
	actual_heading_from_upwind_deg: float
	heading_error_deg: float
	speed_through_water_m_s: float
	forward_speed_m_s: float
	leeway_angle_deg: float


def _move_intent_toward_heading(
	boat_heading_rad: float,
	target_heading_rad: float,
) -> arcade.MotionIntent:
	heading_error_rad = math.remainder(
		target_heading_rad - boat_heading_rad,
		2 * math.pi,
	)
	return arcade.MotionIntent(
		forward=1,
		turn=max(-1, min(1, heading_error_rad / (math.pi / 2))),
	)


def sample(
	settings: simulation.SimulationSettings,
	environment: boat.Environment,
	dt_seconds: float,
	headings_from_upwind_deg: tuple[float, ...] = tuple(range(30, 181, 15)),
	settling_seconds: float = 20,
	sampling_seconds: float = 5,
) -> tuple[PolarPoint, ...]:
	points = []
	upwind_angle_rad = environment.true_wind_velocity_m_s.angle + math.pi
	for heading_from_upwind_deg in headings_from_upwind_deg:
		target_heading_rad = upwind_angle_rad + math.radians(heading_from_upwind_deg)
		state = simulation.create_simulation(
			replace(settings, initial_boat_angle_rad=target_heading_rad)
		)
		for _ in range(round(settling_seconds / dt_seconds)):
			intent = _move_intent_toward_heading(
				state.boat.angle,
				target_heading_rad,
			)
			simulation.advance_surface_game(
				state,
				intent,
				simulation.YAW_RATE_ARCADE_CONTROL,
				settings,
				environment,
				dt_seconds,
			)

		speeds_m_s = []
		forward_speeds_m_s = []
		leeway_angles_deg = []
		heading_errors_deg = []
		for _ in range(round(sampling_seconds / dt_seconds)):
			intent = _move_intent_toward_heading(
				state.boat.angle,
				target_heading_rad,
			)
			simulation.advance_surface_game(
				state,
				intent,
				simulation.YAW_RATE_ARCADE_CONTROL,
				settings,
				environment,
				dt_seconds,
			)
			through_water_m_s = (
				state.boat.velocity - environment.true_water_velocity_m_s
			)
			local_velocity_m_s = through_water_m_s.rotated(-state.boat.angle)
			speeds_m_s.append(through_water_m_s.length)
			forward_speeds_m_s.append(local_velocity_m_s.x)
			leeway_angles_deg.append(math.degrees(local_velocity_m_s.angle))
			heading_errors_deg.append(
				math.degrees(
					math.remainder(
						state.boat.angle - target_heading_rad,
						2 * math.pi,
					)
				)
			)
		average_heading_error_deg = sum(heading_errors_deg) / len(
			heading_errors_deg
		)
		points.append(
			PolarPoint(
				heading_from_upwind_deg,
				heading_from_upwind_deg + average_heading_error_deg,
				average_heading_error_deg,
				sum(speeds_m_s) / len(speeds_m_s),
				sum(forward_speeds_m_s) / len(forward_speeds_m_s),
				sum(leeway_angles_deg) / len(leeway_angles_deg),
			)
		)
	return tuple(points)


def main() -> None:
	import matplotlib.pyplot

	from game import config

	points = sample(
		config.SIMULATION_SETTINGS,
		config.ENVIRONMENT,
		config.DT_SECONDS,
	)
	matplotlib.pyplot.plot(
		[point.heading_from_upwind_deg for point in points],
		[point.speed_through_water_m_s for point in points],
		label="articulated",
	)
	matplotlib.pyplot.xlabel("Heading from upwind (degrees)")
	matplotlib.pyplot.ylabel("Speed through water (m/s)")
	matplotlib.pyplot.title("Simulated sailing polar")
	matplotlib.pyplot.grid(True)
	matplotlib.pyplot.legend()
	matplotlib.pyplot.savefig("polar.png")


if __name__ == "__main__":
	main()
