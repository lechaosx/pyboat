import math
import unittest

from simple_sailing_game.controllers import arcade


MotionIntent = arcade.MotionIntent


class SurfaceControllerTest(unittest.TestCase):
	def control_input(
		self,
		*,
		angular_velocity_rad_s=0,
		free_sail_angle_rad=-math.pi / 2,
		sail_angle_rad=-math.pi / 4,
		apparent_wind_angle_from_boat_forward_rad=0,
		sail_side_hysteresis_angle_rad=math.radians(10),
	):
		return arcade.ControlInput(
			boat_angular_velocity_rad_s=angular_velocity_rad_s,
			free_sail_angle_rad=free_sail_angle_rad,
			sail_angle_rad=sail_angle_rad,
			apparent_wind_angle_from_boat_forward_rad=(
				apparent_wind_angle_from_boat_forward_rad
			),
			sail_side_hysteresis_angle_rad=sail_side_hysteresis_angle_rad,
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
		output = arcade.auto_sheet_control(
			self.control_input(),
			arcade.AutoSheetIntent(power=1, rudder=0),
		)

		self.assertAlmostEqual(output.sheet_target_angle_rad, -math.pi / 4)

	def test_zero_power_eases_the_sheet_to_the_free_sail_angle(self):
		output = arcade.auto_sheet_control(
			self.control_input(),
			arcade.AutoSheetIntent(power=0, rudder=0),
		)

		self.assertAlmostEqual(output.sheet_target_angle_rad, -math.pi / 2)

	def test_auto_sheet_retains_sail_side_within_downwind_hysteresis(self):
		output = arcade.auto_sheet_control(
			self.control_input(
				sail_angle_rad=math.pi / 4,
				apparent_wind_angle_from_boat_forward_rad=math.radians(5),
			),
			arcade.AutoSheetIntent(power=1, rudder=0),
		)

		self.assertAlmostEqual(output.sheet_target_angle_rad, math.pi / 4)

	def test_auto_sheet_switches_sail_side_beyond_downwind_hysteresis(self):
		output = arcade.auto_sheet_control(
			self.control_input(
				sail_angle_rad=math.pi / 4,
				apparent_wind_angle_from_boat_forward_rad=math.radians(11),
			),
			arcade.AutoSheetIntent(power=1, rudder=0),
		)

		self.assertAlmostEqual(output.sheet_target_angle_rad, -math.pi / 4)

	def test_auto_sheet_does_not_retain_sail_side_upwind(self):
		output = arcade.auto_sheet_control(
			self.control_input(
				free_sail_angle_rad=-math.pi / 4,
				sail_angle_rad=math.pi / 4,
				apparent_wind_angle_from_boat_forward_rad=3 * math.pi / 4,
			),
			arcade.AutoSheetIntent(power=1, rudder=0),
		)

		self.assertAlmostEqual(output.sheet_target_angle_rad, -math.pi / 4)

	def test_yaw_rate_arcade_preserves_turn_sense_while_backing(self):
		forward = arcade.yaw_rate_move_control(
			self.control_input(),
			MotionIntent(forward=1, turn=1),
			maximum_yaw_rate_rad_s=0.5,
		)
		backward = arcade.yaw_rate_move_control(
			self.control_input(),
			MotionIntent(forward=-1, turn=1),
			maximum_yaw_rate_rad_s=0.5,
		)

		self.assertLess(forward.rudder_target_angle_rad, 0)
		self.assertLess(backward.rudder_target_angle_rad, 0)
		self.assertIsNotNone(backward.sail_target_angle_rad)

	def test_yaw_rate_arcade_has_no_drive_power_for_turn_only_input(self):
		for turn in (-1, 1):
			with self.subTest(turn=turn):
				output = arcade.yaw_rate_move_control(
					self.control_input(),
					MotionIntent(turn=turn),
					maximum_yaw_rate_rad_s=0.5,
				)

				self.assertAlmostEqual(
					abs(output.sheet_target_angle_rad),
					math.pi / 2,
				)
				self.assertIsNone(output.sail_target_angle_rad)

	def test_yaw_rate_arcade_uses_analog_turn_values_continuously(self):
		for turn, expected_rudder_angle_rad in (
			(0.25, math.pi / 12),
			(0.5, 0),
			(0.75, -math.pi / 12),
		):
			with self.subTest(turn=turn):
				output = arcade.yaw_rate_move_control(
					self.control_input(angular_velocity_rad_s=0.25),
					MotionIntent(forward=0.5, turn=turn),
					maximum_yaw_rate_rad_s=0.5,
				)
				self.assertAlmostEqual(
					output.rudder_target_angle_rad,
					expected_rudder_angle_rad,
				)

	def test_yaw_rate_arcade_sail_target_is_continuous_across_neutral_forward_input(self):
		before = arcade.yaw_rate_move_control(
			self.control_input(),
			MotionIntent(forward=0.01, turn=1),
			maximum_yaw_rate_rad_s=0.5,
		)
		after = arcade.yaw_rate_move_control(
			self.control_input(),
			MotionIntent(forward=-0.01, turn=1),
			maximum_yaw_rate_rad_s=0.5,
		)

		before_sail_target_rad = before.sheet_target_angle_rad
		self.assertIsNotNone(after.sail_target_angle_rad)
		self.assertLess(
			abs(after.sail_target_angle_rad - before_sail_target_rad),
			math.radians(5),
		)

	def test_auto_sheet_maps_rudder_directly_to_position(self):
		output = arcade.auto_sheet_control(
			self.control_input(),
			arcade.AutoSheetIntent(power=1, rudder=0.5),
		)

		self.assertAlmostEqual(output.rudder_target_angle_rad, math.pi / 12)


if __name__ == "__main__":
	unittest.main()
