//! Cena WebGL procedural: nenhum asset externo e RAF cancelável de verdade.
import * as THREE from "three";
import { createClient } from "../../packages/sdk/src/client";

const client = createClient();
if (location.protocol === "http:") client.enableHmr();
const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
renderer.setSize(window.innerWidth, window.innerHeight);
renderer.setClearColor(0x080d1f, 1);
document.body.appendChild(renderer.domElement);

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(50, window.innerWidth / window.innerHeight, 0.1, 100);
camera.position.z = 4;
// Dois cubos fora da faixa central: o cliente Wayland abre no meio e o juiz
// precisa ver a cena mesmo com um aplicativo por cima do centro da tela.
const geometry = new THREE.BoxGeometry(1.35, 1.35, 1.35);
const leftCube = new THREE.Mesh(geometry, new THREE.MeshNormalMaterial());
leftCube.position.x = -1.75;
const rightCube = new THREE.Mesh(geometry, new THREE.MeshNormalMaterial());
rightCube.position.x = 1.75;
scene.add(leftCube, rightCube);

let running = true;
let raf = 0;
let lastControlToken = "";
function report() {
  document.body.dataset.running = running ? "running" : "stopped";
  window.aero.postMessage({
    type: "guest-ready", id: window.aeroSurfaceId, framework: "three",
    running: running ? "running" : "stopped",
  });
}
function frame(time) {
  if (!running) return;
  leftCube.rotation.x = time * 0.0007;
  leftCube.rotation.y = time * 0.0011;
  rightCube.rotation.x = time * 0.0011;
  rightCube.rotation.y = -time * 0.0007;
  renderer.render(scene, camera);
  raf = requestAnimationFrame(frame);
}
function stop() {
  if (!running) return;
  running = false;
  cancelAnimationFrame(raf);
  renderer.render(scene, camera);
  report();
}
function start() {
  if (running) return;
  running = true;
  report();
  raf = requestAnimationFrame(frame);
}
window.__aeroThree = { start, stop, toggle: () => running ? stop() : start() };
const control = new BroadcastChannel("aero-three-control");
function handleControl(token) {
  if (!token || token === lastControlToken) return;
  lastControlToken = token;
  window.__aeroThree.toggle();
  report();
}
control.onmessage = (event) => handleControl(event.data);
window.addEventListener("storage", (event) => {
  if (event.key === "aero-three-control") handleControl(event.newValue);
});
// No nested, o bind pode chegar antes do widget; ouvir no fundo evita depender
// de BroadcastChannel entre WebViews.
client.subscribe((event) => {
  if ("KeyBind" in event && event.KeyBind.keys === "Super+4") {
    handleControl(`key-${Date.now()}`);
  }
});
window.addEventListener("resize", () => {
  camera.aspect = window.innerWidth / window.innerHeight;
  camera.updateProjectionMatrix();
  renderer.setSize(window.innerWidth, window.innerHeight);
  renderer.render(scene, camera);
});
await client.setInputRegion(null);
report();
raf = requestAnimationFrame(frame);
