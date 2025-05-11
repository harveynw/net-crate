import * as THREE from 'three';

import * as ViewHelpers from './helpers/view.ts';
import * as LightHelpers from './helpers/light.ts';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { RGBELoader } from 'three/addons/loaders/RGBELoader.js';
import { createFloor } from './helpers/mesh';
import { Player } from './entity/player.ts';
import { dataChannel } from './network';
import { ClientMessage } from '@binding/ClientMessage';
import { PlayerState } from '@binding/PlayerState.ts';

// Three JS objects
let context: Context;
// Network players
let players: Map<string, Player>;
// Local 'controllable' player
let player: Player | undefined = undefined;

export function setupScene() {
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

    players.get(id)?.remove();
    players.delete(id);
}

export function updatePlayer(id: string, state: PlayerState) {
    players.get(id)?.setState(state);
}

// Render loop body
function animate(context: Context, clock: THREE.Clock) {
    requestAnimationFrame( () => animate(context, clock) );

    let delta = clock.getDelta();

    players.forEach((p) => { p.update(delta); });
    if(player) {
        player.update(delta);

        const clientUpdate: ClientMessage = { Update: player.getState() };
        dataChannel?.send(JSON.stringify(clientUpdate));
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
        this.renderer = ViewHelpers.createRenderer();
        this.camera = ViewHelpers.createCamera(this.renderer);
        this.orbitControls = ViewHelpers.createOrbitControls(this.camera, this.renderer);

        this.scene.background = new THREE.Color( 0x5e5d5d );
        this.scene.fog = new THREE.Fog( 0x5e5d5d, 2, 20 );

        this.followGroup = new THREE.Group();
        this.scene.add(this.followGroup);

        let light = LightHelpers.createLight();
        this.followGroup.add( light );
        this.followGroup.add( light.target );

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





