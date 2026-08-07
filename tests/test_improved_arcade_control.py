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

	def test_turning_eases_power_without_changing_old_move_controller(self):
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

	def test_gamepad_smoothing_approaches_target_without_overshoot(self):
		result = improved_arcade.smooth_gamepad_intent(
			MotionIntent(), MotionIntent(1, -1), dt_seconds=1 / 60
		)

		self.assertGreater(result.forward, 0)
		self.assertLess(result.forward, 1)
		self.assertGreater(result.turn, -1)
		self.assertLess(result.turn, 0)


if __name__ == "__main__":
	unittest.main()
