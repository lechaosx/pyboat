import math
import unittest
from dataclasses import replace

from simple_sailing_game.controllers import arcade


MotionIntent = arcade.MotionIntent


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
			boat_moment_of_inertia_kg_m2=1,
			current_sail_yaw_moment_n_m=0,
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

	def direct_control_input(self, *, angular_velocity_rad_s=0):
		return arcade.ControlInput(
			boat_heading_rad=0,
			boat_angular_velocity_rad_s=angular_velocity_rad_s,
			boat_moment_of_inertia_kg_m2=1,
			current_sail_yaw_moment_n_m=0,
			free_sail_angle_rad=-math.pi / 2,
			maximum_sail_angle_rad=math.pi / 2,
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

	def test_heading_hold_captures_release_heading_and_clears_on_turn(self):
		heading_hold = arcade.HeadingHold()

		turning = heading_hold.apply(
			boat_heading_rad=1,
			intent=MotionIntent(forward=1, turn=1),
		)
		released = heading_hold.apply(
			boat_heading_rad=2,
			intent=MotionIntent(forward=1),
		)
		held = heading_hold.apply(
			boat_heading_rad=2.5,
			intent=MotionIntent(forward=1),
		)
		turned_again = heading_hold.apply(
			boat_heading_rad=3,
			intent=MotionIntent(forward=1, turn=-1),
		)

		self.assertIsNone(turning.held_heading_rad)
		self.assertEqual(released.held_heading_rad, 2)
		self.assertEqual(held.held_heading_rad, 2)
		self.assertIsNone(turned_again.held_heading_rad)

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

	def test_direct_arcade_neutral_turn_restores_held_heading(self):
		control_input = self.direct_control_input(angular_velocity_rad_s=0)
		control_input = replace(control_input, boat_heading_rad=0.5)
		neutral = arcade.direct_move_control(
			control_input,
			MotionIntent(forward=0.5),
		)
		held = arcade.direct_move_control(
			control_input,
			MotionIntent(forward=0.5, held_heading_rad=0),
		)

		self.assertGreater(
			held.rudder_target_angle_rad,
			neutral.rudder_target_angle_rad,
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
		moment_n_m = arcade._critical_yaw_correction_moment(
			heading_error_rad=0.1,
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
			-0.1 * expected_stiffness_n_m_rad
			- 0.25 * expected_damping_n_m_s_rad,
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
