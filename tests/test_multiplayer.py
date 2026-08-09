import asyncio
import json
import unittest
from unittest.mock import patch

import pygame

from game import config
from game.client import (
	DelayedMessages,
	InputHistory,
	PhysicsBubble,
	SnapshotPrediction,
	draw_snapshot,
	receive_snapshots,
)
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

		snapshot = self.match.snapshot_for(first_id)
		boats = {boat["id"]: boat for boat in snapshot["boats"]}
		self.assertEqual(boats[first_id]["acknowledgedInput"], 7)
		self.assertNotIn("acknowledgedInput", boats[second_id])
		self.assertEqual(boats[first_id]["forward"], 1)
		self.assertEqual(boats[first_id]["turn"], -0.5)
		self.assertNotEqual(
			(boats[first_id]["vx"], boats[first_id]["vy"]),
			(boats[second_id]["vx"], boats[second_id]["vy"]),
		)

	def test_snapshot_detail_and_visibility_are_relative_to_player(self):
		local_id = self.match.add_player()
		near_id = self.match.add_player()
		visible_id = self.match.add_player()
		visibility_edge_id = self.match.add_player()
		far_id = self.match.add_player()

		boats = {
			item["id"]: item for item in self.match.snapshot_for(local_id)["boats"]
		}

		self.assertEqual(
			set(boats),
			{local_id, near_id, visible_id, visibility_edge_id},
		)
		self.assertIn("forward", boats[local_id])
		self.assertIn("forward", boats[near_id])
		self.assertNotIn("forward", boats[visible_id])

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

	def test_draw_snapshot_keeps_stale_remote_position_as_an_edge_arrow(self):
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
						"x": 2,
						"y": 0,
						"angle": 0,
						"sail": 0,
						"rudder": 0,
						"stale": True,
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


class NetworkConditionsTest(unittest.TestCase):
	def test_messages_wait_for_the_configured_delay(self):
		messages = DelayedMessages(delay_seconds=0.05, jitter_seconds=0)
		messages.add("snapshot", now=1)

		self.assertEqual(messages.take_ready(now=1.049), [])
		self.assertEqual(messages.take_ready(now=1.05), ["snapshot"])

	def test_jitter_does_not_reorder_websocket_messages(self):
		messages = DelayedMessages(delay_seconds=0.1, jitter_seconds=0.05)
		with patch("game.client.random.uniform", side_effect=(0.05, -0.05)):
			messages.add("first", now=1)
			messages.add("second", now=1.01)

		self.assertEqual(messages.take_ready(now=1.149), [])
		self.assertEqual(messages.take_ready(now=1.151), ["first", "second"])


class SnapshotReceiverTest(unittest.IsolatedAsyncioTestCase):
	async def test_burst_keeps_the_latest_snapshot(self):
		class Connection:
			def __aiter__(self):
				async def messages():
					yield json.dumps({"type": "snapshot", "tick": 1})
					yield json.dumps({"type": "snapshot", "tick": 2})

				return messages()

		snapshots = asyncio.Queue(maxsize=1)
		await receive_snapshots(Connection(), snapshots)

		_, snapshot = snapshots.get_nowait()
		self.assertEqual(snapshot["tick"], 2)


class SnapshotPredictionTest(unittest.TestCase):
	def boat(self, **changes):
		return {
			"id": "player-1",
			"x": 10,
			"y": 5,
			"vx": 2,
			"vy": -1,
			"angle": 0.5,
			"angularVelocity": 0.2,
			"sail": 0.1,
			"sailAngularVelocity": -0.1,
			"rudder": -0.2,
			"rudderAngularVelocity": 0.4,
		} | changes

	def test_extrapolates_snapshot_to_render_time(self):
		prediction = SnapshotPrediction()
		prediction.accept(
			{"type": "snapshot", "tick": 1, "boats": [self.boat()]},
			received_at=1,
			now=1,
		)

		boat = prediction.at(1.05)["boats"][0]
		self.assertAlmostEqual(boat["x"], 10.1)
		self.assertAlmostEqual(boat["y"], 4.95)
		self.assertAlmostEqual(boat["angle"], 0.51)
		self.assertAlmostEqual(boat["sail"], 0.095)
		self.assertAlmostEqual(boat["rudder"], -0.18)

	def test_stops_extrapolating_after_a_quarter_second(self):
		prediction = SnapshotPrediction()
		prediction.accept(
			{"type": "snapshot", "tick": 1, "boats": [self.boat()]},
			received_at=1,
			now=1,
		)

		boat = prediction.at(2)["boats"][0]
		self.assertAlmostEqual(boat["x"], 10.5)
		self.assertAlmostEqual(boat["y"], 4.75)

	def test_new_authoritative_snapshot_does_not_jump_at_reconciliation(self):
		prediction = SnapshotPrediction(correction_seconds=0.1)
		prediction.accept(
			{"type": "snapshot", "tick": 1, "boats": [self.boat(x=0, vx=1)]},
			received_at=1,
			now=1,
		)
		before = prediction.at(1.05)["boats"][0]["x"]

		prediction.accept(
			{"type": "snapshot", "tick": 2, "boats": [self.boat(x=0.03, vx=1)]},
			received_at=1.05,
			now=1.05,
		)

		self.assertAlmostEqual(prediction.at(1.05)["boats"][0]["x"], before)
		self.assertAlmostEqual(prediction.at(1.15)["boats"][0]["x"], 0.13)

	def test_keeps_last_position_for_active_boat_omitted_by_interest(self):
		prediction = SnapshotPrediction()
		prediction.accept(
			{
				"type": "snapshot",
				"tick": 1,
				"removedPlayerIds": [],
				"boats": [self.boat(), self.boat(id="player-2", x=24)],
			},
			received_at=1,
			now=1,
		)

		prediction.accept(
			{
				"type": "snapshot",
				"tick": 2,
				"removedPlayerIds": [],
				"boats": [self.boat()],
			},
			received_at=1.1,
			now=1.1,
		)

		remote = next(
			item for item in prediction.at(2)["boats"] if item["id"] == "player-2"
		)
		self.assertEqual(remote["x"], 24)
		self.assertTrue(remote["stale"])

	def test_removes_last_position_when_boat_disconnects(self):
		prediction = SnapshotPrediction()
		prediction.accept(
			{
				"type": "snapshot",
				"tick": 1,
				"removedPlayerIds": [],
				"boats": [self.boat(), self.boat(id="player-2", x=24)],
			},
			received_at=1,
			now=1,
		)

		prediction.accept(
			{
				"type": "snapshot",
				"tick": 2,
				"removedPlayerIds": ["player-2"],
				"boats": [self.boat()],
			},
			received_at=1.1,
			now=1.1,
		)

		self.assertEqual(
			[item["id"] for item in prediction.at(1.1)["boats"]],
			["player-1"],
		)


class InputHistoryTest(unittest.TestCase):
	def test_replays_only_unacknowledged_input_timeline(self):
		history = InputHistory()
		turning = arcade.MotionIntent(forward=1, turn=1)
		straight = arcade.MotionIntent(forward=1, turn=0)
		history.record(1, turning, now=1)
		history.record(2, straight, now=1.05)

		segments = history.replay_segments(
			acknowledged_sequence=1,
			server_intent=turning,
			received_at=1.1,
			now=1.15,
			maximum_seconds=0.25,
		)
		self.assertEqual(len(segments), 1)
		self.assertAlmostEqual(segments[0][0], 0.1)
		self.assertEqual(segments[0][1], straight)

		history.acknowledge(1)
		self.assertEqual([item.sequence for item in history.commands], [2])


class PhysicsBubbleTest(unittest.TestCase):
	def boat(self, player_id, x, y=0, vx=0, vy=0, **changes):
		return {
			"id": player_id,
			"acknowledgedInput": 0,
			"forward": 0,
			"turn": 0,
			"x": x,
			"y": y,
			"vx": vx,
			"vy": vy,
			"angle": 0,
			"angularVelocity": 0,
			"sail": 0,
			"sailAngularVelocity": 0,
			"rudder": 0,
			"rudderAngularVelocity": 0,
		} | changes

	def bubble(self):
		return PhysicsBubble(
			config.SIMULATION_SETTINGS,
			config.ENVIRONMENT,
			config.DT_SECONDS,
		)

	def test_simulates_local_and_nearby_boats_in_one_space(self):
		bubble = self.bubble()
		snapshot = {
			"type": "snapshot",
			"tick": 1,
			"boats": [
				self.boat("local", 0),
				self.boat("near", 6),
				self.boat("far", 10),
			],
		}

		bubble.accept(
			snapshot,
			"local",
			InputHistory(),
			received_at=1,
			now=1,
		)

		self.assertEqual(set(bubble.boats), {"local", "near"})
		self.assertEqual(len(bubble.space.bodies), 6)

	def test_nearby_boats_predict_their_collision(self):
		bubble = self.bubble()
		snapshot = {
			"type": "snapshot",
			"tick": 1,
			"boats": [
				self.boat("local", -3, vx=5),
				self.boat("other", 3, vx=-5),
			],
		}
		bubble.accept(
			snapshot,
			"local",
			InputHistory(),
			received_at=1,
			now=1,
		)

		bubble.advance_to(1.5, "local", arcade.MotionIntent())

		self.assertLess(bubble.boats["local"].state.boat.velocity.x, 3)
		self.assertGreater(bubble.boats["other"].state.boat.velocity.x, -3)

	def test_replays_unacknowledged_turn_instead_of_waiting_for_server(self):
		bubble = self.bubble()
		history = InputHistory()
		history.record(1, arcade.MotionIntent(forward=1, turn=1), now=1)
		snapshot = {
			"type": "snapshot",
			"tick": 1,
			"boats": [self.boat("local", 0, vx=4)],
		}

		bubble.accept(
			snapshot,
			"local",
			history,
			received_at=1.1,
			now=1.1,
		)

		self.assertNotEqual(bubble.boats["local"].state.rudder_angle_rad, 0)


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

	async def test_two_clients_receive_distinct_ids_and_player_specific_snapshots(self):
		first, first_id = await self.connect_player()
		second, second_id = await self.connect_player()

		self.game_server.advance()

		first_snapshot = json.loads(await asyncio.wait_for(first.recv(), 0.2))
		second_snapshot = json.loads(await asyncio.wait_for(second.recv(), 0.2))
		self.assertNotEqual(first_id, second_id)
		self.assertNotEqual(first_snapshot, second_snapshot)
		self.assertEqual(
			{boat["id"] for boat in first_snapshot["boats"]},
			{first_id, second_id},
		)
		first_boats = {boat["id"]: boat for boat in first_snapshot["boats"]}
		second_boats = {boat["id"]: boat for boat in second_snapshot["boats"]}
		self.assertIn("acknowledgedInput", first_boats[first_id])
		self.assertNotIn("acknowledgedInput", first_boats[second_id])
		self.assertIn("acknowledgedInput", second_boats[second_id])
		self.assertNotIn("acknowledgedInput", second_boats[first_id])

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

	async def test_disconnect_sends_tombstone_only_to_client_that_saw_player(self):
		first, _ = await self.connect_player()
		second, second_id = await self.connect_player()
		self.game_server.advance()
		await first.recv()
		await second.recv()

		await second.close()
		for _ in range(100):
			if second_id not in self.game_server.match.players:
				break
			await asyncio.sleep(0)
		self.game_server.advance()

		snapshot = json.loads(await asyncio.wait_for(first.recv(), 0.2))
		self.assertEqual(snapshot["removedPlayerIds"], [second_id])


if __name__ == "__main__":
	unittest.main()
