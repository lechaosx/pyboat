import math
import unittest
from dataclasses import replace

from simple_sailing_game.controllers import arcade


MotionIntent = arcade.MotionIntent


class SurfaceControllerTest(unittest.TestCase):
	def test_controller_functions_have_policy_names(self):
		self.assertTrue(hasattr(arcade, "auto_sheet_control"))
		self.assertTrue(hasattr(arcade, "direct_move_control"))
		self.assertTrue(hasattr(arcade, "yaw_rate_move_control"))
		self.assertEqual(tuple(MotionIntent.__dataclass_fields__), ("forward", "turn"))

	def control_input(
		self,
		*,
		angular_velocity_rad_s=0,
		free_sail_angle_rad=-math.pi / 2,
	):
		return arcade.ControlInput(
			boat_angular_velocity_rad_s=angular_velocity_rad_s,
			boat_moment_of_inertia_kg_m2=1,
			current_sail_yaw_moment_n_m=0,
			free_sail_angle_rad=free_sail_angle_rad,
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

	def direct_control_input(self, *, angular_velocity_rad_s=0):
		return arcade.ControlInput(
			boat_angular_velocity_rad_s=angular_velocity_rad_s,
			boat_moment_of_inertia_kg_m2=1,
			current_sail_yaw_moment_n_m=0,
			free_sail_angle_rad=-math.pi / 2,
			maximum_rudder_angle_rad=math.pi / 6,
			sail_effects=(
				arcade.SurfaceEffect(-math.pi / 2, 0, 0),
				arcade.SurfaceEffect(-math.pi / 3, 5, 1),
				arcade.SurfaceEffect(-math.pi / 6, 10, 2),
				arcade.SurfaceEffect(0, 6, 0),
				arcade.SurfaceEffect(math.pi / 3, -5, -1),
				arcade.SurfaceEffect(math.pi / 2, -10, -2),
			),
			rudder_effects=(
				arcade.SurfaceEffect(-math.pi / 6, 0, 10),
				arcade.SurfaceEffect(-math.pi / 12, 0, 4),
				arcade.SurfaceEffect(0, 0, 0),
				arcade.SurfaceEffect(math.pi / 12, 0, -4),
				arcade.SurfaceEffect(math.pi / 6, 0, -10),
			),
		)

	def test_direct_arcade_selects_fractional_drive_force(self):
		neutral = arcade.direct_move_control(
			self.direct_control_input(), MotionIntent(forward=0, turn=0)
		)
		partial = arcade.direct_move_control(
			self.direct_control_input(), MotionIntent(forward=0.5, turn=0)
		)
		full = arcade.direct_move_control(
			self.direct_control_input(), MotionIntent(forward=1, turn=0)
		)

		self.assertAlmostEqual(neutral.sheet_limit_angle_rad, math.pi / 2)
		self.assertAlmostEqual(partial.sheet_limit_angle_rad, math.pi / 3)
		self.assertAlmostEqual(full.sheet_limit_angle_rad, math.pi / 6)

	def test_direct_arcade_selects_fractional_reverse_drive(self):
		partial = arcade.direct_move_control(
			self.direct_control_input(), MotionIntent(forward=-0.5, turn=0)
		)
		full = arcade.direct_move_control(
			self.direct_control_input(), MotionIntent(forward=-1, turn=0)
		)

		self.assertAlmostEqual(partial.sail_target_angle_rad, math.pi / 3)
		self.assertAlmostEqual(full.sail_target_angle_rad, math.pi / 2)

	def test_direct_arcade_maps_turn_to_achievable_moment(self):
		for turn, expected_rudder_angle_rad in (
			(1, -math.pi / 6),
			(0.5, -math.pi / 12),
			(0, 0),
			(-0.5, math.pi / 12),
			(-1, math.pi / 6),
		):
			with self.subTest(turn=turn):
				output = arcade.direct_move_control(
					self.direct_control_input(),
					MotionIntent(forward=0.5, turn=turn),
				)
				self.assertAlmostEqual(
					output.rudder_target_angle_rad,
					expected_rudder_angle_rad,
				)

	def test_direct_arcade_does_not_reshape_nonzero_turn_for_yaw_rate(self):
		stationary = arcade.direct_move_control(
			self.direct_control_input(angular_velocity_rad_s=0),
			MotionIntent(forward=0.5, turn=0.5),
		)
		rotating = arcade.direct_move_control(
			self.direct_control_input(angular_velocity_rad_s=10),
			MotionIntent(forward=0.5, turn=0.5),
		)

		self.assertEqual(rotating, stationary)

	def test_direct_arcade_neutral_turn_countersteers_yaw(self):
		stationary = arcade.direct_move_control(
			self.direct_control_input(angular_velocity_rad_s=0),
			MotionIntent(forward=0.5),
		)
		port_yaw = arcade.direct_move_control(
			self.direct_control_input(angular_velocity_rad_s=0.5),
			MotionIntent(forward=0.5),
		)
		starboard_yaw = arcade.direct_move_control(
			self.direct_control_input(angular_velocity_rad_s=-0.5),
			MotionIntent(forward=0.5),
		)

		self.assertGreater(
			port_yaw.rudder_target_angle_rad,
			stationary.rudder_target_angle_rad,
		)
		self.assertLess(
			starboard_yaw.rudder_target_angle_rad,
			stationary.rudder_target_angle_rad,
		)

	def test_direct_arcade_neutral_turn_compensates_current_sail_moment(self):
		control_input = replace(
			self.direct_control_input(),
			current_sail_yaw_moment_n_m=8,
		)

		output = arcade.direct_move_control(
			control_input,
			MotionIntent(forward=0.5),
		)

		self.assertAlmostEqual(output.rudder_target_angle_rad, math.pi / 6)

	def test_neutral_yaw_damping_uses_critical_damping_from_current_authority(self):
		moment_n_m = arcade._critical_yaw_damping_moment(
			angular_velocity_rad_s=0.25,
			moment_of_inertia_kg_m2=4,
			maximum_rudder_angle_rad=0.5,
			available_moments_n_m=(-10, 10),
		)

		expected_stiffness_n_m_rad = 20 / 1
		expected_damping_n_m_s_rad = 2 * math.sqrt(
			4 * expected_stiffness_n_m_rad
		)
		self.assertAlmostEqual(
			moment_n_m,
			-0.25 * expected_damping_n_m_s_rad,
		)

	def test_auto_sheet_uses_only_the_wind_selected_side(self):
		output = arcade.auto_sheet_control(
			self.control_input(),
			arcade.AutoSheetIntent(power=1, rudder=0),
		)

		self.assertAlmostEqual(output.sheet_limit_angle_rad, math.pi / 4)

	def test_zero_power_eases_the_sheet_to_the_free_sail_angle(self):
		output = arcade.auto_sheet_control(
			self.control_input(),
			arcade.AutoSheetIntent(power=0, rudder=0),
		)

		self.assertAlmostEqual(output.sheet_limit_angle_rad, math.pi / 2)

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

				self.assertAlmostEqual(output.sheet_limit_angle_rad, math.pi / 2)
				self.assertIsNone(output.sail_target_angle_rad)

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

		before_sail_target_rad = -before.sheet_limit_angle_rad
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
