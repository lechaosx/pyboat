from dataclasses import dataclass

import pymunk

from sailing import boat, simulation
from sailing.controllers import arcade
from sailing.physics import coupled


@dataclass
class Player:
	state: boat.SailingState
	intent: arcade.MotionIntent
	acknowledged_input: int = 0


class Match:
	def __init__(
		self,
		settings: simulation.SimulationSettings,
		environment: boat.Environment,
		dt_seconds: float,
	) -> None:
		self.settings = settings
		self.environment = environment
		self.dt_seconds = dt_seconds
		self.space = pymunk.Space()
		self.players: dict[str, Player] = {}
		self.tick = 0
		self._next_player_number = 1

	def add_player(self) -> str:
		player_number = self._next_player_number
		self._next_player_number += 1
		player_id = f"player-{player_number}"
		state = coupled.create_simulation(
			self.settings.parameters,
			self.settings.initial_boat_angle_rad,
			self.space,
			(0, (player_number - 1) * 8),
		)
		self.players[player_id] = Player(state, arcade.MotionIntent())
		return player_id

	def remove_player(self, player_id: str) -> None:
		player = self.players.pop(player_id)
		coupled.remove_simulation(player.state)

	def set_input(
		self,
		player_id: str,
		sequence: int,
		intent: arcade.MotionIntent,
	) -> None:
		player = self.players[player_id]
		if sequence <= player.acknowledged_input:
			return
		player.intent = intent
		player.acknowledged_input = sequence

	def advance(self) -> None:
		for player in self.players.values():
			simulation.apply_surface_game_forces(
				player.state,
				player.intent,
				simulation.COMPUTED_YAW_RATE_ARCADE_CONTROL,
				self.settings,
				self.environment,
			)
		self.space.step(self.dt_seconds)
		for player in self.players.values():
			coupled.synchronize(player.state)
		self.tick += 1

	def snapshot(self) -> dict:
		return {
			"tick": self.tick,
			"boats": [
				{
					"id": player_id,
					"acknowledgedInput": player.acknowledged_input,
					"x": player.state.boat.position.x,
					"y": player.state.boat.position.y,
					"vx": player.state.boat.velocity.x,
					"vy": player.state.boat.velocity.y,
					"angle": player.state.boat.angle,
					"angularVelocity": player.state.boat.angular_velocity,
					"sail": player.state.sail_angle_rad,
					"sailAngularVelocity": player.state.sail_angular_velocity_rad_s,
					"rudder": player.state.rudder_angle_rad,
					"rudderAngularVelocity": player.state.rudder_angular_velocity_rad_s,
				}
				for player_id, player in self.players.items()
			],
		}
