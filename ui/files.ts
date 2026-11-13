// Files in and out. In: device models for the viewer and the triangles the
// core analyzes, in meters as glTF defines; STL has no unit of its own, and CAD tools write
// millimeters. Out: printable parts as binary STL.

import * as THREE from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { DRACO_GLTF_CONFIG, DRACOLoader } from 'three/examples/jsm/loaders/DRACOLoader.js';
import { STLLoader } from 'three/examples/jsm/loaders/STLLoader.js';
import { STLExporter } from 'three/examples/jsm/exporters/STLExporter.js';
import type { PartMesh } from './engine/worker.ts';
import type { Size } from './studio.svelte.ts';

export const MODEL_FILES = '.glb,.stl';

/** STLs and boxes come without a look of their own: they get the white, matte plastic of UniFi's models. */
const PLASTIC = new THREE.MeshStandardMaterial({ color: 0xf2f2f2, roughness: 0.7, metalness: 0 });

const gltf = new GLTFLoader().setDRACOLoader(new DRACOLoader().setDecoderPath(DRACO_GLTF_CONFIG));

interface LoadedModel {
  scene: THREE.Object3D;
  /** World-space triangles in meters, flat xyz per vertex. */
  positions: Float32Array;
}

export async function loadModel(name: string, data: ArrayBuffer): Promise<LoadedModel> {
  let scene: THREE.Object3D;
  if (name.toLowerCase().endsWith('.stl')) {
    const mesh = new THREE.Mesh(new STLLoader().parse(data), PLASTIC);
    mesh.scale.setScalar(0.001);
    scene = new THREE.Group().add(mesh);
  } else {
    scene = (await gltf.parseAsync(data, '')).scene;
  }
  scene.updateMatrixWorld(true);
  const out: number[] = [];
  const v = new THREE.Vector3();
  scene.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (!mesh.isMesh) return;
    const pos = mesh.geometry.getAttribute('position');
    const idx = mesh.geometry.getIndex();
    const n = idx ? idx.count : pos.count;
    for (let i = 0; i < n; i++) {
      v.fromBufferAttribute(pos, idx ? idx.getX(i) : i).applyMatrix4(mesh.matrixWorld);
      out.push(v.x, v.y, v.z);
    }
  });
  if (!out.length) throw new Error('This file has no triangles.');
  return { scene, positions: new Float32Array(out) };
}

/** A box device as the viewer shows it, in the device frame: centered, on y = 0, front at z = 0. */
export function boxScene([w, h, d]: Size): THREE.Object3D {
  const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), PLASTIC);
  mesh.position.set(0, h / 2, -d / 2);
  return new THREE.Group().add(mesh);
}

export function stl({ positions, indices }: PartMesh): Uint8Array {
  const geometry = new THREE.BufferGeometry().setAttribute('position', new THREE.BufferAttribute(positions, 3));
  const view = new STLExporter().parse(new THREE.Mesh(geometry.setIndex(new THREE.BufferAttribute(indices, 1))), { binary: true });
  return new Uint8Array(view.buffer);
}

export function download(name: string, bytes: Uint8Array) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([bytes as BlobPart]));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

export const slug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
