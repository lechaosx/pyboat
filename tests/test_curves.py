import math
import unittest

from sailing.physics import curves


class CoefficientCurveTest(unittest.TestCase):
	def test_flat_plate_coefficients_have_expected_characteristic_values(self):
		self.assertAlmostEqual(curves.flat_plate_lift(0), 0)
		self.assertAlmostEqual(curves.flat_plate_lift(math.pi / 4), 1)
		self.assertAlmostEqual(curves.flat_plate_drag(0, 0.02), 0.02)
		self.assertAlmostEqual(curves.flat_plate_drag(math.pi / 2, 0.02), 2.02)

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
