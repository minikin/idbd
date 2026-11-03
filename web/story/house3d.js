// The hero house in three.js. Loaded only when WebGL works and the visitor has not asked
// for reduced motion. Each sensor is a small device whose light follows its live reading.
// IDBD's attention is an amber ring sized by the learned step size. The room acts out the
// three relevant sensors' current state: sunbeam, oven heat and the front door.

import * as THREE from "three";
import { SENSOR_COLORS, SENSOR_POSITIONS, glowRadius, trust } from "./house.js";

const COLORS = {
  floor: 0x121316,
  wall: 0x191b1f,
  wallSide: 0x15171a,
  block: 0x24272c,
  door: 0x2a2d33,
  idbd: 0xffb547,
  heat: 0xff7a45,
  sun: 0xffe08a,
};

/**
 * Where each label sits relative to its device, in CSS pixels, and which edge of the label
 * is anchored there: 0 for its left edge, 0.5 for its middle, 1 for its right edge.
 */
const LABEL_OFFSETS = [
  [18, 30, 0],
  [16, 10, 0],
  [10, 30, 0],
  [16, -26, 0],
  [0, -32, 0.5],
  [6, 56, 0],
];

/** Each sensor's own blink period in seconds, so they never pulse in step. */
const PERIODS = [3.1, 1.7, 2.3, 0.9, 2.9, 2.6];

/**
 * The 3D room is larger than the 8 x 6 floor of the shared sensor layout, so the devices
 * and their labels have room. Positions are stretched by these factors; furniture keeps
 * its size.
 */
const WIDTH = 11;
const DEPTH = 8;
const SX = WIDTH / 8;
const SZ = DEPTH / 6;

/** House coordinates (x, y, z up) to three.js coordinates (x, y up, z). */
function toScene([x, y, z]) {
  return new THREE.Vector3(x * SX, z, y * SZ);
}

function radialTexture(ring) {
  const size = 128;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext("2d");
  const c = size / 2;
  if (ring) {
    context.strokeStyle = "rgba(255,255,255,1)";
    context.lineWidth = 5;
    context.beginPath();
    context.arc(c, c, c - 6, 0, Math.PI * 2);
    context.stroke();
  } else {
    const gradient = context.createRadialGradient(c, c, 0, c, c, c);
    gradient.addColorStop(0, "rgba(255,255,255,1)");
    gradient.addColorStop(0.3, "rgba(255,255,255,0.5)");
    gradient.addColorStop(1, "rgba(255,255,255,0)");
    context.fillStyle = gradient;
    context.fillRect(0, 0, size, size);
  }
  return new THREE.CanvasTexture(canvas);
}

/** An additive sprite drawn over the walls, so a glow is never cut by a surface. */
function sprite(texture, color, opacity) {
  const node = new THREE.Sprite(
    new THREE.SpriteMaterial({
      map: texture,
      color,
      transparent: true,
      opacity,
      blending: THREE.AdditiveBlending,
      depthTest: false,
      depthWrite: false,
    }),
  );
  node.renderOrder = 10;
  return node;
}

function box(size, position, color, edges) {
  const geometry = new THREE.BoxGeometry(...size);
  const mesh = new THREE.Mesh(geometry, new THREE.MeshStandardMaterial({ color, roughness: 0.85 }));
  mesh.position.set(...position);
  mesh.add(
    new THREE.LineSegments(
      new THREE.EdgesGeometry(geometry),
      new THREE.LineBasicMaterial({ color: 0xffffff, transparent: true, opacity: edges }),
    ),
  );
  return mesh;
}

const lerp = (a, b, t) => a + (b - a) * t;

/**
 * Mounts the scene in `container` with one HTML label per sensor. Throws if WebGL is
 * unavailable, so the caller can keep the 2D drawing.
 */
export function mountHouse3d(container, names) {
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.append(renderer.domElement);

  const scene = new THREE.Scene();
  scene.add(new THREE.AmbientLight(0xffffff, 2.2));
  const key = new THREE.DirectionalLight(0xffffff, 1.6);
  key.position.set(6, 10, 8);
  scene.add(key);

  scene.add(box([WIDTH, 0.1, DEPTH], [WIDTH / 2, -0.05, DEPTH / 2], COLORS.floor, 0.1));
  scene.add(box([WIDTH, 3.5, 0.1], [WIDTH / 2, 1.75, -0.05], COLORS.wall, 0.1));
  scene.add(box([0.1, 3.5, DEPTH], [-0.05, 1.75, DEPTH / 2], COLORS.wallSide, 0.1));
  scene.add(box([1.4, 1.1, 1.2], [7.1 * SX, 0.55, 0.8 * SZ], COLORS.block, 0.08));
  scene.add(box([2.4, 0.8, 1.2], [2.6 * SX, 0.4, 5.0 * SZ], COLORS.block, 0.08));
  scene.add(box([1.4, 0.9, 1.2], [4.9 * SX, 0.45, 3.0 * SZ], COLORS.block, 0.08));

  const glow = radialTexture(false);
  const ringTexture = radialTexture(true);

  // The window and the patch of sunlight it throws on the floor.
  const pane = new THREE.Mesh(
    new THREE.PlaneGeometry(2, 1.5),
    new THREE.MeshBasicMaterial({ color: COLORS.sun, transparent: true, opacity: 0.2 }),
  );
  pane.position.set(3 * SX, 1.95, 0.02);
  scene.add(pane);
  const beam = new THREE.Mesh(
    new THREE.PlaneGeometry(2.4, 2.6),
    new THREE.MeshBasicMaterial({
      map: glow,
      color: COLORS.sun,
      transparent: true,
      opacity: 0.3,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    }),
  );
  beam.rotation.x = -Math.PI / 2;
  beam.position.set(3.4 * SX, 0.02, 1.7 * SZ);
  scene.add(beam);

  const heat = sprite(glow, COLORS.heat, 0.4);
  heat.position.set(7.1 * SX, 1.35, 0.8 * SZ);
  heat.scale.set(1.8, 1.8, 1);
  scene.add(heat);

  // The front door swings on a hinge at its back edge.
  const hinge = new THREE.Group();
  hinge.position.set(0.04, 0, 3.0 * SZ);
  hinge.add(box([0.05, 2.5, 1.6], [0, 1.25, 0.8], COLORS.door, 0.2));
  scene.add(hinge);

  // The water heater stands in a closed boiler room in the back corner: a tank behind two
  // glass partitions, so it heats water, not this room.
  const tank = new THREE.Mesh(
    new THREE.CylinderGeometry(0.42, 0.42, 1.6, 32),
    new THREE.MeshStandardMaterial({ color: 0x3a3f46, roughness: 0.5, metalness: 0.3 }),
  );
  tank.position.set(0.7 * SX, 0.8, 0.7 * SZ);
  scene.add(tank);
  const pilot = sprite(glow, 0x6ea8ff, 0.5);
  pilot.position.set(0.7 * SX + 0.3, 0.18, 0.7 * SZ + 0.3);
  pilot.scale.set(0.35, 0.35, 1);
  scene.add(pilot);
  const glass = new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.05, depthWrite: false });
  const partitionX = new THREE.Mesh(new THREE.PlaneGeometry(2.0, 2.4), glass);
  partitionX.position.set(2.0, 1.2, 1.0);
  partitionX.rotation.y = Math.PI / 2;
  const partitionZ = new THREE.Mesh(new THREE.PlaneGeometry(2.0, 2.4), glass);
  partitionZ.position.set(1.0, 1.2, 2.0);
  const closet = new THREE.LineSegments(
    new THREE.EdgesGeometry(new THREE.BoxGeometry(2.0, 2.4, 2.0)),
    new THREE.LineBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.14 }),
  );
  closet.position.set(1.0, 1.2, 1.0);
  scene.add(partitionX, partitionZ, closet);

  // Wi-fi rings keep pulsing outward: the signal wiggles whether or not it matters.
  const wifiAt = toScene(SENSOR_POSITIONS[3]);
  const waves = [0, 1, 2].map(() => {
    const wave = sprite(ringTexture, new THREE.Color(SENSOR_COLORS[3]), 0);
    wave.position.copy(wifiAt);
    scene.add(wave);
    return wave;
  });

  const layer = document.createElement("div");
  layer.className = "house-labels";
  layer.setAttribute("aria-hidden", "true");
  container.append(layer);

  const sensors = SENSOR_POSITIONS.map((position, i) => {
    const point = toScene(position);
    const color = new THREE.Color(SENSOR_COLORS[i]);
    const body = new THREE.Mesh(
      new THREE.BoxGeometry(0.22, 0.22, 0.22),
      new THREE.MeshStandardMaterial({ color: 0x30343b, roughness: 0.6 }),
    );
    body.position.copy(point);
    const led = new THREE.Mesh(new THREE.SphereGeometry(0.07, 16, 16), new THREE.MeshBasicMaterial({ color }));
    led.position.copy(point);
    const halo = sprite(glow, color, 0.5);
    halo.position.copy(point);
    const ring = sprite(ringTexture, new THREE.Color(COLORS.idbd), 0);
    ring.position.copy(point);
    scene.add(body, led, halo, ring);

    const label = document.createElement("span");
    label.className = "house-label";
    const dot = document.createElement("i");
    dot.style.background = SENSOR_COLORS[i];
    label.append(dot, names[i]);
    layer.append(label);

    return { point, led, halo, ring, label, flash: 0, attention: 0.5, size: 1, phase: i * 1.3 };
  });

  const state = { strong: [1, 1, 1], door: 0, sun: 1, oven: 1 };

  const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 100);
  const target = new THREE.Vector3(WIDTH / 2, 1.5, DEPTH / 2);
  const base = new THREE.Vector3(WIDTH / 2 + 16, 1.5 + 13, DEPTH / 2 + 16);
  const corners = [];
  for (const x of [0, WIDTH]) for (const y of [0, 3.5]) for (const z of [0, DEPTH]) corners.push(new THREE.Vector3(x, y, z));

  /** Fits the whole house in the frame, with a margin for the camera's sway. */
  function resize() {
    const width = container.clientWidth || 1;
    const height = container.clientHeight || 1;
    renderer.setSize(width, height, false);
    camera.position.copy(base);
    camera.lookAt(target);
    camera.updateMatrixWorld();
    let [minX, maxX, minY, maxY] = [Infinity, -Infinity, Infinity, -Infinity];
    for (const corner of corners) {
      const p = corner.clone().applyMatrix4(camera.matrixWorldInverse);
      minX = Math.min(minX, p.x);
      maxX = Math.max(maxX, p.x);
      minY = Math.min(minY, p.y);
      maxY = Math.max(maxY, p.y);
    }
    const margin = 1.1;
    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;
    let halfW = ((maxX - minX) / 2) * margin;
    let halfH = ((maxY - minY) / 2) * margin;
    const aspect = width / height;
    if (halfW / halfH > aspect) halfH = halfW / aspect;
    else halfW = halfH * aspect;
    camera.left = cx - halfW;
    camera.right = cx + halfW;
    camera.top = cy + halfH;
    camera.bottom = cy - halfH;
    camera.updateProjectionMatrix();
  }
  const resizer = new ResizeObserver(resize);
  resizer.observe(container);
  resize();

  let visible = true;
  const watcher = new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
  });
  watcher.observe(container);

  const screen = new THREE.Vector3();
  let frame = null;
  let last = performance.now();
  function draw(now) {
    frame = requestAnimationFrame(draw);
    if (!visible || document.hidden) return;
    const dt = Math.min(0.1, (now - last) / 1000);
    last = now;
    const t = now / 1000;
    const ease = 1 - Math.exp(-dt * 4);

    const sway = Math.sin(t / 5) * 0.6;
    camera.position.set(base.x + sway, base.y, base.z - sway);
    camera.lookAt(target);

    state.sun = lerp(state.sun, state.strong[0], ease);
    state.oven = lerp(state.oven, state.strong[1], ease);
    state.door = lerp(state.door, state.strong[2], ease);
    beam.material.opacity = 0.08 + 0.5 * state.sun;
    pane.material.opacity = 0.08 + 0.2 * state.sun;
    heat.material.opacity = (0.1 + 0.45 * state.oven) * (0.85 + 0.15 * Math.sin(t * 9));
    hinge.rotation.y = 1.05 * state.door;
    pilot.material.opacity = 0.35 + 0.2 * Math.sin(t * 7) * Math.sin(t * 2.3);

    waves.forEach((wave, k) => {
      const phase = (t / 1.6 + k / 3) % 1;
      const size = 0.3 + phase * 1.4;
      wave.scale.set(size, size, 1);
      wave.material.opacity = 0.5 * (1 - phase);
    });

    const width = container.clientWidth;
    const height = container.clientHeight;
    const placed = [];
    sensors.forEach((sensor, i) => {
      sensor.flash = lerp(sensor.flash, 0, dt * 3);
      const pulse = 0.5 + 0.5 * Math.sin((t / PERIODS[i]) * Math.PI * 2 + sensor.phase);
      const light = Math.min(1, 0.25 + 0.35 * pulse + sensor.flash);
      sensor.halo.material.opacity = 0.25 + 0.55 * light;
      const haloSize = 0.5 + 0.5 * light;
      sensor.halo.scale.set(haloSize, haloSize, 1);
      sensor.led.scale.setScalar(0.8 + 0.5 * light);
      sensor.ring.material.opacity = lerp(sensor.ring.material.opacity, 0.15 + 0.75 * sensor.attention, ease);
      const ringSize = lerp(sensor.ring.scale.x, sensor.size, ease);
      sensor.ring.scale.set(ringSize, ringSize, 1);

      screen.copy(sensor.point).project(camera);
      const [dx, dy, anchor] = LABEL_OFFSETS[i];
      const x = ((screen.x + 1) / 2) * width + dx;
      const y = ((1 - screen.y) / 2) * height + dy;
      const w = sensor.label.offsetWidth;
      const h = sensor.label.offsetHeight;
      const left = Math.max(4, Math.min(width - w - 4, x - anchor * w));
      const top = Math.max(4, Math.min(height - h - 4, y - h / 2));
      placed.push({ label: sensor.label, left, top, w, h });
    });
    // Labels that would overlap are pushed down, top to bottom, until none touch.
    placed.sort((a, b) => a.top - b.top);
    placed.forEach((box, i) => {
      for (const other of placed.slice(0, i)) {
        const overlaps =
          box.left < other.left + other.w + 4 &&
          other.left < box.left + box.w + 4 &&
          box.top < other.top + other.h + 4;
        if (overlaps) box.top = other.top + other.h + 4;
      }
      box.label.style.transform = `translate(${box.left}px, ${Math.min(box.top, height - box.h - 4)}px)`;
    });
    placed.length = 0;
    renderer.render(scene, camera);
  }
  frame = requestAnimationFrame(draw);

  return {
    /** Takes IDBD's step sizes, the latest readings and which relevant sensors are strong. */
    update({ alphas, features, strong }) {
      const t = trust(alphas);
      sensors.forEach((sensor, i) => {
        sensor.attention = t[i];
        sensor.size = (glowRadius(alphas[i]) / 37) * 1.6;
        sensor.flash = Math.min(1, Math.abs(features[i]) / 2.5);
      });
      state.strong = strong.map(Number);
    },
    dispose() {
      cancelAnimationFrame(frame);
      resizer.disconnect();
      watcher.disconnect();
      renderer.dispose();
      renderer.domElement.remove();
      layer.remove();
    },
  };
}
