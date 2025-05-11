import * as THREE from 'three';

export function createLight(): THREE.DirectionalLight {
    const dirLight = new THREE.DirectionalLight( 0xffffff, 5 );

    dirLight.position.set( - 2, 5, - 3 );
    dirLight.castShadow = true;
    let cam = dirLight.shadow.camera;
    cam.top = cam.right = 2;
    cam.bottom = cam.left = - 2;
    cam.near = 3;
    cam.far = 8;
    dirLight.shadow.bias = -0.005;
    dirLight.shadow.radius = 4;

    return dirLight;
}