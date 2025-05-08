import * as THREE from 'three';

import { GLTF, GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { GUI } from 'three/addons/libs/lil-gui.module.min.js';
import { Context } from './scene';

interface ActionDictionary {
    [key: string]: THREE.AnimationAction; 
}

const loader = new GLTFLoader();
const PI = Math.PI;

// Player instance sets this when it binds to the browser controls
let panel: GUI | null = null;

let settings = {
    show_skeleton:false,
    fixe_transition: true,
};


export class Player {
    key = [0, 0];
    ease = new THREE.Vector3();
    position = new THREE.Vector3();
    up = new THREE.Vector3(0, 1, 0);
    rotate = new THREE.Quaternion();
    current = 'Idle';
    fadeDuration = 0.5;
    runVelocity = 5;
    walkVelocity = 1.8;
    rotateSpeed = 0.05;

    // Is this this the local player that we need to focus on?
    tracking = false;

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

        if(!this.tracking) {
            // Remote player : Just update position
            if(this.playerGroup) {
                this.playerGroup.position.copy( this.position );
            }
            return;
        }

        const azimut = this.context.orbitControls.getAzimuthalAngle();

        let active = this.key[0] === 0 && this.key[1] === 0 ? false : true;
        let play = active ? (this.key[2] ? 'Run' : 'Walk') : 'Idle';

        // change animation

        if ( this.current != play && this.actions ){
            const current = this.actions[play];
            const old = this.actions[this.current];
            this.current = play;

            if( settings.fixe_transition ){
                current.reset()
                current.weight = 1.0;
                current.stopFading()
                old.stopFading();
                // sycro if not idle
                if ( play !== 'Idle' ) current.time = old.time * ( current.getClip().duration / old.getClip().duration );
                old._scheduleFading( this.fadeDuration, old.getEffectiveWeight(), 0 );
                current._scheduleFading( this.fadeDuration, current.getEffectiveWeight(), 1 );	
                current.play();
            } else {
                setWeight( current, 1.0 );
                old.fadeOut(this.fadeDuration);
                current.reset().fadeIn( this.fadeDuration ).play();
            }

        }

        // move object

        if ( this.current !== 'Idle' && this.playerGroup) {
            // run/walk velocity
            let velocity = this.current == 'Run' ? this.runVelocity : this.walkVelocity;

            // direction with key
            this.ease.set( this.key[1], 0, this.key[0] ).multiplyScalar( velocity * dt );

            // calculate camera direction
            let angle = unwrapRad( Math.atan2( this.ease.x, this.ease.z ) + azimut );
            this.rotate.setFromAxisAngle( this.up, angle );
            
            // apply camera angle on ease
            this.ease.applyAxisAngle( this.up, azimut );

            this.position.add( this.ease );
            this.context.camera.position.add( this.ease );

            this.playerGroup.position.copy( this.position );
            this.playerGroup.quaternion.rotateTowards( this.rotate, this.rotateSpeed );

            this.context.orbitControls.target.copy( this.position ).add({x:0, y:1, z:0});
            this.context.followGroup.position.copy( this.position );

            // decale floor at infinie
            let dx = ( this.position.x - this.context.floor.position.x );
            let dz = ( this.position.z - this.context.floor.position.z );
            if( Math.abs(dx) > this.context.floorDecale ) this.context.floor.position.x += dx;
            if( Math.abs(dz) > this.context.floorDecale ) this.context.floor.position.z += dz;
        }
    }

    // Update the position
    setPosition(pos: THREE.Vector3) {
        this.position.copy(pos);
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
            panel.add( settings, 'fixe_transition' );
        }
    }

    #onKeyDown( event: KeyboardEvent ) {
        const key = this.key;
        switch ( event.code ) {
            case 'ArrowUp': case 'KeyW': case 'KeyZ': key[0] = -1; break;
            case 'ArrowDown': case 'KeyS': key[0] = 1; break;
            case 'ArrowLeft': case 'KeyA': case 'KeyQ': key[1] = -1; break;
            case 'ArrowRight': case 'KeyD': key[1] = 1; break;
            case 'ShiftLeft' : case 'ShiftRight' : key[2] = 1; break;
        }
    }

    #onKeyUp( event: KeyboardEvent ) {
        const key = this.key;
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