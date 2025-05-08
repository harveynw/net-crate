import * as THREE from 'three';
import { loadFloorN, loadFloorT } from './texture';

const PI90 = Math.PI / 2;

export function createFloor(renderer: THREE.WebGLRenderer, size: number, repeat: number): THREE.Mesh {
    // Floor
    const floorT = loadFloorT(renderer, repeat);
    const floorN = loadFloorN(renderer, repeat);

    let mat = new THREE.MeshStandardMaterial( { map:floorT, normalMap:floorN, normalScale:new THREE.Vector2(0.5,0.5), color: 0x404040, depthWrite: false, roughness:0.85 } )

    let g = new THREE.PlaneGeometry( size, size, 50, 50 );
    g.rotateX( -PI90 );

    let floor = new THREE.Mesh( g, mat );
    floor.receiveShadow = true;

    // Light
    const bulbGeometry = new THREE.SphereGeometry( 0.05, 16, 8 );
    let bulbLight = new THREE.PointLight( 0xffee88, 2, 500, 2 );

    let bulbMat = new THREE.MeshStandardMaterial( { emissive: 0xffffee, emissiveIntensity: 1, color: 0x000000 } );
    bulbLight.add( new THREE.Mesh( bulbGeometry, bulbMat ) );
    bulbLight.position.set( 1, 0.1, -3 );
    bulbLight.castShadow = true;
    floor.add( bulbLight );

    return floor;
}