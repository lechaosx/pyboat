import math
import unittest

from simple_sailing_game.controllers import arcade
from simple_sailing_game.controllers.assisted import MotionIntent


class SurfaceControllerTest(unittest.TestCase):
	def control_input(
		self,
		*,
		angular_velocity_rad_s=0,
		free_sail_angle_rad=-math.pi / 2,
	):
		return arcade.ControlInput(
			boat_heading_rad=0,
			boat_angular_velocity_rad_s=angular_velocity_rad_s,
			free_sail_angle_rad=free_sail_angle_rad,
			maximum_sail_angle_rad=math.pi / 2,
			maximum_rudder_angle_rad=math.pi / 6,
			sail_effects=(
				arcade.SurfaceEffect(-math.pi / 2, 0, 0),
				arcade.SurfaceEffect(-math.pi / 4, 10, 2),
				arcade.SurfaceEffect(0, 5, 0),
				arcade.SurfaceEffect(math.pi / 4, 20, -2),
				arcade.SurfaceEffect(math.pi / 2, -8, -1),
			),
			rudder_effects=(
				arcade.SurfaceEffect(-math.pi / 6, -1, 10),
				arcade.SurfaceEffect(-math.pi / 12, -0.5, 2),
				arcade.SurfaceEffect(0, 0, 0),
				arcade.SurfaceEffect(math.pi / 12, -0.5, -2),
				arcade.SurfaceEffect(math.pi / 6, -1, -10),
			),
		)

	def test_auto_sheet_uses_only_the_wind_selected_side(self):
		output = arcade.auto_sheet_manual_tiller(
			self.control_input(),
			arcade.AutoSheetIntent(power=1, rudder=0),
		)

		self.assertAlmostEqual(output.sheet_limit_angle_rad, math.pi / 4)

	def test_zero_power_eases_the_sheet_to_the_free_sail_angle(self):
		output = arcade.auto_sheet_manual_tiller(
			self.control_input(),
			arcade.AutoSheetIntent(power=0, rudder=0),
		)

		self.assertAlmostEqual(output.sheet_limit_angle_rad, math.pi / 2)

	def test_sternway_arcade_points_the_stern_at_a_rear_heading(self):
		output = arcade.sternway_control(
			self.control_input(),
			arcade.HeadingIntent(world_direction=(-1, 0)),
			maximum_yaw_rate_rad_s=0.5,
		)

		self.assertAlmostEqual(output.sail_target_angle_rad, math.pi / 2)
		self.assertAlmostEqual(output.rudder_target_angle_rad, 0)

	def test_sternway_arcade_reverses_turn_when_targeting_with_the_stern(self):
		forward = arcade.sternway_control(
			self.control_input(),
			arcade.HeadingIntent(world_direction=(1, 1)),
			maximum_yaw_rate_rad_s=0.5,
		)
		backward = arcade.sternway_control(
			self.control_input(),
			arcade.HeadingIntent(world_direction=(-1, 1)),
			maximum_yaw_rate_rad_s=0.5,
		)

		self.assertLess(forward.rudder_target_angle_rad, 0)
		self.assertLess(backward.rudder_target_angle_rad, 0)
		self.assertIsNotNone(backward.sail_target_angle_rad)

	def test_move_arcade_preserves_turn_sense_while_backing(self):
		forward = arcade.move_control(
			self.control_input(),
			MotionIntent(forward=1, turn=1),
			maximum_yaw_rate_rad_s=0.5,
		)
		backward = arcade.move_control(
			self.control_input(),
			MotionIntent(forward=-1, turn=1),
			maximum_yaw_rate_rad_s=0.5,
		)

		self.assertLess(forward.rudder_target_angle_rad, 0)
		self.assertLess(backward.rudder_target_angle_rad, 0)
		self.assertIsNotNone(backward.sail_target_angle_rad)

	def test_move_arcade_has_no_drive_power_for_turn_only_input(self):
		for turn in (-1, 1):
			with self.subTest(turn=turn):
				output = arcade.move_control(
					self.control_input(),
					MotionIntent(turn=turn),
					maximum_yaw_rate_rad_s=0.5,
				)

				self.assertAlmostEqual(output.sheet_limit_angle_rad, math.pi / 2)
				self.assertIsNone(output.sail_target_angle_rad)

	def test_move_arcade_sail_target_is_continuous_across_neutral_forward_input(self):
		before = arcade.move_control(
			self.control_input(),
			MotionIntent(forward=0.01, turn=1),
			maximum_yaw_rate_rad_s=0.5,
		)
		after = arcade.move_control(
			self.control_input(),
			MotionIntent(forward=-0.01, turn=1),
			maximum_yaw_rate_rad_s=0.5,
		)

		before_sail_target_rad = -before.sheet_limit_angle_rad
		self.assertIsNotNone(after.sail_target_angle_rad)
		self.assertLess(
			abs(after.sail_target_angle_rad - before_sail_target_rad),
			math.radians(5),
		)

	def test_original_arcade_turns_the_bow_toward_a_rear_heading(self):
		output = arcade.heading_control(
			self.control_input(),
			arcade.HeadingIntent(world_direction=(-1, 0)),
			maximum_yaw_rate_rad_s=0.5,
		)

		self.assertIsNone(output.sail_target_angle_rad)
		self.assertAlmostEqual(output.rudder_target_angle_rad, -math.pi / 6)

	def test_original_heading_control_uses_reverse_as_an_input_modifier(self):
		neutral = arcade.heading_control(
			self.control_input(),
			arcade.HeadingIntent(explicit_reverse=True),
			maximum_yaw_rate_rad_s=0.5,
		)
		backing = arcade.heading_control(
			self.control_input(),
			arcade.HeadingIntent(
				world_direction=(0.5, 0),
				explicit_reverse=True,
			),
			maximum_yaw_rate_rad_s=0.5,
		)
		full_backing = arcade.heading_control(
			self.control_input(),
			arcade.HeadingIntent(
				world_direction=(1, 0),
				explicit_reverse=True,
			),
			maximum_yaw_rate_rad_s=0.5,
		)

		self.assertIsNone(neutral.sail_target_angle_rad)
		self.assertAlmostEqual(backing.sail_target_angle_rad, 0)
		self.assertAlmostEqual(full_backing.sail_target_angle_rad, math.pi / 2)

	def test_manual_tiller_maps_turn_directly_to_rudder_position(self):
		output = arcade.auto_sheet_manual_tiller(
			self.control_input(),
			arcade.AutoSheetIntent(power=1, rudder=0.5),
		)

		self.assertAlmostEqual(output.rudder_target_angle_rad, math.pi / 12)

	def test_arcade_heading_control_compensates_sail_yaw_at_target_rate(self):
		output = arcade.heading_control(
			self.control_input(angular_velocity_rad_s=0),
			arcade.HeadingIntent(world_direction=(1, 0)),
			maximum_yaw_rate_rad_s=0.5,
		)

		self.assertAlmostEqual(output.rudder_target_angle_rad, math.pi / 12)

	def test_arcade_heading_control_uses_achievable_moment_for_requested_rate(self):
		output = arcade.heading_control(
			self.control_input(angular_velocity_rad_s=0),
			arcade.HeadingIntent(world_direction=(0, 1)),
			maximum_yaw_rate_rad_s=0.5,
		)

		self.assertAlmostEqual(output.rudder_target_angle_rad, -math.pi / 6)


if __name__ == "__main__":
	unittest.main()
