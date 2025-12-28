import math
import pygame
import pymunk


def lift(angle_of_attack: float) -> float:
	return math.sin(angle_of_attack * 2)


def drag(angle_of_attack: float) -> float:
	return 1 - math.cos(angle_of_attack * 2)


def foil_force(apparent_flow_speed: float, angle_of_attack: float) -> pymunk.vec2d.Vec2d:
	lift_force_mag = lift(angle_of_attack) * apparent_flow_speed * apparent_flow_speed
	drag_force_mag = drag(angle_of_attack) * apparent_flow_speed * apparent_flow_speed

	return pymunk.vec2d.Vec2d(drag_force_mag, -lift_force_mag)


WIDTH, HEIGHT = 1280, 720
DT = 1 / 60.0

PROPER_WIND = pymunk.vec2d.Vec2d(1, 0)
PROPER_WATER = pymunk.vec2d.Vec2d(0, 0)

SAIL_STRENGTH = 1.0
KEEL_STRENGTH = 1.0
HULL_STRENGTH = 0.1
RUDDER_STRENGTH = 1.0

RUDDER_ROT_SPEED = math.pi

MAX_RUDDER_ANGLE = math.radians(30)


pygame.init()
screen = pygame.display.set_mode((WIDTH, HEIGHT))
clock = pygame.time.Clock()

space = pymunk.Space()

mass = 1.0
size = (60, 20)
moment = pymunk.moment_for_box(mass, size)
boat = pymunk.Body(mass, moment)
boat.angle = 135 * math.pi / 180

shape = pymunk.Poly.create_box(boat, size)

space.add(boat, shape)

sail_angle = 0.0
rudder_angle = 0.0

running = True
while running:
	for event in pygame.event.get():
		if event.type == pygame.QUIT:
			running = False

	keys = pygame.key.get_pressed()

	if keys[pygame.K_a]:
		rudder_angle += RUDDER_ROT_SPEED * DT
	elif keys[pygame.K_d]:
		rudder_angle -= RUDDER_ROT_SPEED * DT
	else:
		rudder_angle *= 0.9  # relax to center

	rudder_angle = max(-MAX_RUDDER_ANGLE, min(MAX_RUDDER_ANGLE, rudder_angle))

	apparent_water = PROPER_WATER - boat.velocity
	apparent_wind = PROPER_WIND - boat.velocity

	def compute_force(apparent_flow, angle_offset, strength):
		aoa = boat.angle + angle_offset - math.pi - apparent_flow.angle
		return foil_force(apparent_flow.length, aoa).rotated(apparent_flow.angle) * strength

	if keys[pygame.K_w]:
		def sail_effort_at_angle(foil_angle):
			s_force = compute_force(apparent_wind, foil_angle, SAIL_STRENGTH)
			k_force = compute_force(apparent_water, 0, KEEL_STRENGTH)
			h_force = compute_force(apparent_water, 0, HULL_STRENGTH)
			total_force = s_force + k_force + h_force
			boat_forward = pymunk.vec2d.Vec2d(1, 0).rotated(boat.angle)
			return total_force.dot(boat_forward)

		sail_angle = max((i * math.pi / 180 for i in range(-90, 91)), key=sail_effort_at_angle)

	sail_force = compute_force(apparent_wind, sail_angle, SAIL_STRENGTH)
	keel_force = compute_force(apparent_water, 0, KEEL_STRENGTH)
	hull_force = compute_force(apparent_water, 0, HULL_STRENGTH)

	boat.apply_force_at_local_point((sail_force + keel_force + hull_force).rotated(-boat.angle), (0, 0))

	rudder_force = compute_force(apparent_water, rudder_angle, RUDDER_STRENGTH)

	boat.apply_force_at_local_point(rudder_force.rotated(-boat.angle), (-30, 0))

	space.step(DT)


	screen.fill((30, 30, 40))

	center = pymunk.vec2d.Vec2d(WIDTH / 2, HEIGHT / 2)
	boat_points = [p.rotated(boat.angle) + center for p in shape.get_vertices()]
	pygame.draw.polygon(screen, (200, 200, 220), boat_points, 2)

	sail_dir_local = pymunk.vec2d.Vec2d(-1, 0).rotated(boat.angle + sail_angle)
	pygame.draw.line(screen, (200, 200, 220), center, center + sail_dir_local * 40, 2)

	# Rudder (visual) - relative to boat
	rudder_pos_local = pymunk.vec2d.Vec2d(-30, 0).rotated(boat.angle) + center
	rudder_dir_local = pymunk.vec2d.Vec2d(-1, 0).rotated(boat.angle + rudder_angle)
	pygame.draw.line(screen, (200, 200, 220), rudder_pos_local, rudder_pos_local + rudder_dir_local * 20, 2)

	# Velocit (visual)
	velocity_scale = 50.0

	wind_indicator_pos = pymunk.vec2d.Vec2d(80, 80)
	pygame.draw.line(screen, (100, 200, 255), wind_indicator_pos, wind_indicator_pos + PROPER_WIND * velocity_scale, 1)
	pygame.draw.circle(screen, (100, 200, 255), wind_indicator_pos, 5)

	apparent_wind_indicator_pos = center + pymunk.vec2d.Vec2d(-80, 80)
	apparent_water_indicator_pos = center + pymunk.vec2d.Vec2d(80, 80)
	pygame.draw.line(screen, (255, 255, 255), apparent_wind_indicator_pos, apparent_wind_indicator_pos + apparent_wind * velocity_scale, 2)
	pygame.draw.circle(screen, (255, 255, 255), apparent_wind_indicator_pos, 5)
	pygame.draw.line(screen, (50, 50, 255), apparent_water_indicator_pos, apparent_water_indicator_pos + apparent_water * velocity_scale, 2)
	pygame.draw.circle(screen, (50, 50, 255), apparent_water_indicator_pos, 5)

	pygame.draw.line(screen, (100, 255, 100), center, center + boat.velocity * velocity_scale, 1)

	# Forces (visual)
	force_scale = 50.0
	pygame.draw.line(screen, (255, 100, 100), center, center + sail_force * force_scale, 2)
	pygame.draw.line(screen, (100, 255, 100), center, center + keel_force * force_scale, 2)
	pygame.draw.line(screen, (100, 100, 100), center, center + hull_force * force_scale, 2)
	pygame.draw.line(screen, (255, 255, 100), center, center + (sail_force + keel_force + hull_force) * force_scale, 4)
	pygame.draw.line(screen, (100, 100, 255), rudder_pos_local, rudder_pos_local + rudder_force * force_scale, 2)



	
	pygame.display.flip()
	clock.tick(60)

pygame.quit()
