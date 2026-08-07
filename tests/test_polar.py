import math
import unittest

from simple_sailing_game import game, polar


class PolarSamplingTest(unittest.TestCase):
	def test_sample_reports_finite_motion_for_each_requested_heading(self):
		points = polar.sample(
			game.COUPLED_PHYSICS,
			game.SIMULATION_SETTINGS,
			game.ENVIRONMENT,
			game.DT_SECONDS,
			headings_from_upwind_deg=(60, 120),
			settling_seconds=0,
			sampling_seconds=0.1,
		)

		self.assertEqual([point.heading_from_upwind_deg for point in points], [60, 120])
		for point in points:
			self.assertTrue(math.isfinite(point.speed_through_water_m_s))
			self.assertTrue(math.isfinite(point.forward_speed_m_s))
			self.assertTrue(math.isfinite(point.leeway_angle_deg))


if __name__ == "__main__":
	unittest.main()
