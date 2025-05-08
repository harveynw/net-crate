import * as THREE from 'three';

import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { RGBELoader } from 'three/addons/loaders/RGBELoader.js';
import { createFloor } from './mesh';
import { Player } from './player';
import { dataChannel } from '../network';
import { ClientMessage } from '@binding/ClientMessage';

let context: Context;
let players: Map<string, Player>;
let player: Player | undefined = undefined;

export function setup() {
    context = new Context();

    // Create player(s)
    player = new Player(context);
    player.bindControls(); // Set as the main player
    players = new Map<string, Player>();

    // Start demo
    let clock = new THREE.Clock();
    animate(context, clock);
}

export function addPlayer(id: string) {
    console.log("Added player with id=" + id);
    players.set(id, new Player(context));
}

export function removePlayer(id: string) {
    console.log("Removed player with id=" + id);
    let p = players.get(id);
    if (p) { p.remove(); }
    players.delete(id);
}

export function movePlayer(id: string, x: number, y: number, z: number) {
    let p = players.get(id);
    if(p) {
        p.setPosition(new THREE.Vector3(x, y, z));
    }
}

// Render loop body
function animate(context: Context, clock: THREE.Clock) {
    requestAnimationFrame( () => animate(context, clock) );

    let delta = clock.getDelta();

    players.forEach((p) => { p.update(delta); });
    if(player) {
        player.update(delta);
        if(dataChannel) {
            const clientMessage: ClientMessage = {
                Move: [player.position.x, player.position.y, player.position.z]
            };
            dataChannel.send(JSON.stringify(clientMessage));
        }
    }

    context.render();
}

/// Stuff specific to our demo that acts on the THREE js scene
export class Context {
    scene: THREE.Scene;
    renderer: THREE.WebGLRenderer;
    camera: THREE.PerspectiveCamera;
    orbitControls: OrbitControls;

    followGroup: THREE.Group;

    floor: THREE.Mesh;
    floorDecale: number = 0;

    constructor() {
        this.scene = new THREE.Scene();
        this.renderer = createRenderer();
        this.camera = createCamera(this.renderer);
        this.orbitControls = createOrbitControls(this.camera, this.renderer);

        this.scene.background = new THREE.Color( 0x5e5d5d );
        this.scene.fog = new THREE.Fog( 0x5e5d5d, 2, 20 );

                this.followGroup = new THREE.Group();
        this.scene.add(this.followGroup);

        /*const hemiLight = new THREE.HemisphereLight( 0xffffff, 0xb3602b, 0.5 );
        hemiLight.position.set( 0, 20, 0 );
        scene.add( hemiLight );*/

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
        this.followGroup.add( dirLight );
        this.followGroup.add( dirLight.target );

        // Environment
        new RGBELoader().load( 'textures/lobe.hdr', (texture) => {
            texture.mapping = THREE.EquirectangularReflectionMapping;
            this.scene.environment = texture;
            this.scene.environmentIntensity = 1.5;
        });

        // Floor
        let size = 50;
        let repeat = 16;

        this.floor = createFloor(this.renderer, size, repeat);
        this.scene.add( this.floor );

        this.floorDecale = (size / repeat) * 4;
    }

    render() {
        // Update camera first
        this.orbitControls.update();

        // Render to canvas
        this.renderer.render(this.scene, this.camera);
    }
}





function createRenderer(): THREE.WebGLRenderer {
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

function createCamera(renderer: THREE.WebGLRenderer): THREE.PerspectiveCamera {
    let camera = new THREE.PerspectiveCamera( 45, window.innerWidth / window.innerHeight, 0.1, 100 );

    camera.position.set( 0, 2, - 5 );
    //camera.lookAt( 0, 1, 0 );

    window.addEventListener('resize', () => {
        camera.aspect = window.innerWidth / window.innerHeight;
        camera.updateProjectionMatrix();
        renderer.setSize( window.innerWidth, window.innerHeight );
    });

    return camera;
}

function createOrbitControls(camera: THREE.PerspectiveCamera, renderer: THREE.WebGLRenderer): OrbitControls {
    let orbitControls = new OrbitControls( camera, renderer.domElement );
    orbitControls.target.set( 0, 1, 0 );
    orbitControls.enableDamping = true;
    orbitControls.enablePan = false;
    orbitControls.maxPolarAngle = (Math.PI/2) - 0.05;
    orbitControls.update();

    return orbitControls;
}


