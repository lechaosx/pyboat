import argparse
import asyncio
import json
import math
from contextlib import suppress

import pygame
import pymunk
from websockets.asyncio.client import ClientConnection, connect

from sailing import boat

from . import config, input as player_input, protocol


WIDTH = 1280
HEIGHT = 720
PIXELS_PER_METER = 40


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
	async for message in connection:
		payload = json.loads(message)
		if payload["type"] != "snapshot":
			continue
		if snapshots.full():
			snapshots.get_nowait()
		snapshots.put_nowait(payload)


async def run_client(uri: str) -> None:
	pygame.init()
	screen = pygame.display.set_mode((WIDTH, HEIGHT))
	pygame.display.set_caption("Sailing multiplayer client")
	font = pygame.font.Font(None, 24)
	clock = pygame.time.Clock()
	try:
		async with connect(uri) as connection:
			welcome = json.loads(await connection.recv())
			player_id = welcome["playerId"]
			snapshots: asyncio.Queue = asyncio.Queue(maxsize=1)
			receiver = asyncio.create_task(receive_snapshots(connection, snapshots))
			latest_snapshot = {"tick": 0, "boats": []}
			last_intent = None
			sequence = 0
			running = True
			try:
				while running:
					for event in pygame.event.get():
						if event.type == pygame.QUIT:
							running = False
					while not snapshots.empty():
						latest_snapshot = snapshots.get_nowait()
					keys = pygame.key.get_pressed()
					intent = player_input.keyboard_motion(
						keys[pygame.K_w],
						keys[pygame.K_s],
						keys[pygame.K_a],
						keys[pygame.K_d],
					)
					if intent != last_intent:
						sequence += 1
						await connection.send(protocol.input_message(sequence, intent))
						last_intent = intent
					draw_snapshot(
						screen,
						font,
						player_id,
						latest_snapshot,
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
	arguments = parser.parse_args()
	asyncio.run(run_client(arguments.uri))


if __name__ == "__main__":
	main()
