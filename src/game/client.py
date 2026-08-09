import argparse
import asyncio
import json
import math
import random
from collections import deque
from contextlib import suppress
from dataclasses import dataclass

import pygame
import pymunk
from websockets.asyncio.client import ClientConnection, connect

from sailing import boat, simulation
from sailing.controllers import arcade
from sailing.physics import coupled

from . import config, input as player_input, protocol


WIDTH = 1280
HEIGHT = 720
PIXELS_PER_METER = 40


class DelayedMessages:
	def __init__(self, delay_seconds: float, jitter_seconds: float) -> None:
		if delay_seconds < 0 or jitter_seconds < 0:
			raise ValueError("network delay and jitter must not be negative")
		self.delay_seconds = delay_seconds
		self.jitter_seconds = jitter_seconds
		self._messages = deque()
		self._last_release_time = 0.0

	def add(self, message, now: float) -> None:
		release_time = max(
			now,
			now
			+ self.delay_seconds
			+ random.uniform(-self.jitter_seconds, self.jitter_seconds),
			self._last_release_time,
		)
		self._messages.append((release_time, message))
		self._last_release_time = release_time

	def take_ready(self, now: float) -> list:
		ready = []
		while self._messages and self._messages[0][0] <= now:
			_, message = self._messages.popleft()
			ready.append(message)
		return ready


class SnapshotPrediction:
	_predicted_fields = (
		("x", "vx"),
		("y", "vy"),
		("angle", "angularVelocity"),
		("sail", "sailAngularVelocity"),
		("rudder", "rudderAngularVelocity"),
	)
	_corrected_fields = ("x", "y", "angle", "sail", "rudder")
	_angular_fields = {"angle", "sail", "rudder"}

	def __init__(
		self,
		maximum_extrapolation_seconds: float = 0.25,
		correction_seconds: float = 0.1,
	) -> None:
		self.maximum_extrapolation_seconds = maximum_extrapolation_seconds
		self.correction_seconds = correction_seconds
		self._snapshot = None
		self._received_at = 0.0
		self._corrections = {}
		self._correction_started_at = 0.0

	def _extrapolated(self, now: float) -> dict:
		if self._snapshot is None:
			return {"type": "snapshot", "tick": 0, "boats": []}
		seconds = min(
			self.maximum_extrapolation_seconds,
			max(0, now - self._received_at),
		)
		boats = []
		for boat in self._snapshot["boats"]:
			predicted = dict(boat)
			for value_name, velocity_name in self._predicted_fields:
				predicted[value_name] += predicted[velocity_name] * seconds
			boats.append(predicted)
		return self._snapshot | {"boats": boats}

	def accept(self, snapshot: dict, received_at: float, now: float) -> None:
		previous = self.at(now) if self._snapshot is not None else None
		self._snapshot = snapshot
		self._received_at = received_at
		self._correction_started_at = now
		self._corrections = {}
		if previous is None:
			return
		previous_boats = {boat["id"]: boat for boat in previous["boats"]}
		for boat in self._extrapolated(now)["boats"]:
			if boat["id"] not in previous_boats:
				continue
			old = previous_boats[boat["id"]]
			correction = {}
			for field in self._corrected_fields:
				difference = old[field] - boat[field]
				if field in self._angular_fields:
					difference = math.remainder(difference, 2 * math.pi)
				correction[field] = difference
			self._corrections[boat["id"]] = correction

	def at(self, now: float) -> dict:
		predicted = self._extrapolated(now)
		if not self.correction_seconds:
			return predicted
		correction_scale = max(
			0,
			1 - (now - self._correction_started_at) / self.correction_seconds,
		)
		if not correction_scale:
			return predicted
		for boat in predicted["boats"]:
			correction = self._corrections.get(boat["id"])
			if correction is None:
				continue
			for field, difference in correction.items():
				boat[field] += difference * correction_scale
		return predicted


@dataclass
class BubbleBoat:
	state: boat.SailingState
	intent: arcade.MotionIntent


class PhysicsBubble:
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
		self.boats: dict[str, BubbleBoat] = {}
		self._last_time: float | None = None
		self._accumulator_seconds = 0.0

	def accept(
		self,
		snapshot: dict,
		local_id: str,
		local_intent: arcade.MotionIntent,
		received_at: float,
		now: float,
	) -> None:
		snapshot_boats = {item["id"]: item for item in snapshot["boats"]}
		local = snapshot_boats.get(local_id)
		if local is None:
			return
		local_position = pymunk.Vec2d(local["x"], local["y"])
		included = {
			player_id
			for player_id, item in snapshot_boats.items()
			if (pymunk.Vec2d(item["x"], item["y"]) - local_position).length
			<= config.PHYSICS_BUBBLE_RADIUS_M
		}
		for player_id in set(self.boats) - included:
			coupled.remove_simulation(self.boats.pop(player_id).state)
		for player_id in included:
			if player_id not in self.boats:
				state = coupled.create_simulation(
					self.settings.parameters,
					self.settings.initial_boat_angle_rad,
					self.space,
				)
				self.boats[player_id] = BubbleBoat(
					state,
					arcade.MotionIntent(),
				)
			item = snapshot_boats[player_id]
			coupled.restore_motion(
				self.boats[player_id].state,
				self.settings.parameters,
				boat.BoatMotion(
					position_m=(item["x"], item["y"]),
					velocity_m_s=(item["vx"], item["vy"]),
					angle_rad=item["angle"],
					angular_velocity_rad_s=item["angularVelocity"],
					sail_angle_rad=item["sail"],
					sail_angular_velocity_rad_s=item["sailAngularVelocity"],
					rudder_angle_rad=item["rudder"],
					rudder_angular_velocity_rad_s=item["rudderAngularVelocity"],
				),
			)
			self.boats[player_id].intent = arcade.MotionIntent(
				item["forward"],
				item["turn"],
			)
		self.boats[local_id].intent = local_intent
		self._last_time = received_at
		self._accumulator_seconds = 0
		self.advance_to(
			min(now, received_at + 0.25),
			local_id,
			local_intent,
		)
		self._last_time = now

	def advance_to(
		self,
		now: float,
		local_id: str,
		local_intent: arcade.MotionIntent,
	) -> None:
		if self._last_time is None:
			self._last_time = now
			return
		if local_id in self.boats:
			self.boats[local_id].intent = local_intent
		self._accumulator_seconds += max(0, now - self._last_time)
		self._last_time = now
		while self._accumulator_seconds >= self.dt_seconds:
			for predicted_boat in self.boats.values():
				simulation.apply_surface_game_forces(
					predicted_boat.state,
					predicted_boat.intent,
					simulation.COMPUTED_YAW_RATE_ARCADE_CONTROL,
					self.settings,
					self.environment,
				)
			self.space.step(self.dt_seconds)
			for predicted_boat in self.boats.values():
				coupled.synchronize(predicted_boat.state)
			self._accumulator_seconds -= self.dt_seconds

	def apply_to(self, snapshot: dict) -> dict:
		boats = []
		for item in snapshot["boats"]:
			predicted_boat = self.boats.get(item["id"])
			if predicted_boat is None:
				boats.append(item)
				continue
			state = predicted_boat.state
			boats.append(
				item
				| {
					"x": state.boat.position.x,
					"y": state.boat.position.y,
					"vx": state.boat.velocity.x,
					"vy": state.boat.velocity.y,
					"angle": state.boat.angle,
					"angularVelocity": state.boat.angular_velocity,
					"sail": state.sail_angle_rad,
					"sailAngularVelocity": state.sail_angular_velocity_rad_s,
					"rudder": state.rudder_angle_rad,
					"rudderAngularVelocity": (
						state.rudder_angular_velocity_rad_s
					),
				}
			)
		return snapshot | {"boats": boats}


def draw_snapshot(
	screen: pygame.Surface,
	font: pygame.font.Font,
	player_id: str,
	snapshot: dict,
	parameters: boat.BoatParameters,
) -> None:
	local_boat = next(
		(item for item in snapshot["boats"] if item["id"] == player_id),
		None,
	)
	camera_m = (
		pymunk.Vec2d(local_boat["x"], local_boat["y"])
		if local_boat is not None
		else pymunk.Vec2d(0, 0)
	)
	screen_center = pymunk.Vec2d(screen.get_width() / 2, screen.get_height() / 2)
	screen.fill((30, 30, 40))
	grid_spacing_px = 5 * PIXELS_PER_METER
	x_offset = -camera_m.x * PIXELS_PER_METER % grid_spacing_px
	y_offset = -camera_m.y * PIXELS_PER_METER % grid_spacing_px
	for x in range(round(x_offset), screen.get_width(), grid_spacing_px):
		pygame.draw.line(screen, (38, 46, 58), (x, 0), (x, screen.get_height()))
	for y in range(round(y_offset), screen.get_height(), grid_spacing_px):
		pygame.draw.line(screen, (38, 46, 58), (0, y), (screen.get_width(), y))

	half_length_px = parameters.length_m * PIXELS_PER_METER / 2
	half_beam_px = parameters.beam_m * PIXELS_PER_METER / 2
	bow_base_px = (
		parameters.length_m / 2 - parameters.bow_length_m
	) * PIXELS_PER_METER
	hull_points = (
		pymunk.Vec2d(half_length_px, 0),
		pymunk.Vec2d(bow_base_px, -half_beam_px),
		pymunk.Vec2d(-half_length_px, -half_beam_px),
		pymunk.Vec2d(-half_length_px, half_beam_px),
		pymunk.Vec2d(bow_base_px, half_beam_px),
	)
	offscreen_players = []
	for item in snapshot["boats"]:
		angle_rad = item["angle"]
		position_px = (
			(pymunk.Vec2d(item["x"], item["y"]) - camera_m)
			* PIXELS_PER_METER
			+ screen_center
		)
		color = (100, 220, 140) if item["id"] == player_id else (205, 205, 220)
		if item["id"] != player_id and not screen.get_rect().collidepoint(position_px):
			offscreen_players.append((position_px, color))
		pygame.draw.polygon(
			screen,
			color,
			[point.rotated(angle_rad) + position_px for point in hull_points],
			3,
		)
		mast_px = (
			pymunk.Vec2d(*parameters.mast_position_m).rotated(angle_rad)
			* PIXELS_PER_METER
			+ position_px
		)
		sail_direction = pymunk.Vec2d(-1, 0).rotated(angle_rad + item["sail"])
		pygame.draw.line(
			screen,
			(235, 205, 100),
			mast_px,
			mast_px + sail_direction * parameters.boom_length_m * PIXELS_PER_METER,
			4,
		)
		rudder_px = (
			pymunk.Vec2d(*parameters.rudder_position_m).rotated(angle_rad)
			* PIXELS_PER_METER
			+ position_px
		)
		rudder_direction = pymunk.Vec2d(-1, 0).rotated(
			angle_rad + item["rudder"]
		)
		pygame.draw.line(
			screen,
			(220, 220, 230),
			rudder_px,
			rudder_px
			+ rudder_direction
			* parameters.rudder_draw_length_m
			* PIXELS_PER_METER,
			4,
		)
		screen.blit(font.render(item["id"], True, color), position_px + (10, 10))

	for position_px, color in offscreen_players:
		direction = (position_px - screen_center).normalized()
		distance_to_edge_px = min(
			(screen.get_width() / 2 - 30) / abs(direction.x)
			if direction.x
			else math.inf,
			(screen.get_height() / 2 - 30) / abs(direction.y)
			if direction.y
			else math.inf,
		)
		tip = screen_center + direction * distance_to_edge_px
		normal = direction.perpendicular()
		pygame.draw.polygon(
			screen,
			color,
			(
				tip,
				tip - direction * 20 + normal * 9,
				tip - direction * 20 - normal * 9,
			),
		)

	status = f"{player_id}    server tick {snapshot['tick']}    WASD to sail"
	screen.blit(font.render(status, True, (220, 220, 230)), (20, 20))


async def receive_snapshots(
	connection: ClientConnection,
	snapshots: asyncio.Queue,
) -> None:
	loop = asyncio.get_running_loop()
	async for message in connection:
		payload = json.loads(message)
		if payload["type"] != "snapshot":
			continue
		if snapshots.full():
			snapshots.get_nowait()
		snapshots.put_nowait((loop.time(), payload))


async def run_client(uri: str, latency_ms: float = 0, jitter_ms: float = 0) -> None:
	pygame.init()
	screen = pygame.display.set_mode((WIDTH, HEIGHT))
	pygame.display.set_caption("Sailing multiplayer client")
	font = pygame.font.Font(None, 24)
	clock = pygame.time.Clock()
	one_way_delay_seconds = latency_ms / 2000
	jitter_seconds = jitter_ms / 1000
	incoming_messages = DelayedMessages(one_way_delay_seconds, jitter_seconds)
	outgoing_messages = DelayedMessages(one_way_delay_seconds, jitter_seconds)
	prediction = SnapshotPrediction()
	physics_bubble = PhysicsBubble(
		config.SIMULATION_SETTINGS,
		config.ENVIRONMENT,
		config.DT_SECONDS,
	)
	try:
		async with connect(uri) as connection:
			welcome = json.loads(await connection.recv())
			player_id = welcome["playerId"]
			snapshots: asyncio.Queue = asyncio.Queue(maxsize=1)
			receiver = asyncio.create_task(receive_snapshots(connection, snapshots))
			last_intent = None
			intent = arcade.MotionIntent()
			sequence = 0
			running = True
			try:
				while running:
					for event in pygame.event.get():
						if event.type == pygame.QUIT:
							running = False
					now = asyncio.get_running_loop().time()
					keys = pygame.key.get_pressed()
					intent = player_input.keyboard_motion(
						keys[pygame.K_w],
						keys[pygame.K_s],
						keys[pygame.K_a],
						keys[pygame.K_d],
					)
					while not snapshots.empty():
						received_at, snapshot = snapshots.get_nowait()
						incoming_messages.add((received_at, snapshot), received_at)
					ready_snapshots = incoming_messages.take_ready(now)
					for received_at, snapshot in ready_snapshots:
						prediction.accept(snapshot, received_at, now)
						physics_bubble.accept(
							snapshot,
							player_id,
							intent,
							received_at,
							now,
						)
					physics_bubble.advance_to(now, player_id, intent)
					if intent != last_intent:
						sequence += 1
						outgoing_messages.add(
							protocol.input_message(sequence, intent),
							now,
						)
						last_intent = intent
					for message in outgoing_messages.take_ready(now):
						await connection.send(message)
					draw_snapshot(
						screen,
						font,
						player_id,
						physics_bubble.apply_to(prediction.at(now)),
						config.BOAT_PARAMETERS,
					)
					pygame.display.flip()
					clock.tick(60)
					await asyncio.sleep(0)
					if receiver.done():
						await receiver
			finally:
				receiver.cancel()
				with suppress(asyncio.CancelledError):
					await receiver
	finally:
		pygame.quit()


def main() -> None:
	parser = argparse.ArgumentParser()
	parser.add_argument("--uri", default="ws://127.0.0.1:8765")
	parser.add_argument(
		"--latency-ms",
		type=float,
		default=0,
		help="artificial round-trip latency",
	)
	parser.add_argument(
		"--jitter-ms",
		type=float,
		default=0,
		help="artificial one-way timing variation",
	)
	arguments = parser.parse_args()
	asyncio.run(
		run_client(arguments.uri, arguments.latency_ms, arguments.jitter_ms)
	)


if __name__ == "__main__":
	main()
