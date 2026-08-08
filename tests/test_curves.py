import inspect
import math
import unittest

from simple_sailing_game import boat
from simple_sailing_game.physics import curves


class CoefficientCurveTest(unittest.TestCase):
	def test_force_functions_require_explicit_curves(self):
		for function in (boat.force_on_foil, boat.hinged_foil_load):
			with self.subTest(function=function.__name__):
				parameters = inspect.signature(function).parameters
				self.assertIs(
					parameters["lift_coefficient_at"].default,
					inspect.Parameter.empty,
				)
				self.assertIs(
					parameters["drag_coefficient_at"].default,
					inspect.Parameter.empty,
				)

	def test_streamlined_lift_uses_caller_selected_stall(self):
		lift = curves.streamlined_lift(
			stall_angle_rad=math.radians(10),
			stall_lift_coefficient=0.6,
		)

		self.assertAlmostEqual(lift(math.radians(10)), 0.6)
		self.assertGreater(lift(math.radians(10)), lift(math.radians(15)))
		self.assertAlmostEqual(lift(math.radians(90)), 0)

if __name__ == "__main__":
	unittest.main()
