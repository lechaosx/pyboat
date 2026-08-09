import json
import math

from sailing.controllers import arcade


def input_message(sequence: int, intent: arcade.MotionIntent) -> str:
	return json.dumps(
		{
			"type": "input",
			"sequence": sequence,
			"forward": intent.forward,
			"turn": intent.turn,
		},
		separators=(",", ":"),
	)


def parse_input(message: str) -> tuple[int, arcade.MotionIntent]:
	payload = json.loads(message)
	if payload["type"] != "input":
		raise ValueError("expected input message")
	forward = float(payload["forward"])
	turn = float(payload["turn"])
	if not all(math.isfinite(value) and -1 <= value <= 1 for value in (forward, turn)):
		raise ValueError("input values must be between -1 and 1")
	return (
		int(payload["sequence"]),
		arcade.MotionIntent(forward, turn),
	)


def welcome_message(player_id: str) -> str:
	return json.dumps(
		{"type": "welcome", "playerId": player_id},
		separators=(",", ":"),
	)


def snapshot_message(snapshot: dict) -> str:
	return json.dumps(
		{"type": "snapshot", **snapshot},
		separators=(",", ":"),
	)
