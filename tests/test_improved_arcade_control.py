import math
import unittest

from simple_sailing_game.controllers import arcade, improved_arcade
from simple_sailing_game.controllers.assisted import MotionIntent


class ImprovedArcadeControlTest(unittest.TestCase):
	def control_input(self):
		return arcade.ControlInput(
			boat_heading_rad=0,
			boat_angular_velocity_rad_s=0,
			free_sail_angle_rad=-math.pi / 2,
			maximum_sail_angle_rad=math.pi / 2,
			maximum_rudder_angle_rad=math.pi / 6,
			sail_effects=(
				arcade.SurfaceEffect(-math.pi / 2, 0, 0),
				arcade.SurfaceEffect(-math.pi / 4, 10, 0),
				arcade.SurfaceEffect(0, 5, 0),
			),
			rudder_effects=(
				arcade.SurfaceEffect(-math.pi / 6, 0, 10),
				arcade.SurfaceEffect(0, 0, 0),
				arcade.SurfaceEffect(math.pi / 6, 0, -10),
			),
		)

	def test_turning_eases_power(self):
		straight = improved_arcade.control(
			self.control_input(), MotionIntent(1, 0), 2, maximum_yaw_rate_rad_s=1
		)
		turning = improved_arcade.control(
			self.control_input(), MotionIntent(1, 1), 2, maximum_yaw_rate_rad_s=1
		)

		self.assertGreater(
			abs(turning.sheet_limit_angle_rad),
			abs(straight.sheet_limit_angle_rad),
		)

	def test_original_move_controller_does_not_ease_power_while_turning(self):
		straight = arcade.move_control(
			self.control_input(), MotionIntent(1, 0), maximum_yaw_rate_rad_s=1
		)
		turning = arcade.move_control(
			self.control_input(), MotionIntent(1, 1), maximum_yaw_rate_rad_s=1
		)

		self.assertEqual(turning.sheet_limit_angle_rad, straight.sheet_limit_angle_rad)

	def test_low_water_speed_reduces_the_requested_yaw_rate(self):
		low_speed = improved_arcade.control(
			self.control_input(), MotionIntent(1, 0.8), 0, maximum_yaw_rate_rad_s=1
		)
		steering_speed = improved_arcade.control(
			self.control_input(), MotionIntent(1, 0.8), 2, maximum_yaw_rate_rad_s=1
		)

		self.assertEqual(low_speed.rudder_target_angle_rad, 0)
		self.assertNotEqual(
			steering_speed.rudder_target_angle_rad,
			low_speed.rudder_target_angle_rad,
		)


if __name__ == "__main__":
	unittest.main()
