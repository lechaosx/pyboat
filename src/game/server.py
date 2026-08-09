import argparse
import asyncio

from websockets.asyncio.server import ServerConnection, broadcast, serve

from . import config, protocol
from .match import Match


class GameServer:
	def __init__(self, match: Match, snapshot_interval_ticks: int = 1) -> None:
		self.match = match
		self.snapshot_interval_ticks = snapshot_interval_ticks
		self.connections: dict[ServerConnection, str] = {}
		self.known_player_ids: dict[ServerConnection, set[str]] = {}
		self.closing_connections: set[ServerConnection] = set()

	async def handle(self, connection: ServerConnection) -> None:
		player_id = self.match.add_player()
		self.connections[connection] = player_id
		self.known_player_ids[connection] = set()
		await connection.send(protocol.welcome_message(player_id))
		try:
			async for message in connection:
				sequence, intent = protocol.parse_input(message)
				self.match.set_input(player_id, sequence, intent)
		finally:
			del self.connections[connection]
			del self.known_player_ids[connection]
			self.closing_connections.discard(connection)
			if not self.match.players[player_id].wreck:
				self.match.remove_player(player_id)

	def advance(self) -> None:
		self.match.advance()
		for connection, player_id in self.connections.items():
			if self.match.players[player_id].wreck:
				if connection not in self.closing_connections:
					self.closing_connections.add(connection)
					asyncio.create_task(connection.close(reason="boat destroyed"))
		if self.match.tick % self.snapshot_interval_ticks == 0:
			for connection, player_id in self.connections.items():
				if self.match.players[player_id].wreck:
					continue
				snapshot = self.match.snapshot_for(player_id)
				known_ids = self.known_player_ids[connection]
				removed_ids = sorted(known_ids - self.match.players.keys())
				known_ids.difference_update(removed_ids)
				known_ids.update(item["id"] for item in snapshot["boats"])
				snapshot["removedPlayerIds"] = removed_ids
				broadcast(
					(connection,),
					protocol.snapshot_message(snapshot),
				)

	async def run(self) -> None:
		loop = asyncio.get_running_loop()
		next_tick_time = loop.time()
		while True:
			self.advance()
			next_tick_time += self.match.dt_seconds
			await asyncio.sleep(max(0, next_tick_time - loop.time()))


async def run_server(host: str, port: int) -> None:
	match = Match(
		config.SIMULATION_SETTINGS,
		config.ENVIRONMENT,
		config.DT_SECONDS,
	)
	game_server = GameServer(match)
	async with serve(game_server.handle, host, port):
		print(f"Server listening on ws://{host}:{port}")
		await game_server.run()


def main() -> None:
	parser = argparse.ArgumentParser()
	parser.add_argument("--host", default="127.0.0.1")
	parser.add_argument("--port", type=int, default=8765)
	arguments = parser.parse_args()
	asyncio.run(run_server(arguments.host, arguments.port))


if __name__ == "__main__":
	main()
