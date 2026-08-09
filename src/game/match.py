from collections import deque
from dataclasses import dataclass, field

import pymunk

from sailing import boat, simulation
from sailing.controllers import arcade
from sailing.physics import coupled

from . import config


@dataclass
class Player:
	state: boat.SailingState
	intent: arcade.MotionIntent
	acknowledged_input: int = 0
	last_received_input: int = 0
	pending_inputs: deque[tuple[int, arcade.MotionIntent]] = field(default_factory=deque)


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
		self.player_ids_by_shape: dict[pymunk.Shape, str] = {}
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
		for shape in state.boat.shapes:
			self.player_ids_by_shape[shape] = player_id
		return player_id

	def remove_player(self, player_id: str) -> None:
		player = self.players.pop(player_id)
		for shape in player.state.boat.shapes:
			del self.player_ids_by_shape[shape]
		coupled.remove_simulation(player.state)

	def set_input(
		self,
		player_id: str,
		sequence: int,
		intent: arcade.MotionIntent,
	) -> None:
		player = self.players[player_id]
		if sequence <= player.last_received_input:
			return
		player.pending_inputs.append((sequence, intent))
		player.last_received_input = sequence

	def advance(self) -> None:
		for player in self.players.values():
			if player.pending_inputs:
				sequence, player.intent = player.pending_inputs.popleft()
				player.acknowledged_input = sequence
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

	def snapshot_for(self, player_id: str) -> dict:
		viewer_position = self.players[player_id].state.boat.position
		radius_m = config.VISIBILITY_RADIUS_M
		candidate_ids = {
			self.player_ids_by_shape[shape]
			for shape in self.space.bb_query(
				pymunk.BB(
					viewer_position.x - radius_m,
					viewer_position.y - radius_m,
					viewer_position.x + radius_m,
					viewer_position.y + radius_m,
				),
				pymunk.ShapeFilter(),
			)
		}
		boats = []
		for other_id, player in self.players.items():
			if other_id not in candidate_ids:
				continue
			distance_m = (player.state.boat.position - viewer_position).length
			if distance_m > config.VISIBILITY_RADIUS_M:
				continue
			item = {
				"id": other_id,
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
			if distance_m <= config.PHYSICS_BUBBLE_RADIUS_M:
				item["forward"] = player.intent.forward
				item["turn"] = player.intent.turn
			if other_id == player_id:
				item["acknowledgedInput"] = player.acknowledged_input
			boats.append(item)
		return {
			"tick": self.tick,
			"boats": boats,
		}
