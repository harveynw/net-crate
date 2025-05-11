import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';


export function createRenderer(): THREE.WebGLRenderer {
    const container = document.getElementById( 'container' );
    if(!container) {
        throw new Error("Couldn't find DOM element");
    }

    let renderer = new THREE.WebGLRenderer( { antialias: true } );
    renderer.setPixelRatio( window.devicePixelRatio );
    renderer.setSize( window.innerWidth, window.innerHeight );
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 0.5;
    renderer.shadowMap.enabled = true;
    container.appendChild( renderer.domElement );

    return renderer;
}

export function createCamera(renderer: THREE.WebGLRenderer): THREE.PerspectiveCamera {
    let camera = new THREE.PerspectiveCamera( 45, window.innerWidth / window.innerHeight, 0.1, 100 );

    camera.position.set( 0, 2, - 5 );

    window.addEventListener('resize', () => {
        camera.aspect = window.innerWidth / window.innerHeight;
        camera.updateProjectionMatrix();
        renderer.setSize( window.innerWidth, window.innerHeight );
    });

    return camera;
}

export function createOrbitControls(camera: THREE.PerspectiveCamera, renderer: THREE.WebGLRenderer): OrbitControls {
    let orbitControls = new OrbitControls( camera, renderer.domElement );
    orbitControls.target.set( 0, 1, 0 );
    orbitControls.enableDamping = true;
    orbitControls.enablePan = false;
    orbitControls.maxPolarAngle = (Math.PI/2) - 0.05;
    orbitControls.update();

    return orbitControls;
}

