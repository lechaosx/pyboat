import math

import matplotlib.pyplot

from sailing_game import drag_coefficient as drag
from sailing_game import lift_coefficient as lift


def main() -> None:
	angles_deg = list(range(-180, 181))
	angles_rad = [math.radians(angle) for angle in angles_deg]
	lifts = [lift(angle) for angle in angles_rad]
	drags = [drag(angle) for angle in angles_rad]

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
