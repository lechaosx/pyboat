import math
import unittest

from simple_sailing_game import course


class TimeTrialTest(unittest.TestCase):
	def test_only_the_next_buoy_advances_the_course(self):
		trial = course.TimeTrial(((0, 0), (10, 0)), buoy_radius_m=1)

		trial.advance((10, 0), 2)
		self.assertEqual(trial.next_buoy_index, 0)

		trial.advance((0, 0), 1)
		self.assertEqual(trial.next_buoy_index, 1)

	def test_crossing_all_buoys_completes_a_timed_lap(self):
		trial = course.TimeTrial(((0, 0), (10, 0)), buoy_radius_m=1)

		trial.advance((0, 0), 2)
		trial.advance((10, 0), 3)

		self.assertEqual(trial.completed_laps, 1)
		self.assertEqual(trial.best_time_seconds, 5)
		self.assertEqual(trial.elapsed_seconds, 0)
		self.assertEqual(trial.next_buoy_index, 0)

	def test_reset_keeps_the_best_time(self):
		trial = course.TimeTrial(((0, 0),), buoy_radius_m=1)
		trial.advance((0, 0), 4)
		trial.advance((100, 100), 2)

		trial.reset()

		self.assertEqual(trial.best_time_seconds, 4)
		self.assertEqual(trial.elapsed_seconds, 0)
		self.assertEqual(trial.next_buoy_index, 0)


class SailingFeedbackTest(unittest.TestCase):
	def test_point_of_sail_names_characteristic_wind_angles(self):
		self.assertEqual(course.point_of_sail(0), "in irons")
		self.assertEqual(course.point_of_sail(math.radians(45)), "close-hauled")
		self.assertEqual(course.point_of_sail(math.radians(90)), "beam reach")
		self.assertEqual(course.point_of_sail(math.radians(135)), "broad reach")
		self.assertEqual(course.point_of_sail(math.pi), "run")

	def test_trim_feedback_compares_actual_and_target_sail_angles(self):
		self.assertEqual(course.trim_feedback(math.radians(30), math.radians(31)), "good")
		self.assertEqual(course.trim_feedback(math.radians(20), math.radians(40)), "too tight")
		self.assertEqual(course.trim_feedback(math.radians(50), math.radians(30)), "too loose")


if __name__ == "__main__":
	unittest.main()
