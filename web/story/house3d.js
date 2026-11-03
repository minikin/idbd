// The hero house in three.js. Loaded only when WebGL works and the visitor has not asked
// for reduced motion. Sensor glows are additive sprites, so no post-processing is needed.

import * as THREE from "three";
import { SENSOR_POSITIONS, TRUSTED, glowRadius, trust } from "./house.js";

const COLORS = {
  floor: 0x101113,
  wall: 0x16181b,
  wallSide: 0x131417,
  block: 0x23262a,
  door: 0x1a1c20,
  idbd: 0xffb547,
  ignored: 0x62676f,
};

/** House coordinates (x, y, z up) to three.js coordinates (x, y up, z). */
function toScene([x, y, z]) {
  return new THREE.Vector3(x, z, y);
}

function glowTexture() {
  const size = 128;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const context = canvas.getContext("2d");
  const gradient = context.createRadialGradient(size / 2, size / 2, 0, size / 2, size / 2, size / 2);
  gradient.addColorStop(0, "rgba(255,255,255,1)");
  gradient.addColorStop(0.25, "rgba(255,255,255,0.55)");
  gradient.addColorStop(1, "rgba(255,255,255,0)");
  context.fillStyle = gradient;
  context.fillRect(0, 0, size, size);
  return new THREE.CanvasTexture(canvas);
}

function box(size, position, color, edges) {
  const geometry = new THREE.BoxGeometry(...size);
  const mesh = new THREE.Mesh(geometry, new THREE.MeshStandardMaterial({ color, roughness: 0.9 }));
  mesh.position.set(...position);
  const lines = new THREE.LineSegments(
    new THREE.EdgesGeometry(geometry),
    new THREE.LineBasicMaterial({ color: 0xffffff, transparent: true, opacity: edges }),
  );
  mesh.add(lines);
  return mesh;
}

/**
 * Mounts the scene in `container`. Throws if WebGL is unavailable, so the caller can keep
 * the 2D drawing.
 */
export function mountHouse3d(container) {
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.append(renderer.domElement);

  const scene = new THREE.Scene();
  scene.add(new THREE.AmbientLight(0xffffff, 2.4));
  const sun = new THREE.DirectionalLight(0xffffff, 1.8);
  sun.position.set(6, 10, 8);
  scene.add(sun);

  scene.add(box([8, 0.1, 6], [4, -0.05, 3], COLORS.floor, 0.1));
  scene.add(box([8, 3.5, 0.1], [4, 1.75, -0.05], COLORS.wall, 0.1));
  scene.add(box([0.1, 3.5, 6], [-0.05, 1.75, 3], COLORS.wallSide, 0.1));
  scene.add(box([1.4, 1.1, 1.2], [7.1, 0.55, 0.8], COLORS.block, 0.08));
  scene.add(box([2.4, 0.8, 1.2], [2.2, 0.4, 5.0], COLORS.block, 0.08));
  scene.add(box([1.4, 0.9, 1.2], [4.9, 0.45, 3.0], COLORS.block, 0.08));
  scene.add(box([0.04, 2.5, 1.6], [0.03, 1.25, 3.8], COLORS.door, 0.16));
  const windowPane = new THREE.Mesh(
    new THREE.PlaneGeometry(2, 1.5),
    new THREE.MeshBasicMaterial({ color: COLORS.idbd, transparent: true, opacity: 0.12 }),
  );
  windowPane.position.set(3, 1.95, 0.02);
  scene.add(windowPane);

  const texture = glowTexture();
  const sensors = SENSOR_POSITIONS.map((position) => {
    const point = toScene(position);
    const dot = new THREE.Mesh(
      new THREE.SphereGeometry(0.06, 12, 12),
      new THREE.MeshBasicMaterial({ color: COLORS.ignored }),
    );
    dot.position.copy(point);
    const glow = new THREE.Sprite(
      new THREE.SpriteMaterial({
        map: texture,
        color: COLORS.ignored,
        transparent: true,
        opacity: 0.2,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      }),
    );
    glow.position.copy(point);
    scene.add(dot, glow);
    return { dot, glow };
  });

  const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 100);
  const target = new THREE.Vector3(4, 1.2, 3);
  const base = new THREE.Vector3(4 + 14, 1.4 + 11.5, 3 + 14);

  function resize() {
    const width = container.clientWidth || 1;
    const height = container.clientHeight || 1;
    renderer.setSize(width, height, false);
    const aspect = width / height;
    const half = 4.0;
    camera.left = -half * aspect;
    camera.right = half * aspect;
    camera.top = half;
    camera.bottom = -half;
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

  let frame = null;
  function draw(now) {
    frame = requestAnimationFrame(draw);
    if (!visible || document.hidden) {
      return;
    }
    const sway = Math.sin(now / 4000) * 0.9;
    camera.position.set(base.x + sway, base.y, base.z - sway);
    camera.lookAt(target);
    renderer.render(scene, camera);
  }
  frame = requestAnimationFrame(draw);

  return {
    /** Sets every glow from IDBD's step sizes. */
    update(alphas) {
      const t = trust(alphas);
      sensors.forEach(({ dot, glow }, i) => {
        const trusted = t[i] > TRUSTED;
        const color = trusted ? COLORS.idbd : COLORS.ignored;
        dot.material.color.setHex(color);
        glow.material.color.setHex(color);
        glow.material.opacity = trusted ? 0.55 + 0.35 * t[i] : 0.22;
        const size = (glowRadius(alphas[i]) / 37) * 2.4;
        glow.scale.set(size, size, 1);
      });
    },
    dispose() {
      cancelAnimationFrame(frame);
      resizer.disconnect();
      watcher.disconnect();
      renderer.dispose();
      renderer.domElement.remove();
    },
  };
}
