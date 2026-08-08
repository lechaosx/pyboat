import math


def main() -> None:
	import matplotlib.pyplot

	from .game import BOAT_PARAMETERS

	angles_deg = list(range(-180, 181))
	angles_rad = [math.radians(angle) for angle in angles_deg]
	lifts = [BOAT_PARAMETERS.sail_lift_coefficient(angle) for angle in angles_rad]
	underwater_lifts = [
		BOAT_PARAMETERS.underwater_foil_lift_coefficient(angle)
		for angle in angles_rad
	]
	drags = [
		BOAT_PARAMETERS.sail_drag_coefficient(
			angle,
			BOAT_PARAMETERS.sail_base_drag_coefficient,
		)
		for angle in angles_rad
	]

	matplotlib.pyplot.figure(figsize=(16, 9))
	matplotlib.pyplot.plot(angles_deg, lifts, label="Lift coefficient")
	matplotlib.pyplot.plot(
		angles_deg,
		underwater_lifts,
		label="Underwater foil lift coefficient",
	)
	matplotlib.pyplot.plot(angles_deg, drags, label="Drag coefficient")
	matplotlib.pyplot.xlabel("Angle of attack (degrees)")
	matplotlib.pyplot.ylabel("Coefficient")
	matplotlib.pyplot.title("Simplified foil coefficients")
	matplotlib.pyplot.legend()
	matplotlib.pyplot.grid(True)
	matplotlib.pyplot.savefig("output.png")


if __name__ == "__main__":
	main()
