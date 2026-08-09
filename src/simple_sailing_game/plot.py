import math


def main() -> None:
	import matplotlib.pyplot

	from .game import BOAT_PARAMETERS

	angles_deg = list(range(-180, 181))
	angles_rad = [math.radians(angle) for angle in angles_deg]
	matplotlib.pyplot.figure(figsize=(16, 9))
	for name, foil in (
		("Sail", BOAT_PARAMETERS.sail_foil),
		("Keel", BOAT_PARAMETERS.keel_foil),
		("Rudder", BOAT_PARAMETERS.rudder_foil),
	):
		lifts = [foil.lift_coefficient(angle) for angle in angles_rad]
		drags = [
			foil.drag_coefficient(angle, foil.base_drag_coefficient)
			for angle in angles_rad
		]
		matplotlib.pyplot.plot(angles_deg, lifts, label=f"{name} lift")
		matplotlib.pyplot.plot(angles_deg, drags, label=f"{name} drag")
	matplotlib.pyplot.xlabel("Angle of attack (degrees)")
	matplotlib.pyplot.ylabel("Coefficient")
	matplotlib.pyplot.title("Simplified foil coefficients")
	matplotlib.pyplot.legend()
	matplotlib.pyplot.grid(True)
	matplotlib.pyplot.savefig("output.png")


if __name__ == "__main__":
	main()
