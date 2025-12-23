import math
import pygame
import pymunk

# ------------------ Physics helpers ------------------

def sail_force_local(apparent_wind_local: pymunk.vec2d.Vec2d, sail_angle: float) -> pymunk.vec2d.Vec2d:
	speed: float = apparent_wind_local.length
	if speed == 0:
		return pymunk.vec2d.Vec2d(0, 0)

	wind_dir: pymunk.vec2d.Vec2d = apparent_wind_local.normalized()

	sail_dir = pymunk.vec2d.Vec2d(0, 1).rotated(sail_angle)
	sail_normal = sail_dir.perpendicular()

	aoa = max(-1.0, min(1.0, wind_dir.dot(tuple[float, float](sail_normal))))
	aoa = math.asin(aoa)

	lift_coeff = math.sin(2 * aoa)
	drag_coeff = abs(math.cos(aoa))

	lift = lift_coeff * speed * speed
	drag = drag_coeff * speed * speed

	return lift * wind_dir.perpendicular() + tuple[float, float](drag * wind_dir)


def keel_force_local(apparent_water_local, keel_strength):
	# resist sideways motion (local X)
	return pymunk.vec2d.Vec2d(-apparent_water_local.x * keel_strength, 0)


def rudder_torque_local(apparent_water_local, rudder_angle):
	forward_speed = apparent_water_local.y
	return rudder_angle * forward_speed


def optimal_sail_angle(apparent_wind_local, samples=60):
	if apparent_wind_local.length == 0:
		return 0.0

	best_angle = 0.0
	best_thrust = -1e9
	forward = pymunk.vec2d.Vec2d(0, 1)

	for i in range(samples):
		angle = -math.pi / 2 + i * (math.pi / samples)
		f = sail_force_local(apparent_wind_local, angle)
		thrust = f.dot(forward)
		if thrust > best_thrust:
			best_thrust = thrust
			best_angle = angle

	return best_angle


# ------------------ Config ------------------

WIDTH, HEIGHT = 1280, 720
DT = 1 / 60.0

WIND_WORLD = pymunk.vec2d.Vec2d(5, 0)  # constant wind to the right

SAIL_STRENGTH = 10.0
KEEL_STRENGTH = 10.0
RUDDER_STRENGTH = 10.0

SAIL_ROT_SPEED = 1.2       # rad/sec
RUDDER_ROT_SPEED = 1.8    # rad/sec

MAX_RUDDER_ANGLE = math.radians(30)
MAX_SAIL_ANGLE = math.pi / 2

# ------------------ Game setup ------------------

pygame.init()
screen = pygame.display.set_mode((WIDTH, HEIGHT))
clock = pygame.time.Clock()

space = pymunk.Space()
space.damping = 0.99

# Boat body
mass = 1.0
size = (20, 60)
inertia = pymunk.moment_for_box(mass, size)
boat = pymunk.Body(mass, inertia)
boat.position = WIDTH / 2, HEIGHT / 2
boat.angle = 0

shape = pymunk.Poly.create_box(boat, size)
shape.friction = 0.3

space.add(boat, shape)

# ------------------ Control state ------------------

sail_angle = 0.0
rudder_angle = 0.0

# ------------------ Main loop ------------------

running = True
while running:
	for event in pygame.event.get():
		if event.type == pygame.QUIT:
			running = False

	keys = pygame.key.get_pressed()

	# --- World → boat local ---

	apparent_water_local = -boat.velocity.rotated(-boat.angle)
	apparent_wind_local = (WIND_WORLD - boat.velocity).rotated(-boat.angle)

	# --- Rudder control ---
	if keys[pygame.K_a]:
		rudder_angle += RUDDER_ROT_SPEED * DT
	elif keys[pygame.K_d]:
		rudder_angle -= RUDDER_ROT_SPEED * DT
	else:
		rudder_angle *= 0.9  # relax to center

	rudder_angle = max(-MAX_RUDDER_ANGLE, min(MAX_RUDDER_ANGLE, rudder_angle))

	# --- Sail control ---
	optimal = optimal_sail_angle(apparent_wind_local)

	if keys[pygame.K_w]:
		sail_angle += (optimal - sail_angle) * SAIL_ROT_SPEED * DT
	elif keys[pygame.K_s]:
		# deliberately bad trim: flip optimal
		bad = -optimal
		sail_angle += (bad - sail_angle) * SAIL_ROT_SPEED * DT

	sail_angle = max(-MAX_SAIL_ANGLE, min(MAX_SAIL_ANGLE, sail_angle))

	# --- Forces (local) ---
	sail_f_local = sail_force_local(apparent_wind_local, sail_angle) * SAIL_STRENGTH
	keel_f_local = keel_force_local(apparent_water_local, KEEL_STRENGTH)
	total_local_force = sail_f_local + keel_f_local

# --- Apply forces ---
	total_world_force = total_local_force.rotated(boat.angle)
	boat.apply_force_at_local_point(tuple[float, float](total_world_force), (0, 0))

	torque = rudder_torque_local(apparent_water_local, rudder_angle) * RUDDER_STRENGTH
	boat.torque += torque

	# --- Step physics ---
	space.step(DT)

	# ------------------ Rendering ------------------

	screen.fill((30, 30, 40))

	# Boat
	points = [p.rotated(boat.angle) + tuple[float, float](boat.position) for p in shape.get_vertices()]
	pygame.draw.polygon(screen, (200, 200, 220), points, 2)

	# Sail (visual)
	sail_dir_world = pymunk.vec2d.Vec2d(0, 1).rotated(sail_angle + boat.angle)
	sail_start = boat.position
	sail_end = sail_start + tuple[float, float](sail_dir_world * 40)
	pygame.draw.line(screen, (200, 200, 100), sail_start, sail_end, 2)

	# Wind indicator
	wind_end = pymunk.vec2d.Vec2d(60, 0)
	pygame.draw.line(screen, (100, 200, 255), (40, 40), (40, 40) + wind_end, 2)

	pygame.display.flip()
	clock.tick(60)

pygame.quit()
