import * as THREE from 'three';

import { GLTF, GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { GUI } from 'three/addons/libs/lil-gui.module.min.js';
import { Context } from '../scene';
import { PlayerState } from '@binding/PlayerState';

interface ActionDictionary {
    [key: string]: THREE.AnimationAction; 
}

const loader = new GLTFLoader();
const PI = Math.PI;

// Animation + Movement parameters
const FADE_DURATION = 0.5;
const RUN_VELOCITY = 5;
const WALK_VELOCITY = 1.8;
const ROTATE_SPEED = 0.05;

// Player instance sets this when it binds to the browser controls
let panel: GUI | null = null;

let settings = {
    show_skeleton:false,
    fixe_transition: true,
};

// Current key inputs
let key = [0, 0];
let ease = new THREE.Vector3();

export class Player {
    // State parameters, updated by either server messages or an animation timestep
    position = new THREE.Vector3();
    up = new THREE.Vector3(0, 1, 0);
    rotate = new THREE.Quaternion();
    movementState = 'Idle';

    // Is this this the local player that we need to focus on?
    tracking = false;

    // Tracks changes in movement state, useful for animation transitions
    movementStatePrevious = 'Idle';

    // Handle to the global context of our demo
    context: Context

    // Available once the GLTF model is loaded
    skeleton: THREE.SkeletonHelper | undefined = undefined;
    playerGroup: THREE.Group | undefined = undefined;
    mixer: THREE.AnimationMixer | undefined = undefined;
    actions: ActionDictionary | undefined = undefined;

    constructor(context: Context) {
        this.context = context;

        let modelReady = (gltf: GLTF) => {
            let model = gltf.scene;

            this.playerGroup = new THREE.Group();
            this.playerGroup.add( model );
            model.rotation.y = PI;
            this.playerGroup.rotation.y = PI;

            this.context.scene.add(this.playerGroup);

            model.traverse( function ( object ) {

                if ( object instanceof THREE.Mesh ){
                    if( object.name == 'vanguard_Mesh' ){
                        object.castShadow = true;
                        object.receiveShadow = true;
                        object.material.shadowSide = THREE.DoubleSide;
                        //object.material.envMapIntensity = 0.5;
                        object.material.metalness = 1.0;
                        object.material.roughness = 0.2;
                        object.material.color.set(1,1,1);
                        object.material.metalnessMap = object.material.map;
                    } else {
                        object.material.metalness = 1;
                        object.material.roughness = 0;
                        object.material.transparent = true;
                        object.material.opacity = 0.8;
                        object.material.color.set(1,1,1);
                    }
                }

            });

            //

            this.skeleton = new THREE.SkeletonHelper( model );
            this.skeleton.visible = false;
            this.context.scene.add( this.skeleton );

            //

            const animations = gltf.animations;

            this.mixer = new THREE.AnimationMixer( model );

            this.actions = {
                Idle: this.mixer.clipAction( animations[ 0 ] ),
                Walk: this.mixer.clipAction( animations[ 3 ] ),
                Run: this.mixer.clipAction( animations[ 1 ] )
            };

            for( let m in this.actions ){
                this.actions[m].enabled = true;
                this.actions[m].setEffectiveTimeScale( 1 );
                if(m!=='Idle') this.actions[m].setEffectiveWeight( 0 );
            }

            this.actions.Idle.play();
        };

        // Load the model and initialise
        loader.load( 'models/Soldier.glb', function ( gltf ) {
            modelReady(gltf);
        });
    }

    // Remove from the scene
    remove() {
        this.playerGroup?.removeFromParent();
        this.skeleton?.removeFromParent();
    }

    // Update the player in time
    update(dt: number) {
        if(this.mixer) {
            this.mixer.update( dt );
        }

        // Animation transition

        if(this.tracking) { // Local player input
            let active = key[0] === 0 && key[1] === 0 ? false : true;

            this.movementStatePrevious = this.movementState;
            this.movementState = active ? (key[2] ? 'Run' : 'Walk') : 'Idle';
        }

        if ( this.movementState != this.movementStatePrevious && this.actions ){
            const current = this.actions[this.movementState];
            const old = this.actions[this.movementStatePrevious];

            setWeight( current, 1.0 );
            old.fadeOut(FADE_DURATION);
            current.reset().fadeIn( FADE_DURATION).play();
        }

        // Move THREE js objects

        if ( this.movementState !== 'Idle' && this.playerGroup) {
            const velocity = this.movementState == 'Run' ? RUN_VELOCITY : WALK_VELOCITY;

            if (this.tracking) {
                // Local player, need to use the camera view
                const azimut = this.context.orbitControls.getAzimuthalAngle();

                // direction with key
                ease.set( key[1], 0, key[0] ).multiplyScalar( velocity * dt );

                // calculate camera direction
                let angle = unwrapRad( Math.atan2( ease.x, ease.z ) + azimut );
                this.rotate.setFromAxisAngle( this.up, angle );
                
                // apply camera angle on ease
                ease.applyAxisAngle( this.up, azimut );

                this.position.add( ease );
                this.context.camera.position.add( ease );

                this.context.orbitControls.target.copy( this.position ).add({x:0, y:1, z:0});
                this.context.followGroup.position.copy( this.position );

                // decale floor at infinie
                let dx = ( this.position.x - this.context.floor.position.x );
                let dz = ( this.position.z - this.context.floor.position.z );
                if( Math.abs(dx) > this.context.floorDecale ) this.context.floor.position.x += dx;
                if( Math.abs(dz) > this.context.floorDecale ) this.context.floor.position.z += dz;
            }

            this.playerGroup.position.copy( this.position );
            this.playerGroup.quaternion.rotateTowards( this.rotate, ROTATE_SPEED );

        }
    }

    // Update position, angle etc.
    setState(state: PlayerState) {
        this.movementStatePrevious = this.movementState;

        this.position.fromArray(state.position);
        this.up.fromArray(state.up);
        this.rotate.fromArray(state.rotate);
        this.movementState = state.movement_state;
    }

    // Retrieve player state
    getState(): PlayerState {
        return {
            position: this.position.toArray(),
            up: this.up.toArray(),
            rotate: this.rotate.toArray(),
            movement_state: this.movementState
        } 
    }

    // Bind the current player instance to the browser controls
    bindControls() {
        this.tracking = true;

        // Handle browser events
        {
            document.addEventListener( 'keydown', (event) => this.#onKeyDown(event) );
            document.addEventListener( 'keyup', (event) => this.#onKeyUp(event) );
        }

        // Create a GUI panel
        {
            if(panel != null) {
                panel.close();
                panel = null;
            }

            panel = new GUI( { width: 310 } );

            panel.add( settings, 'show_skeleton' ).onChange( (b) => { 
                if(this.skeleton) { this.skeleton.visible = b; }
            });
        }
    }

    #onKeyDown( event: KeyboardEvent ) {
        switch ( event.code ) {
            case 'ArrowUp': case 'KeyW': case 'KeyZ': key[0] = -1; break;
            case 'ArrowDown': case 'KeyS': key[0] = 1; break;
            case 'ArrowLeft': case 'KeyA': case 'KeyQ': key[1] = -1; break;
            case 'ArrowRight': case 'KeyD': key[1] = 1; break;
            case 'ShiftLeft' : case 'ShiftRight' : key[2] = 1; break;
        }
    }

    #onKeyUp( event: KeyboardEvent ) {
        switch ( event.code ) {
            case 'ArrowUp': case 'KeyW': case 'KeyZ': key[0] = key[0]<0 ? 0:key[0]; break;
            case 'ArrowDown': case 'KeyS': key[0] = key[0]>0 ? 0:key[0]; break;
            case 'ArrowLeft': case 'KeyA': case 'KeyQ': key[1] = key[1]<0 ? 0:key[1]; break;
            case 'ArrowRight': case 'KeyD': key[1] = key[1]>0 ? 0:key[1]; break;
            case 'ShiftLeft' : case 'ShiftRight' : key[2] = 0; break;
        }
    }

}

/*
    Misc functionality
*/

function setWeight( action: THREE.AnimationAction, weight: number ) {
    action.enabled = true;
    action.setEffectiveTimeScale( 1 );
    action.setEffectiveWeight( weight );
}

function unwrapRad(r: number) {
    return Math.atan2(Math.sin(r), Math.cos(r));
}