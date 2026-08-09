import asyncio
import json
import unittest

import pygame

from game import config
from game.client import draw_snapshot
from game.match import Match
from game.protocol import input_message, parse_input, snapshot_message, welcome_message
from game.server import GameServer
from sailing.controllers import arcade
from websockets.asyncio.client import connect
from websockets.asyncio.server import serve


class MatchTest(unittest.TestCase):
	def setUp(self):
		self.match = Match(
			config.SIMULATION_SETTINGS,
			config.ENVIRONMENT,
			config.DT_SECONDS,
		)

	def test_two_players_share_one_authoritative_physics_space(self):
		first_id = self.match.add_player()
		second_id = self.match.add_player()

		self.assertIs(
			self.match.players[first_id].state.space,
			self.match.players[second_id].state.space,
		)
		self.assertEqual(len(self.match.players[first_id].state.space.bodies), 6)

	def test_input_controls_only_its_player_and_is_acknowledged(self):
		first_id = self.match.add_player()
		second_id = self.match.add_player()
		self.match.set_input(first_id, 7, arcade.MotionIntent(forward=1, turn=-0.5))

		for _ in range(120):
			self.match.advance()

		snapshot = self.match.snapshot()
		boats = {boat["id"]: boat for boat in snapshot["boats"]}
		self.assertEqual(boats[first_id]["acknowledgedInput"], 7)
		self.assertEqual(boats[second_id]["acknowledgedInput"], 0)
		self.assertNotEqual(
			(boats[first_id]["vx"], boats[first_id]["vy"]),
			(boats[second_id]["vx"], boats[second_id]["vy"]),
		)

	def test_removing_player_removes_its_bodies_from_the_space(self):
		first_id = self.match.add_player()
		second_id = self.match.add_player()
		space = self.match.players[first_id].state.space

		self.match.remove_player(second_id)

		self.assertEqual(len(space.bodies), 3)
		self.assertEqual(set(self.match.players), {first_id})


class ProtocolTest(unittest.TestCase):
	def test_input_round_trip(self):
		encoded = input_message(12, arcade.MotionIntent(0.75, -0.25))

		self.assertEqual(
			parse_input(encoded),
			(12, arcade.MotionIntent(0.75, -0.25)),
		)

	def test_input_rejects_values_outside_the_control_range(self):
		with self.assertRaises(ValueError):
			parse_input(
				json.dumps(
					{"type": "input", "sequence": 1, "forward": 2, "turn": 0}
				)
			)

	def test_welcome_identifies_the_assigned_player(self):
		self.assertEqual(
			json.loads(welcome_message("player-3")),
			{"type": "welcome", "playerId": "player-3"},
		)

	def test_snapshot_is_json_with_a_server_tick(self):
		message = snapshot_message(
			{
				"tick": 4,
				"boats": [{"id": "player-1", "x": 1.5}],
			}
		)

		self.assertEqual(
			json.loads(message),
			{
				"type": "snapshot",
				"tick": 4,
				"boats": [{"id": "player-1", "x": 1.5}],
			},
		)


class ClientRenderTest(unittest.TestCase):
	def test_draw_snapshot_renders_local_and_remote_boats(self):
		pygame.font.init()
		screen = pygame.Surface((640, 360))
		draw_snapshot(
			screen,
			pygame.font.Font(None, 24),
			"player-1",
			{
				"tick": 3,
				"boats": [
					{
						"id": "player-1",
						"x": 0,
						"y": 0,
						"angle": 0,
						"sail": 0.5,
						"rudder": -0.2,
					},
					{
						"id": "player-2",
						"x": 5,
						"y": 2,
						"angle": 1,
						"sail": -0.5,
						"rudder": 0.2,
					},
				],
			},
			config.BOAT_PARAMETERS,
		)

		colors = {
			tuple(screen.get_at((x, y))) for x in range(640) for y in range(360)
		}
		self.assertIn((100, 220, 140, 255), colors)
		self.assertIn((205, 205, 220, 255), colors)

	def test_draw_snapshot_points_to_remote_boat_outside_the_viewport(self):
		pygame.font.init()
		screen = pygame.Surface((640, 360))
		draw_snapshot(
			screen,
			pygame.font.Font(None, 24),
			"player-1",
			{
				"tick": 3,
				"boats": [
					{
						"id": "player-1",
						"x": 0,
						"y": 0,
						"angle": 0,
						"sail": 0,
						"rudder": 0,
					},
					{
						"id": "player-2",
						"x": 100,
						"y": 0,
						"angle": 0,
						"sail": 0,
						"rudder": 0,
					},
				],
			},
			config.BOAT_PARAMETERS,
		)

		arrow_pixels = sum(
			tuple(screen.get_at((x, y))) == (205, 205, 220, 255)
			for x in range(590, 640)
			for y in range(130, 230)
		)
		self.assertGreater(arrow_pixels, 0)


class ServerTest(unittest.IsolatedAsyncioTestCase):
	async def asyncSetUp(self):
		match = Match(
			config.SIMULATION_SETTINGS,
			config.ENVIRONMENT,
			config.DT_SECONDS,
		)
		self.game_server = GameServer(match)
		self.websocket_server = await serve(
			self.game_server.handle,
			"127.0.0.1",
			0,
		)
		port = self.websocket_server.sockets[0].getsockname()[1]
		self.uri = f"ws://127.0.0.1:{port}"
		self.connections = []

	async def asyncTearDown(self):
		for connection in self.connections:
			await connection.close()
		self.websocket_server.close()
		await self.websocket_server.wait_closed()

	async def connect_player(self):
		connection = await connect(self.uri)
		self.connections.append(connection)
		welcome = json.loads(await connection.recv())
		return connection, welcome["playerId"]

	async def test_two_clients_receive_distinct_ids_and_the_shared_snapshot(self):
		first, first_id = await self.connect_player()
		second, second_id = await self.connect_player()

		self.game_server.advance()

		first_snapshot = json.loads(await asyncio.wait_for(first.recv(), 0.2))
		second_snapshot = json.loads(await asyncio.wait_for(second.recv(), 0.2))
		self.assertNotEqual(first_id, second_id)
		self.assertEqual(first_snapshot, second_snapshot)
		self.assertEqual(
			{boat["id"] for boat in first_snapshot["boats"]},
			{first_id, second_id},
		)

	async def test_client_input_is_applied_and_acknowledged(self):
		connection, player_id = await self.connect_player()
		await connection.send(input_message(8, arcade.MotionIntent(1, -0.5)))
		for _ in range(100):
			if self.game_server.match.players[player_id].acknowledged_input == 8:
				break
			await asyncio.sleep(0)

		self.game_server.advance()

		snapshot = json.loads(await asyncio.wait_for(connection.recv(), 0.2))
		boat = next(boat for boat in snapshot["boats"] if boat["id"] == player_id)
		self.assertEqual(boat["acknowledgedInput"], 8)


if __name__ == "__main__":
	unittest.main()
