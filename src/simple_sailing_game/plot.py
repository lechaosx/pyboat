import math

import matplotlib.pyplot

from . import boat


lift = boat.lift_coefficient


def drag(angle_of_attack_rad: float, base_drag_coefficient: float) -> float:
	return boat.drag_coefficient(
		angle_of_attack_rad,
		base_drag_coefficient,
	)


def main() -> None:
	from .game import BOAT_PARAMETERS

	angles_deg = list(range(-180, 181))
	angles_rad = [math.radians(angle) for angle in angles_deg]
	lifts = [lift(angle) for angle in angles_rad]
	drags = [
		drag(angle, BOAT_PARAMETERS.sail_base_drag_coefficient)
		for angle in angles_rad
	]

	matplotlib.pyplot.figure(figsize=(16, 9))
	matplotlib.pyplot.plot(angles_deg, lifts, label="Lift coefficient")
	matplotlib.pyplot.plot(angles_deg, drags, label="Drag coefficient")
	matplotlib.pyplot.xlabel("Angle of attack (degrees)")
	matplotlib.pyplot.ylabel("Coefficient")
	matplotlib.pyplot.title("Simplified foil coefficients")
	matplotlib.pyplot.legend()
	matplotlib.pyplot.grid(True)
	matplotlib.pyplot.savefig("output.png")


if __name__ == "__main__":
	main()
